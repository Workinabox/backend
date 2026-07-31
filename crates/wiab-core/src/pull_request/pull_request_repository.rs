use crate::pull_request::{PullRequest, PullRequestId};
use crate::repository::{RepoError, SaveError, Version};

/// Port for persisting pull request aggregates. One repository per aggregate root.
///
/// Concurrency is optimistic: `get` returns the aggregate's current [`Version`], and `save`
/// is gated on the expected version, returning [`SaveError::Conflict`] when a concurrent
/// save has advanced it. A brand-new aggregate is saved with [`Version::NEW`].
#[allow(async_fn_in_trait)]
pub trait PullRequestRepository: Send + Sync + 'static {
    async fn save(
        &self,
        pull_request: PullRequest,
        expected: Version,
    ) -> Result<Version, SaveError>;
    async fn get(&self, id: &PullRequestId) -> Result<Option<(PullRequest, Version)>, RepoError>;
    async fn list(&self) -> Result<Vec<PullRequest>, RepoError>;
}
