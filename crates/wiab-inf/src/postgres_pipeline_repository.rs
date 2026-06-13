use deadpool_postgres::Pool;
use wiab_core::pipeline::{Pipeline, PipelineId, PipelineRepository};
use wiab_core::project::ProjectId;
use wiab_core::repository::{RepoError, SaveError, Version};

/// PostgreSQL-backed pipeline repository. One row per aggregate in `pipeline`,
/// guarded by an optimistic-concurrency `version` column.
#[derive(Clone)]
pub struct PostgresPipelineRepository {
    pool: Pool,
}

impl PostgresPipelineRepository {
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

impl PipelineRepository for PostgresPipelineRepository {
    async fn save(&self, pipeline: Pipeline, expected: Version) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = pipeline.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO pipeline (id, version, project_id, name, description) \
                     VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &pipeline.project_id().to_string(),
                        &pipeline.name(),
                        &pipeline.description(),
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE pipeline SET version = $2, project_id = $3, name = $4, \
                     description = $5 WHERE id = $1 AND version = $6",
                    &[
                        &id,
                        &next_version,
                        &pipeline.project_id().to_string(),
                        &pipeline.name(),
                        &pipeline.description(),
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

    async fn get(&self, id: &PipelineId) -> Result<Option<(Pipeline, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                "SELECT version, project_id, name, description FROM pipeline WHERE id = $1",
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
                let pipeline =
                    Pipeline::new(*id, project_id, name, description).map_err(repo_error)?;
                Ok(Some((pipeline, Version::from_value(version as u64))))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Pipeline>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(
                "SELECT id, project_id, name, description FROM pipeline",
                &[],
            )
            .await
            .map_err(repo_error)?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get(0);
                let id: PipelineId = id.parse().map_err(repo_error)?;
                let project_id: String = row.get(1);
                let project_id: ProjectId = project_id.parse().map_err(repo_error)?;
                let name: String = row.get(2);
                let description: String = row.get(3);
                Pipeline::new(id, project_id, name, description).map_err(repo_error)
            })
            .collect()
    }
}
