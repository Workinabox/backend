use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("repo name must be a non-empty trimmed string")]
    EmptyName,
    #[error("'{0}' is not a valid repo id")]
    InvalidRepoId(String),
}
