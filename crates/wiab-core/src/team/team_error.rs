use thiserror::Error;

use crate::team::TeamState;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TeamError {
    #[error("team name must be a non-empty trimmed string")]
    EmptyName,
    #[error("'{0}' is not a valid team id")]
    InvalidTeamId(String),
    #[error("'{0}' is not a valid team state")]
    InvalidTeamState(String),
    /// Carries the current state so a caller is told *why* the transition was rejected.
    #[error("team is {0}, not stopped")]
    NotStopped(TeamState),
    #[error("team is {0}, not starting")]
    NotStarting(TeamState),
    #[error("team is {0}; it is not running")]
    NotRunning(TeamState),
    #[error("team is {0}, not paused")]
    NotPaused(TeamState),
}
