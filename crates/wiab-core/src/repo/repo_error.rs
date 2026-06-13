use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("repo name must be a non-empty trimmed string")]
    EmptyName,
    #[error("'{0}' is not a valid repo id")]
    InvalidRepoId(String),
    #[error("'{0}' is not a valid branch name")]
    InvalidBranchName(String),
    #[error("'{0}' is not a valid commit hash")]
    InvalidCommitHash(String),
    #[error("'{0}' is not a valid visibility")]
    InvalidVisibility(String),
}
