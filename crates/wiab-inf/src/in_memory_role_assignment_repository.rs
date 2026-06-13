use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use wiab_core::access::{RoleAssignment, RoleAssignmentId, RoleAssignmentRepository};

#[derive(Clone, Default)]
pub struct InMemoryRoleAssignmentRepository {
    assignments: Arc<RwLock<HashMap<RoleAssignmentId, RoleAssignment>>>,
}

impl InMemoryRoleAssignmentRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RoleAssignmentRepository for InMemoryRoleAssignmentRepository {
    fn save(&self, assignment: RoleAssignment) {
        self.assignments
            .write()
            .expect("role assignment repository write lock poisoned")
            .insert(assignment.id(), assignment);
    }

    fn get(&self, id: &RoleAssignmentId) -> Option<RoleAssignment> {
        self.assignments
            .read()
            .expect("role assignment repository read lock poisoned")
            .get(id)
            .copied()
    }

    fn remove(&self, id: &RoleAssignmentId) -> bool {
        self.assignments
            .write()
            .expect("role assignment repository write lock poisoned")
            .remove(id)
            .is_some()
    }

    fn list(&self) -> Vec<RoleAssignment> {
        self.assignments
            .read()
            .expect("role assignment repository read lock poisoned")
            .values()
            .copied()
            .collect()
    }
}
