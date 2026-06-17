use thiserror::Error;

/// Failure parsing a [`Role`](crate::rbac::Role) from its string form.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoleError {
    #[error("'{0}' is not a valid role")]
    Invalid(String),
}
