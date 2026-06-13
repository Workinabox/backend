use deadpool_postgres::Pool;
use wiab_core::project::ProjectId;
use wiab_core::repo::{Repo, RepoId, RepoRepository, Visibility};
use wiab_core::repository::{RepoError, SaveError, Version};

/// PostgreSQL-backed repo repository. One row per aggregate in `repo`,
/// guarded by an optimistic-concurrency `version` column.
#[derive(Clone)]
pub struct PostgresRepoRepository {
    pool: Pool,
}

impl PostgresRepoRepository {
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

impl RepoRepository for PostgresRepoRepository {
    async fn save(&self, repo: Repo, expected: Version) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = repo.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let project_id = repo.project_id().to_string();
        let visibility = repo.visibility().to_string();
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO repo (id, version, project_id, name, description, visibility) \
                     VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &project_id,
                        &repo.name(),
                        &repo.description(),
                        &visibility,
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE repo SET version = $2, project_id = $3, name = $4, \
                     description = $5, visibility = $6 WHERE id = $1 AND version = $7",
                    &[
                        &id,
                        &next_version,
                        &project_id,
                        &repo.name(),
                        &repo.description(),
                        &visibility,
                        &(expected.value() as i64),
                    ],
                )
                .await
                .map_err(save_error)?
        };
        if rows == 0 {
            return Err(SaveError::Conflict);
        }
        Ok(next)
    }

    async fn get(&self, id: &RepoId) -> Result<Option<(Repo, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                "SELECT version, project_id, name, description, visibility FROM repo \
                 WHERE id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        match row {
            None => Ok(None),
            Some(row) => {
                let version: i64 = row.get(0);
                let project_id: String = row.get(1);
                let project_id: ProjectId = project_id.parse().map_err(repo_error)?;
                let name: String = row.get(2);
                let description: String = row.get(3);
                let visibility: String = row.get(4);
                let visibility: Visibility = visibility.parse().map_err(repo_error)?;
                let repo = Repo::new(*id, project_id, name, description, visibility)
                    .map_err(repo_error)?;
                Ok(Some((repo, Version::from_value(version as u64))))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Repo>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(
                "SELECT id, project_id, name, description, visibility FROM repo",
                &[],
            )
            .await
            .map_err(repo_error)?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get(0);
                let id: RepoId = id.parse().map_err(repo_error)?;
                let project_id: String = row.get(1);
                let project_id: ProjectId = project_id.parse().map_err(repo_error)?;
                let name: String = row.get(2);
                let description: String = row.get(3);
                let visibility: String = row.get(4);
                let visibility: Visibility = visibility.parse().map_err(repo_error)?;
                Repo::new(id, project_id, name, description, visibility).map_err(repo_error)
            })
            .collect()
    }
}
