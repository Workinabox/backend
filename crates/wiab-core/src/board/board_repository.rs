use crate::board::{Board, BoardId};
use crate::repository::{RepoError, SaveError, Version};

/// Port for persisting board aggregates. One repository per aggregate root.
#[allow(async_fn_in_trait)]
pub trait BoardRepository: Send + Sync + 'static {
    async fn save(&self, board: Board, expected: Version) -> Result<Version, SaveError>;
    async fn get(&self, id: &BoardId) -> Result<Option<(Board, Version)>, RepoError>;
    async fn list(&self) -> Result<Vec<Board>, RepoError>;
}
