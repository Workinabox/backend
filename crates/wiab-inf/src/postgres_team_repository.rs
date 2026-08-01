use deadpool_postgres::Pool;
use tokio_postgres::Row;
use wiab_core::board::BoardId;
use wiab_core::organization::OrganizationId;
use wiab_core::repo::RepoId;
use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::team::{Team, TeamId, TeamRepository, TeamState};
use wiab_core::vm::{VmId, VmTemplate};

/// PostgreSQL-backed team repository. One row per aggregate in `team`, guarded by an
/// optimistic-concurrency `version` column.
#[derive(Clone)]
pub struct PostgresTeamRepository {
    pool: Pool,
}

impl PostgresTeamRepository {
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

/// Rebuild from a row laid out as [`COLUMNS`]. Uses `from_persistence`, not `new`, because a
/// stored team may be in a state `new` cannot produce.
fn team_from_row(row: &Row) -> Result<Team, RepoError> {
    let id: String = row.get(0);
    let id: TeamId = id.parse().map_err(repo_error)?;
    let organization_id: String = row.get(1);
    let organization_id: OrganizationId = organization_id.parse().map_err(repo_error)?;
    let board_id: String = row.get(4);
    let board_id: BoardId = board_id.parse().map_err(repo_error)?;
    let repo_id: String = row.get(5);
    let repo_id: RepoId = repo_id.parse().map_err(repo_error)?;
    let vm_template: String = row.get(6);
    let vm_template = VmTemplate::new(vm_template).map_err(repo_error)?;
    let state: String = row.get(7);
    let state: TeamState = state.parse().map_err(repo_error)?;
    let vm_id: Option<String> = row.get(8);
    let vm_id = vm_id
        .map(|id| id.parse::<VmId>())
        .transpose()
        .map_err(repo_error)?;
    Ok(Team::from_persistence(
        id,
        organization_id,
        row.get(2),
        row.get(3),
        board_id,
        repo_id,
        vm_template,
        state,
        vm_id,
    ))
}

const COLUMNS: &str =
    "id, organization_id, name, description, board_id, repo_id, vm_template, state, vm_id";

impl TeamRepository for PostgresTeamRepository {
    async fn save(&self, team: Team, expected: Version) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = team.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let organization_id = team.organization_id().to_string();
        let board_id = team.board_id().to_string();
        let repo_id = team.repo_id().to_string();
        let vm_template = team.vm_template().to_string();
        let state = team.state().to_string();
        let vm_id = team.vm_id().map(|id| id.to_string());
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO team (id, version, organization_id, name, description, board_id, \
                     repo_id, vm_template, state, vm_id) \
                     VALUES ($1, $2, $3, $4, $5, $9, $10, $6, $7, $8) \
                     ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &organization_id,
                        &team.name(),
                        &team.description(),
                        &vm_template,
                        &state,
                        &vm_id,
                        &board_id,
                        &repo_id,
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE team SET version = $2, organization_id = $3, name = $4, \
                     description = $5, vm_template = $6, state = $7, vm_id = $8, \
                     board_id = $10, repo_id = $11 WHERE id = $1 AND version = $9",
                    &[
                        &id,
                        &next_version,
                        &organization_id,
                        &team.name(),
                        &team.description(),
                        &vm_template,
                        &state,
                        &vm_id,
                        &(expected.value() as i64),
                        &board_id,
                        &repo_id,
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

    async fn get(&self, id: &TeamId) -> Result<Option<(Team, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                &format!("SELECT {COLUMNS}, version FROM team WHERE id = $1"),
                &[&id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        match row {
            None => Ok(None),
            Some(row) => {
                let version: i64 = row.get(9);
                Ok(Some((
                    team_from_row(&row)?,
                    Version::from_value(version as u64),
                )))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Team>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(&format!("SELECT {COLUMNS} FROM team"), &[])
            .await
            .map_err(repo_error)?;
        rows.iter().map(team_from_row).collect()
    }
}
