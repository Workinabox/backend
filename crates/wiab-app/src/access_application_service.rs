use std::sync::Arc;

use wiab_core::access::{
    Role, RoleAssignment, RoleAssignmentId, RoleAssignmentNumbering, RoleAssignmentRepository,
    RoleAssignmentSnapshot, Scope,
};
use wiab_core::repository::Version;
use wiab_core::user::{UserId, UserRepository};

use crate::access_requests::GrantRoleRequest;

/// Orchestrates granting, revoking, and listing role assignments. Holds the user
/// repository to verify the grantee exists.
pub struct AccessApplicationService<A: RoleAssignmentRepository, U: UserRepository> {
    assignment_repository: A,
    user_repository: U,
    numbering: Arc<dyn RoleAssignmentNumbering>,
}

impl<A: RoleAssignmentRepository, U: UserRepository> AccessApplicationService<A, U> {
    pub fn new(
        assignment_repository: A,
        user_repository: U,
        numbering: Arc<dyn RoleAssignmentNumbering>,
    ) -> Self {
        Self {
            assignment_repository,
            user_repository,
            numbering,
        }
    }

    /// Grants a role. `Ok(None)` when the grantee user does not exist.
    pub async fn grant(
        &self,
        request: GrantRoleRequest,
    ) -> anyhow::Result<Option<RoleAssignmentSnapshot>> {
        let user_id: UserId = request.user_id.parse()?;
        if self.user_repository.get(&user_id).await?.is_none() {
            return Ok(None);
        }
        let scope = Scope::parse(&request.scope_kind, &request.scope_id)?;
        let role: Role = request.role.parse()?;
        let assignment = RoleAssignment::new(self.numbering.next(), user_id, scope, role);
        let snapshot = assignment.snapshot();
        self.assignment_repository
            .save(assignment, Version::NEW)
            .await?;
        Ok(Some(snapshot))
    }

    /// Grants directly (no HTTP request), used by bootstrap seeding and agent provisioning.
    pub async fn grant_direct(
        &self,
        user_id: UserId,
        scope: Scope,
        role: Role,
    ) -> anyhow::Result<RoleAssignmentSnapshot> {
        let assignment = RoleAssignment::new(self.numbering.next(), user_id, scope, role);
        let snapshot = assignment.snapshot();
        self.assignment_repository
            .save(assignment, Version::NEW)
            .await?;
        Ok(snapshot)
    }

    pub async fn revoke(&self, assignment_id: &str) -> anyhow::Result<bool> {
        let id: RoleAssignmentId = assignment_id.parse()?;
        Ok(self.assignment_repository.remove(&id).await?)
    }

    /// Whether the user holds an Owner role anywhere — the bar for managing users and
    /// grants until finer per-scope management checks exist.
    pub async fn is_owner(&self, user: UserId) -> anyhow::Result<bool> {
        let assignments = self.assignment_repository.list().await?;
        Ok(assignments
            .into_iter()
            .any(|assignment| assignment.user_id() == user && assignment.role() == Role::Owner))
    }

    pub async fn list_assignments(&self) -> anyhow::Result<Vec<RoleAssignmentSnapshot>> {
        let mut assignments = self.assignment_repository.list().await?;
        assignments.sort_by_key(|assignment| assignment.id().number());
        Ok(assignments
            .iter()
            .map(|assignment| assignment.snapshot())
            .collect())
    }
}
