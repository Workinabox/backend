use std::sync::{Arc, Mutex};

use wiab_core::access::{
    Role, RoleAssignment, RoleAssignmentId, RoleAssignmentNumbering, RoleAssignmentRepository,
    RoleAssignmentSnapshot, Scope,
};
use wiab_core::user::{UserId, UserRepository};

use crate::access_requests::GrantRoleRequest;

/// Orchestrates granting, revoking, and listing role assignments. Holds the user
/// repository to verify the grantee exists.
pub struct AccessApplicationService<A: RoleAssignmentRepository, U: UserRepository> {
    assignment_repository: A,
    user_repository: U,
    numbering: Arc<dyn RoleAssignmentNumbering>,
    mutation_guard: Mutex<()>,
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
            mutation_guard: Mutex::new(()),
        }
    }

    /// Grants a role. `Ok(None)` when the grantee user does not exist.
    pub fn grant(
        &self,
        request: GrantRoleRequest,
    ) -> anyhow::Result<Option<RoleAssignmentSnapshot>> {
        let _guard = self.lock();
        let user_id: UserId = request.user_id.parse()?;
        if self.user_repository.get(&user_id).is_none() {
            return Ok(None);
        }
        let scope = Scope::parse(&request.scope_kind, &request.scope_id)?;
        let role: Role = request.role.parse()?;
        let assignment = RoleAssignment::new(self.numbering.next(), user_id, scope, role);
        let snapshot = assignment.snapshot();
        self.assignment_repository.save(assignment);
        Ok(Some(snapshot))
    }

    /// Grants directly (no HTTP request), used by bootstrap seeding and agent provisioning.
    pub fn grant_direct(
        &self,
        user_id: UserId,
        scope: Scope,
        role: Role,
    ) -> RoleAssignmentSnapshot {
        let _guard = self.lock();
        let assignment = RoleAssignment::new(self.numbering.next(), user_id, scope, role);
        let snapshot = assignment.snapshot();
        self.assignment_repository.save(assignment);
        snapshot
    }

    pub fn revoke(&self, assignment_id: &str) -> anyhow::Result<bool> {
        let _guard = self.lock();
        let id: RoleAssignmentId = assignment_id.parse()?;
        Ok(self.assignment_repository.remove(&id))
    }

    /// Whether the user holds an Owner role anywhere — the bar for managing users and
    /// grants until finer per-scope management checks exist.
    pub fn is_owner(&self, user: UserId) -> bool {
        self.assignment_repository
            .list()
            .into_iter()
            .any(|assignment| assignment.user_id() == user && assignment.role() == Role::Owner)
    }

    pub fn list_assignments(&self) -> Vec<RoleAssignmentSnapshot> {
        let mut assignments = self.assignment_repository.list();
        assignments.sort_by_key(|assignment| assignment.id().number());
        assignments
            .iter()
            .map(|assignment| assignment.snapshot())
            .collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_guard
            .lock()
            .expect("access mutation guard poisoned")
    }
}
