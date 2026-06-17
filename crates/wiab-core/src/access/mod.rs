mod access_error;
mod access_policy;
mod role_assignment;
mod role_assignment_id;
mod role_assignment_numbering;
mod role_assignment_repository;
mod role_assignment_snapshot;
mod scope;

// The generic role ladder and RBAC primitives live in `authbox-core` so they can be
// reused across products; WIAB re-exports them here and layers its own scope model
// (`Scope`, `effective_role`) on top.
pub use authbox_core::{Operation, ResourceRef, Role};

pub use access_error::AccessError;
pub use access_policy::effective_role;
pub use role_assignment::RoleAssignment;
pub use role_assignment_id::RoleAssignmentId;
pub use role_assignment_numbering::RoleAssignmentNumbering;
pub use role_assignment_repository::RoleAssignmentRepository;
pub use role_assignment_snapshot::RoleAssignmentSnapshot;
pub use scope::Scope;
