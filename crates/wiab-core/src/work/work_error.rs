use thiserror::Error;

use crate::work::DoneId;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkError {
    #[error("work title must be a non-empty trimmed string")]
    EmptyTitle,
    #[error("done criterion must be a non-empty trimmed string")]
    EmptyCriterion,
    #[error("done '{0}' does not belong to this work")]
    DoneNotFound(DoneId),
    #[error("'{0}' is not a valid work id")]
    InvalidWorkId(String),
    #[error("'{0}' is not a valid done id")]
    InvalidDoneId(String),
}
