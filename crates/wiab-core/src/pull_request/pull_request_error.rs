use thiserror::Error;

use crate::pull_request::PullRequestState;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PullRequestError {
    #[error("pull request title must be a non-empty trimmed string")]
    EmptyTitle,
    #[error("a pull request cannot target its own source branch '{0}'")]
    SameSourceAndTarget(String),
    #[error("'{0}' is not a valid pull request id")]
    InvalidPullRequestId(String),
    #[error("'{0}' is not a valid pull request state")]
    InvalidPullRequestState(String),
    /// Carries the current state so a caller is told *why* the transition was
    /// rejected, not merely that it was.
    #[error("pull request is {0}, not open")]
    NotOpen(PullRequestState),
}
