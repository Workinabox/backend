use crate::project::{Project, ProjectId};
use crate::repository::{RepoError, SaveError, Version};

/// Port for persisting project aggregates. One repository per aggregate root.
///
/// Concurrency is optimistic: `get` returns the aggregate's current [`Version`], and `save`
/// is gated on the expected version, returning [`SaveError::Conflict`] when a concurrent
/// save has advanced it. A brand-new aggregate is saved with [`Version::NEW`].
#[allow(async_fn_in_trait)]
pub trait ProjectRepository: Send + Sync + 'static {
    async fn save(&self, project: Project, expected: Version) -> Result<Version, SaveError>;
    async fn get(&self, id: &ProjectId) -> Result<Option<(Project, Version)>, RepoError>;
    async fn list(&self) -> Result<Vec<Project>, RepoError>;
}
