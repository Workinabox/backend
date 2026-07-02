use std::fmt;
use std::str::FromStr;

use crate::vm::VmError;

/// Lifecycle of a microVM.
///
/// A freshly provisioned VM is `Creating`; once the runtime has booted it and reported an
/// endpoint it becomes `Running`; a graceful teardown moves it to `Stopped`; any launch
/// failure moves it to `Failed`. `Stopped` and `Failed` are terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmState {
    Creating,
    Running,
    Stopped,
    Failed,
}

impl fmt::Display for VmState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            VmState::Creating => "creating",
            VmState::Running => "running",
            VmState::Stopped => "stopped",
            VmState::Failed => "failed",
        };
        f.write_str(text)
    }
}

impl FromStr for VmState {
    type Err = VmError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "creating" => Ok(VmState::Creating),
            "running" => Ok(VmState::Running),
            "stopped" => Ok(VmState::Stopped),
            "failed" => Ok(VmState::Failed),
            other => Err(VmError::InvalidVmState(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_string() {
        for state in [
            VmState::Creating,
            VmState::Running,
            VmState::Stopped,
            VmState::Failed,
        ] {
            assert_eq!(state.to_string().parse::<VmState>().unwrap(), state);
        }
    }

    #[test]
    fn rejects_unknown_state() {
        assert_eq!(
            "paused".parse::<VmState>().unwrap_err(),
            VmError::InvalidVmState("paused".to_owned())
        );
    }
}
