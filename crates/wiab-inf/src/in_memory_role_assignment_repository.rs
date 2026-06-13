use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use wiab_core::access::{RoleAssignment, RoleAssignmentId, RoleAssignmentRepository};
use wiab_core::repository::{RepoError, SaveError, Version};

#[derive(Clone, Default)]
pub struct InMemoryRoleAssignmentRepository {
    assignments: Arc<RwLock<HashMap<RoleAssignmentId, (RoleAssignment, u64)>>>,
}

impl InMemoryRoleAssignmentRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RoleAssignmentRepository for InMemoryRoleAssignmentRepository {
    async fn save(
        &self,
        assignment: RoleAssignment,
        expected: Version,
    ) -> Result<Version, SaveError> {
        let mut assignments = self
            .assignments
            .write()
            .expect("role assignment repository write lock poisoned");
        let current = assignments
            .get(&assignment.id())
            .map(|(_, version)| *version)
            .unwrap_or(0);
        if current != expected.value() {
            return Err(SaveError::Conflict);
        }
        let next = expected.next();
        assignments.insert(assignment.id(), (assignment, next.value()));
        Ok(next)
    }

    async fn get(
        &self,
        id: &RoleAssignmentId,
    ) -> Result<Option<(RoleAssignment, Version)>, RepoError> {
        Ok(self
            .assignments
            .read()
            .expect("role assignment repository read lock poisoned")
            .get(id)
            .map(|(assignment, version)| (*assignment, Version::from_value(*version))))
    }

    async fn remove(&self, id: &RoleAssignmentId) -> Result<bool, RepoError> {
        Ok(self
            .assignments
            .write()
            .expect("role assignment repository write lock poisoned")
            .remove(id)
            .is_some())
    }

    async fn list(&self) -> Result<Vec<RoleAssignment>, RepoError> {
        Ok(self
            .assignments
            .read()
            .expect("role assignment repository read lock poisoned")
            .values()
            .map(|(assignment, _)| *assignment)
            .collect())
    }
}
