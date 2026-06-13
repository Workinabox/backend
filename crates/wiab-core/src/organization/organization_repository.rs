use crate::organization::{Organization, OrganizationId};
use crate::repository::{RepoError, SaveError, Version};

/// Port for persisting organization aggregates. One repository per aggregate root.
///
/// Concurrency is optimistic: `get` returns the aggregate's current [`Version`], and `save`
/// is gated on the expected version, returning [`SaveError::Conflict`] when a concurrent
/// save has advanced it. A brand-new aggregate is saved with [`Version::NEW`].
#[allow(async_fn_in_trait)]
pub trait OrganizationRepository: Send + Sync + 'static {
    async fn save(
        &self,
        organization: Organization,
        expected: Version,
    ) -> Result<Version, SaveError>;
    async fn get(&self, id: &OrganizationId) -> Result<Option<(Organization, Version)>, RepoError>;
    async fn list(&self) -> Result<Vec<Organization>, RepoError>;
}
