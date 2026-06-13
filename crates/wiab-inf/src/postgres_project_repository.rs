use deadpool_postgres::Pool;
use wiab_core::organization::OrganizationId;
use wiab_core::project::{Project, ProjectId, ProjectRepository};
use wiab_core::repository::{RepoError, SaveError, Version};

/// PostgreSQL-backed project repository. One row per aggregate in `project`,
/// guarded by an optimistic-concurrency `version` column.
#[derive(Clone)]
pub struct PostgresProjectRepository {
    pool: Pool,
}

impl PostgresProjectRepository {
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

impl ProjectRepository for PostgresProjectRepository {
    async fn save(&self, project: Project, expected: Version) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = project.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO project (id, version, organization_id, name, description) \
                     VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &project.organization_id().to_string(),
                        &project.name(),
                        &project.description(),
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE project SET version = $2, organization_id = $3, name = $4, \
                     description = $5 WHERE id = $1 AND version = $6",
                    &[
                        &id,
                        &next_version,
                        &project.organization_id().to_string(),
                        &project.name(),
                        &project.description(),
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

    async fn get(&self, id: &ProjectId) -> Result<Option<(Project, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                "SELECT version, organization_id, name, description FROM project WHERE id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        match row {
            None => Ok(None),
            Some(row) => {
                let version: i64 = row.get(0);
                let organization_id: String = row.get(1);
                let organization_id: OrganizationId =
                    organization_id.parse().map_err(repo_error)?;
                let name: String = row.get(2);
                let description: String = row.get(3);
                let project =
                    Project::new(*id, organization_id, name, description).map_err(repo_error)?;
                Ok(Some((project, Version::from_value(version as u64))))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Project>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(
                "SELECT id, organization_id, name, description FROM project",
                &[],
            )
            .await
            .map_err(repo_error)?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get(0);
                let id: ProjectId = id.parse().map_err(repo_error)?;
                let organization_id: String = row.get(1);
                let organization_id: OrganizationId =
                    organization_id.parse().map_err(repo_error)?;
                let name: String = row.get(2);
                let description: String = row.get(3);
                Project::new(id, organization_id, name, description).map_err(repo_error)
            })
            .collect()
    }
}
