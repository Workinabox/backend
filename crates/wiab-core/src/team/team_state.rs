use std::fmt;
use std::str::FromStr;

use crate::team::TeamError;

/// Lifecycle of an agent team.
///
/// A team is a long-lived worker, not a per-issue process: it is started once, pulls
/// issues from the board one at a time, and keeps running between them.
///
/// `Stopped` is where a team begins and ends, and it is not terminal — a stopped team can
/// be started again. `Idle` means running with no issue in hand; `Working` means one is in
/// progress. `Paused` is reached by request and means the team will take no new issue; the
/// container stays up, so a paused team resumes without re-provisioning. `Failed` records a
/// team that could not be started at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamState {
    Stopped,
    Starting,
    Idle,
    Working,
    Paused,
    Failed,
}

impl TeamState {
    /// Whether the team has a container that should be running. Pausing does not stop the
    /// container — that is what makes resume cheap.
    pub fn is_provisioned(&self) -> bool {
        matches!(
            self,
            TeamState::Starting | TeamState::Idle | TeamState::Working | TeamState::Paused
        )
    }
}

impl fmt::Display for TeamState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            TeamState::Stopped => "stopped",
            TeamState::Starting => "starting",
            TeamState::Idle => "idle",
            TeamState::Working => "working",
            TeamState::Paused => "paused",
            TeamState::Failed => "failed",
        };
        f.write_str(text)
    }
}

impl FromStr for TeamState {
    type Err = TeamError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "stopped" => Ok(TeamState::Stopped),
            "starting" => Ok(TeamState::Starting),
            "idle" => Ok(TeamState::Idle),
            "working" => Ok(TeamState::Working),
            "paused" => Ok(TeamState::Paused),
            "failed" => Ok(TeamState::Failed),
            other => Err(TeamError::InvalidTeamState(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [TeamState; 6] = [
        TeamState::Stopped,
        TeamState::Starting,
        TeamState::Idle,
        TeamState::Working,
        TeamState::Paused,
        TeamState::Failed,
    ];

    #[test]
    fn round_trips_through_string() {
        for state in ALL {
            assert_eq!(state.to_string().parse::<TeamState>().unwrap(), state);
        }
    }

    #[test]
    fn a_paused_team_still_has_a_container() {
        // The whole point of pause: resume must not need re-provisioning.
        assert!(TeamState::Paused.is_provisioned());
    }

    #[test]
    fn only_the_settled_states_have_no_container() {
        for state in ALL {
            let expected = !matches!(state, TeamState::Stopped | TeamState::Failed);
            assert_eq!(state.is_provisioned(), expected, "{state}");
        }
    }

    #[test]
    fn rejects_unknown_state() {
        assert_eq!(
            "busy".parse::<TeamState>().unwrap_err(),
            TeamError::InvalidTeamState("busy".to_owned())
        );
    }
}
