use deadpool_postgres::Pool;
use wiab_core::agent::{Agent, AgentId, AgentRepository};
use wiab_core::organization::OrganizationId;
use wiab_core::repository::{RepoError, SaveError, Version};

/// PostgreSQL-backed agent repository. One row per aggregate in `agent`,
/// guarded by an optimistic-concurrency `version` column.
#[derive(Clone)]
pub struct PostgresAgentRepository {
    pool: Pool,
}

impl PostgresAgentRepository {
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

impl AgentRepository for PostgresAgentRepository {
    async fn save(&self, agent: Agent, expected: Version) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = agent.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO agent (id, version, organization_id, name, description) \
                     VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &agent.organization_id().to_string(),
                        &agent.name(),
                        &agent.description(),
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE agent SET version = $2, organization_id = $3, name = $4, description = $5 \
                     WHERE id = $1 AND version = $6",
                    &[
                        &id,
                        &next_version,
                        &agent.organization_id().to_string(),
                        &agent.name(),
                        &agent.description(),
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

    async fn get(&self, id: &AgentId) -> Result<Option<(Agent, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                "SELECT version, organization_id, name, description FROM agent WHERE id = $1",
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
                let agent =
                    Agent::new(*id, organization_id, name, description).map_err(repo_error)?;
                Ok(Some((agent, Version::from_value(version as u64))))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Agent>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(
                "SELECT id, organization_id, name, description FROM agent",
                &[],
            )
            .await
            .map_err(repo_error)?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get(0);
                let id: AgentId = id.parse().map_err(repo_error)?;
                let organization_id: String = row.get(1);
                let organization_id: OrganizationId =
                    organization_id.parse().map_err(repo_error)?;
                let name: String = row.get(2);
                let description: String = row.get(3);
                Agent::new(id, organization_id, name, description).map_err(repo_error)
            })
            .collect()
    }
}
