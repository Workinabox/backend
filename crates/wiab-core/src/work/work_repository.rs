use crate::repository::{RepoError, SaveError, Version};
use crate::work::{Work, WorkId};

/// Port for persisting work aggregates. One repository per aggregate root; the whole tree
/// persists as part of its root, so only root works are stored and listed.
#[allow(async_fn_in_trait)]
pub trait WorkRepository: Send + Sync + 'static {
    async fn save(&self, work: Work, expected: Version) -> Result<Version, SaveError>;
    async fn get(&self, id: &WorkId) -> Result<Option<(Work, Version)>, RepoError>;
    async fn list(&self) -> Result<Vec<Work>, RepoError>;
}
