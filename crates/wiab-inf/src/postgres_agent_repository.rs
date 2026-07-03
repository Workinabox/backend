use deadpool_postgres::Pool;
use wiab_core::agent::{Agent, AgentId, AgentRepository};
use wiab_core::organization::OrganizationId;
use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::vm::{VmId, VmTemplate};

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

/// Rebuild an `Agent` from a row's columns.
fn agent_from_columns(
    id: AgentId,
    organization_id: String,
    name: String,
    description: String,
    vm_template: Option<String>,
    active: bool,
    vm_id: Option<String>,
) -> Result<Agent, RepoError> {
    let organization_id: OrganizationId = organization_id.parse().map_err(repo_error)?;
    let vm_template = vm_template
        .map(VmTemplate::new)
        .transpose()
        .map_err(repo_error)?;
    let vm_id = vm_id
        .map(|v| v.parse::<VmId>())
        .transpose()
        .map_err(repo_error)?;
    Ok(Agent::from_parts(
        id,
        organization_id,
        name,
        description,
        vm_template,
        active,
        vm_id,
    ))
}

impl AgentRepository for PostgresAgentRepository {
    async fn save(&self, agent: Agent, expected: Version) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = agent.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let organization_id = agent.organization_id().to_string();
        let vm_template = agent.vm_template().map(|t| t.name().to_owned());
        let active = agent.is_active();
        let vm_id = agent.vm_id().map(|v| v.to_string());
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO agent \
                     (id, version, organization_id, name, description, vm_template, active, vm_id) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &organization_id,
                        &agent.name(),
                        &agent.description(),
                        &vm_template,
                        &active,
                        &vm_id,
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE agent SET version = $2, organization_id = $3, name = $4, \
                     description = $5, vm_template = $6, active = $7, vm_id = $8 \
                     WHERE id = $1 AND version = $9",
                    &[
                        &id,
                        &next_version,
                        &organization_id,
                        &agent.name(),
                        &agent.description(),
                        &vm_template,
                        &active,
                        &vm_id,
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
                "SELECT version, organization_id, name, description, vm_template, active, vm_id \
                 FROM agent WHERE id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        match row {
            None => Ok(None),
            Some(row) => {
                let version: i64 = row.get(0);
                let agent = agent_from_columns(
                    *id,
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                )?;
                Ok(Some((agent, Version::from_value(version as u64))))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Agent>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(
                "SELECT id, organization_id, name, description, vm_template, active, vm_id \
                 FROM agent",
                &[],
            )
            .await
            .map_err(repo_error)?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get(0);
                let id: AgentId = id.parse().map_err(repo_error)?;
                agent_from_columns(
                    id,
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                )
            })
            .collect()
    }
}
