use crate::repository::{RepoError, SaveError, Version};
use crate::vm::{Vm, VmId};

/// Port for persisting microVM aggregates. One repository per aggregate root.
///
/// Concurrency is optimistic: `get` returns the aggregate's current [`Version`], and `save`
/// is gated on the expected version, returning [`SaveError::Conflict`] when a concurrent
/// save has advanced it. A brand-new aggregate is saved with [`Version::NEW`].
#[allow(async_fn_in_trait)]
pub trait VmRepository: Send + Sync + 'static {
    async fn save(&self, vm: Vm, expected: Version) -> Result<Version, SaveError>;
    async fn get(&self, id: &VmId) -> Result<Option<(Vm, Version)>, RepoError>;
    async fn list(&self) -> Result<Vec<Vm>, RepoError>;
}
