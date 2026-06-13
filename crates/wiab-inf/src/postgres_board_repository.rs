use deadpool_postgres::Pool;
use wiab_core::board::{Board, BoardId, BoardRepository};
use wiab_core::project::ProjectId;
use wiab_core::repository::{RepoError, SaveError, Version};

/// PostgreSQL-backed board repository. One row per aggregate in `board`,
/// guarded by an optimistic-concurrency `version` column.
#[derive(Clone)]
pub struct PostgresBoardRepository {
    pool: Pool,
}

impl PostgresBoardRepository {
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

impl BoardRepository for PostgresBoardRepository {
    async fn save(&self, board: Board, expected: Version) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = board.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO board (id, version, project_id, name, description) \
                     VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &board.project_id().to_string(),
                        &board.name(),
                        &board.description(),
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE board SET version = $2, project_id = $3, name = $4, description = $5 \
                     WHERE id = $1 AND version = $6",
                    &[
                        &id,
                        &next_version,
                        &board.project_id().to_string(),
                        &board.name(),
                        &board.description(),
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

    async fn get(&self, id: &BoardId) -> Result<Option<(Board, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                "SELECT version, project_id, name, description FROM board WHERE id = $1",
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
                let board = Board::new(*id, project_id, name, description).map_err(repo_error)?;
                Ok(Some((board, Version::from_value(version as u64))))
            }
        }
    }

    async fn list(&self) -> Result<Vec<Board>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query("SELECT id, project_id, name, description FROM board", &[])
            .await
            .map_err(repo_error)?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get(0);
                let id: BoardId = id.parse().map_err(repo_error)?;
                let project_id: String = row.get(1);
                let project_id: ProjectId = project_id.parse().map_err(repo_error)?;
                let name: String = row.get(2);
                let description: String = row.get(3);
                Board::new(id, project_id, name, description).map_err(repo_error)
            })
            .collect()
    }
}
