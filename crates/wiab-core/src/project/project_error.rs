use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProjectError {
    #[error("project name must be a non-empty trimmed string")]
    EmptyName,
    #[error("'{0}' is not a valid project id")]
    InvalidProjectId(String),
}
