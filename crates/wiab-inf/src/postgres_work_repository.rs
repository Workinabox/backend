use deadpool_postgres::Pool;
use wiab_core::project::ProjectId;
use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::work::{Done, DoneId, Work, WorkId, WorkRepository};

/// PostgreSQL-backed work repository. One row per aggregate in `work`, guarded by an
/// optimistic-concurrency `version` column; the owned `dones` collection lives in the
/// `work_done` child table, rewritten wholesale on every save inside the same transaction.
#[derive(Clone)]
pub struct PostgresWorkRepository {
    pool: Pool,
}

impl PostgresWorkRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn repo_error<E: std::fmt::Display>(error: E) -> RepoError {
    RepoError::Backend(error.to_string())
}

fn save_error<E: std::fmt::Display>(error: E) -> SaveError {
    SaveError::Backend(error.to_string())
}

impl WorkRepository for PostgresWorkRepository {
    async fn save(&self, work: Work, expected: Version) -> Result<Version, SaveError> {
        let mut client = self.pool.get().await.map_err(save_error)?;
        let tx = client.transaction().await.map_err(save_error)?;

        let id = work.id().to_string();
        let project_id = work.project_id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;

        let rows = if expected == Version::NEW {
            tx.execute(
                "INSERT INTO work (id, version, project_id, title, description) \
                 VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO NOTHING",
                &[
                    &id,
                    &next_version,
                    &project_id,
                    &work.title(),
                    &work.description(),
                ],
            )
            .await
            .map_err(save_error)?
        } else {
            tx.execute(
                "UPDATE work SET version = $2, project_id = $3, title = $4, description = $5 \
                 WHERE id = $1 AND version = $6",
                &[
                    &id,
                    &next_version,
                    &project_id,
                    &work.title(),
                    &work.description(),
                    &(expected.value() as i64),
                ],
            )
            .await
            .map_err(save_error)?
        };
        if rows == 0 {
            // The transaction drops here without commit, rolling back any changes.
            return Err(SaveError::Conflict);
        }

        tx.execute("DELETE FROM work_done WHERE work_id = $1", &[&id])
            .await
            .map_err(save_error)?;
        for (position, done) in work.dones().iter().enumerate() {
            tx.execute(
                "INSERT INTO work_done (work_id, position, done_id, criterion, fulfilled) \
                 VALUES ($1, $2, $3, $4, $5)",
                &[
                    &id,
                    &(position as i32),
                    &done.id().to_string(),
                    &done.criterion(),
                    &done.is_fulfilled(),
                ],
            )
            .await
            .map_err(save_error)?;
        }

        tx.commit().await.map_err(save_error)?;
        Ok(next)
    }

    async fn get(&self, id: &WorkId) -> Result<Option<(Work, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let id_str = id.to_string();
        let row = client
            .query_opt(
                "SELECT version, project_id, title, description FROM work WHERE id = $1",
                &[&id_str],
            )
            .await
            .map_err(repo_error)?;
        match row {
            None => Ok(None),
            Some(row) => {
                let version: i64 = row.get(0);
                let project_id: String = row.get(1);
                let project_id: ProjectId = project_id.parse().map_err(repo_error)?;
                let title: String = row.get(2);
                let description: String = row.get(3);
                let dones = load_dones(&client, &id_str).await?;
                let work = Work::from_persistence(*id, project_id, title, description, dones);
                Ok(Some((work, Version::from_value(version as u64))))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Work>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query("SELECT id, project_id, title, description FROM work", &[])
            .await
            .map_err(repo_error)?;
        let mut works = Vec::with_capacity(rows.len());
        for row in rows {
            let id_str: String = row.get(0);
            let id: WorkId = id_str.parse().map_err(repo_error)?;
            let project_id: String = row.get(1);
            let project_id: ProjectId = project_id.parse().map_err(repo_error)?;
            let title: String = row.get(2);
            let description: String = row.get(3);
            let dones = load_dones(&client, &id_str).await?;
            works.push(Work::from_persistence(
                id,
                project_id,
                title,
                description,
                dones,
            ));
        }
        Ok(works)
    }
}

/// Load and rebuild the `dones` collection for a single work, ordered by position.
async fn load_dones(
    client: &deadpool_postgres::Client,
    work_id: &str,
) -> Result<Vec<Done>, RepoError> {
    let rows = client
        .query(
            "SELECT done_id, criterion, fulfilled FROM work_done \
             WHERE work_id = $1 ORDER BY position",
            &[&work_id],
        )
        .await
        .map_err(repo_error)?;
    rows.into_iter()
        .map(|row| {
            let done_id: String = row.get(0);
            let done_id: DoneId = done_id.parse().map_err(repo_error)?;
            let criterion: String = row.get(1);
            let fulfilled: bool = row.get(2);
            Ok(Done::from_persistence(done_id, criterion, fulfilled))
        })
        .collect()
}
