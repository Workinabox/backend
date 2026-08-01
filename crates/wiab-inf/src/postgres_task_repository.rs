use deadpool_postgres::Pool;
use tokio_postgres::Row;
use wiab_core::board::BoardId;
use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::task::{Task, TaskId, TaskRepository, TaskState};
use wiab_core::team::TeamId;
use wiab_core::work::WorkId;

/// PostgreSQL-backed task repository. One row per aggregate in `task`, guarded by an
/// optimistic-concurrency `version` column.
#[derive(Clone)]
pub struct PostgresTaskRepository {
    pool: Pool,
}

impl PostgresTaskRepository {
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

/// Rebuild from a row laid out as `id, board_id, work_id, state, assignee, reason`. Uses
/// `from_persistence`, not `new`, because a stored task may be in a state `new` cannot
/// produce.
fn task_from_row(row: &Row) -> Result<Task, RepoError> {
    let id: String = row.get(0);
    let id: TaskId = id.parse().map_err(repo_error)?;
    let board_id: String = row.get(1);
    let board_id: BoardId = board_id.parse().map_err(repo_error)?;
    let work_id: String = row.get(2);
    let work_id: WorkId = work_id.parse().map_err(repo_error)?;
    let state: String = row.get(3);
    let state: TaskState = state.parse().map_err(repo_error)?;
    let assignee: Option<String> = row.get(4);
    let assignee = assignee
        .map(|id| id.parse::<TeamId>())
        .transpose()
        .map_err(repo_error)?;
    Ok(Task::from_persistence(
        id,
        board_id,
        work_id,
        state,
        assignee,
        row.get(5),
    ))
}

const COLUMNS: &str = "id, board_id, work_id, state, assignee, reason";

impl TaskRepository for PostgresTaskRepository {
    async fn save(&self, task: Task, expected: Version) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = task.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let board_id = task.board_id().to_string();
        let work_id = task.work_id().to_string();
        let state = task.state().to_string();
        let assignee = task.assignee().map(|id| id.to_string());
        let reason = task.reason().map(str::to_owned);
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO task \
                     (id, number, version, board_id, work_id, state, assignee, reason) \
                     VALUES ($1, $8, $2, $3, $4, $5, $6, $7) ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &board_id,
                        &work_id,
                        &state,
                        &assignee,
                        &reason,
                        &(task.id().number() as i64),
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE task SET version = $2, board_id = $3, work_id = $4, state = $5, \
                     assignee = $6, reason = $7 WHERE id = $1 AND version = $8",
                    &[
                        &id,
                        &next_version,
                        &board_id,
                        &work_id,
                        &state,
                        &assignee,
                        &reason,
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

    async fn get(&self, id: &TaskId) -> Result<Option<(Task, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                &format!("SELECT {COLUMNS}, version FROM task WHERE id = $1"),
                &[&id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        match row {
            None => Ok(None),
            Some(row) => {
                let version: i64 = row.get(6);
                Ok(Some((
                    task_from_row(&row)?,
                    Version::from_value(version as u64),
                )))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Task>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(&format!("SELECT {COLUMNS} FROM task"), &[])
            .await
            .map_err(repo_error)?;
        rows.iter().map(task_from_row).collect()
    }

    /// One transaction: lock the oldest available row on the board, then assign it.
    ///
    /// `FOR UPDATE SKIP LOCKED` is what makes concurrent polling safe — a second team asking
    /// at the same instant skips the row this transaction has locked and takes the next one,
    /// instead of blocking on it and then finding it already taken.
    async fn claim_next(
        &self,
        board_id: &BoardId,
        team_id: TeamId,
    ) -> Result<Option<(Task, Version)>, RepoError> {
        let mut client = self.pool.get().await.map_err(repo_error)?;
        let transaction = client.transaction().await.map_err(repo_error)?;
        let row = transaction
            .query_opt(
                &format!(
                    "SELECT {COLUMNS}, version FROM task \
                     WHERE board_id = $1 AND state IN ('created', 'escalated') \
                     ORDER BY number LIMIT 1 FOR UPDATE SKIP LOCKED"
                ),
                &[&board_id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        let Some(row) = row else {
            transaction.commit().await.map_err(repo_error)?;
            return Ok(None);
        };
        let mut task = task_from_row(&row)?;
        let version: i64 = row.get(6);
        task.assign(team_id).map_err(repo_error)?;
        let next = Version::from_value(version as u64).next();
        transaction
            .execute(
                "UPDATE task SET version = $2, state = $3, assignee = $4, reason = NULL \
                 WHERE id = $1",
                &[
                    &task.id().to_string(),
                    &(next.value() as i64),
                    &task.state().to_string(),
                    &team_id.to_string(),
                ],
            )
            .await
            .map_err(repo_error)?;
        transaction.commit().await.map_err(repo_error)?;
        Ok(Some((task, next)))
    }
}
