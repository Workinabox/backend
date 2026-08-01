use crate::board::BoardId;
use crate::repository::{RepoError, SaveError, Version};
use crate::task::{Task, TaskId};
use crate::team::TeamId;

/// Persistence port for `Task`, plus the one query the board is *for*: taking the next
/// available task off it.
#[allow(async_fn_in_trait)]
pub trait TaskRepository: Send + Sync + 'static {
    async fn save(&self, task: Task, expected: Version) -> Result<Version, SaveError>;

    async fn get(&self, id: &TaskId) -> Result<Option<(Task, Version)>, RepoError>;

    async fn list(&self) -> Result<Vec<Task>, RepoError>;

    /// Atomically assign the oldest available task on `board_id` to `team_id`, returning it.
    /// `Ok(None)` when the board has nothing waiting.
    ///
    /// This is not `list` + `save`: two teams polling the same board at the same instant must
    /// not both come away with the same task, and an optimistic-concurrency retry would let
    /// the loser take a *different* task rather than fail. So the claim is one atomic
    /// operation in the adapter, not a read-modify-write in the application layer.
    async fn claim_next(
        &self,
        board_id: &BoardId,
        team_id: TeamId,
    ) -> Result<Option<(Task, Version)>, RepoError>;
}
