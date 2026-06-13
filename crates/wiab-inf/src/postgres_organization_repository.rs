use deadpool_postgres::Pool;
use wiab_core::organization::{Organization, OrganizationId, OrganizationRepository};
use wiab_core::repository::{RepoError, SaveError, Version};

/// PostgreSQL-backed organization repository. One row per aggregate in `organization`,
/// guarded by an optimistic-concurrency `version` column.
#[derive(Clone)]
pub struct PostgresOrganizationRepository {
    pool: Pool,
}

impl PostgresOrganizationRepository {
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

impl OrganizationRepository for PostgresOrganizationRepository {
    async fn save(
        &self,
        organization: Organization,
        expected: Version,
    ) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = organization.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO organization (id, version, name, description) \
                     VALUES ($1, $2, $3, $4) ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &organization.name(),
                        &organization.description(),
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE organization SET version = $2, name = $3, description = $4 \
                     WHERE id = $1 AND version = $5",
                    &[
                        &id,
                        &next_version,
                        &organization.name(),
                        &organization.description(),
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

    async fn get(&self, id: &OrganizationId) -> Result<Option<(Organization, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                "SELECT version, name, description FROM organization WHERE id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        match row {
            None => Ok(None),
            Some(row) => {
                let version: i64 = row.get(0);
                let name: String = row.get(1);
                let description: String = row.get(2);
                let organization = Organization::new(*id, name, description).map_err(repo_error)?;
                Ok(Some((organization, Version::from_value(version as u64))))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Organization>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query("SELECT id, name, description FROM organization", &[])
            .await
            .map_err(repo_error)?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get(0);
                let id: OrganizationId = id.parse().map_err(repo_error)?;
                let name: String = row.get(1);
                let description: String = row.get(2);
                Organization::new(id, name, description).map_err(repo_error)
            })
            .collect()
    }
}
