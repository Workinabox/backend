use crate::access::RoleAssignmentId;

/// Port that mints the next sequential `G-###` identifier (infrastructure seam).
pub trait RoleAssignmentNumbering: Send + Sync {
    fn next(&self) -> RoleAssignmentId;
}
