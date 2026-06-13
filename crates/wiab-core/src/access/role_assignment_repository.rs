use crate::access::{RoleAssignment, RoleAssignmentId};

/// Port for persisting role-assignment aggregates.
pub trait RoleAssignmentRepository: Send + Sync + 'static {
    fn save(&self, assignment: RoleAssignment);
    fn get(&self, id: &RoleAssignmentId) -> Option<RoleAssignment>;
    fn remove(&self, id: &RoleAssignmentId) -> bool;
    fn list(&self) -> Vec<RoleAssignment>;
}
