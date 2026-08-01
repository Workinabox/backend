use thiserror::Error;

use crate::task::TaskState;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskError {
    #[error("'{0}' is not a valid task id")]
    InvalidTaskId(String),
    #[error("'{0}' is not a valid task state")]
    InvalidTaskState(String),
    /// Carries the current state so a caller is told *why* the transition was rejected.
    #[error("task is {0}; it is not waiting to be picked up")]
    NotAvailable(TaskState),
    #[error("task is {0}, not assigned")]
    NotAssigned(TaskState),
    #[error("task is {0}; no team is working on it")]
    NotInProgress(TaskState),
    #[error("task is {0}, not blocked")]
    NotBlocked(TaskState),
    #[error("a blocked, escalated or failed task must say why")]
    EmptyReason,
}
