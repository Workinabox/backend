use crate::access::{RoleAssignment, RoleAssignmentId};
use crate::repository::{RepoError, SaveError, Version};

/// Port for persisting role-assignment aggregates.
#[allow(async_fn_in_trait)]
pub trait RoleAssignmentRepository: Send + Sync + 'static {
    async fn save(
        &self,
        assignment: RoleAssignment,
        expected: Version,
    ) -> Result<Version, SaveError>;
    async fn get(
        &self,
        id: &RoleAssignmentId,
    ) -> Result<Option<(RoleAssignment, Version)>, RepoError>;
    async fn remove(&self, id: &RoleAssignmentId) -> Result<bool, RepoError>;
    async fn list(&self) -> Result<Vec<RoleAssignment>, RepoError>;
}
