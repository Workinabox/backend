use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AccessError {
    #[error("'{0}' is not a valid role assignment id")]
    InvalidRoleAssignmentId(String),
    #[error("'{0}' is not a valid scope")]
    InvalidScope(String),
}
