use std::fmt;
use std::str::FromStr;

use crate::pull_request::PullRequestError;

/// Lifecycle of a pull request.
///
/// A newly opened request is `Open`; integrating the source branch into the target moves it
/// to `Merged`; abandoning it moves it to `Closed`. Both are terminal — a merged or closed
/// request is history, and a new request is opened rather than an old one resurrected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRequestState {
    Open,
    Merged,
    Closed,
}

impl PullRequestState {
    pub fn is_open(&self) -> bool {
        matches!(self, PullRequestState::Open)
    }
}

impl fmt::Display for PullRequestState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            PullRequestState::Open => "open",
            PullRequestState::Merged => "merged",
            PullRequestState::Closed => "closed",
        };
        f.write_str(text)
    }
}

impl FromStr for PullRequestState {
    type Err = PullRequestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "open" => Ok(PullRequestState::Open),
            "merged" => Ok(PullRequestState::Merged),
            "closed" => Ok(PullRequestState::Closed),
            other => Err(PullRequestError::InvalidPullRequestState(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_string() {
        for state in [
            PullRequestState::Open,
            PullRequestState::Merged,
            PullRequestState::Closed,
        ] {
            assert_eq!(
                state.to_string().parse::<PullRequestState>().unwrap(),
                state
            );
        }
    }

    #[test]
    fn only_open_is_open() {
        assert!(PullRequestState::Open.is_open());
        assert!(!PullRequestState::Merged.is_open());
        assert!(!PullRequestState::Closed.is_open());
    }

    #[test]
    fn rejects_unknown_state() {
        assert_eq!(
            "reopened".parse::<PullRequestState>().unwrap_err(),
            PullRequestError::InvalidPullRequestState("reopened".to_owned())
        );
    }
}
