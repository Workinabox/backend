use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrganizationError {
    #[error("organization name must be a non-empty trimmed string")]
    EmptyName,
    #[error("'{0}' is not a valid organization id")]
    InvalidOrganizationId(String),
}
