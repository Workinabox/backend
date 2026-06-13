use thiserror::Error;

/// Errors surfaced by the [`GitBackend`](crate::repo::GitBackend) port.
///
/// `Backend` carries the underlying implementation's error rendered as text, so the
/// domain stays free of any concrete git library type.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GitBackendError {
    #[error("no git repository exists for this repo")]
    RepoNotFound,
    #[error("branch '{0}' does not exist")]
    BranchNotFound(String),
    #[error("path '{0}' does not exist at the given ref")]
    PathNotFound(String),
    #[error("path '{0}' is not a file")]
    NotAFile(String),
    #[error("git backend error: {0}")]
    Backend(String),
}
