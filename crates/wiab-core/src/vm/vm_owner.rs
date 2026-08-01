use std::fmt;
use std::str::FromStr;

use crate::agent::AgentId;
use crate::team::TeamId;
use crate::vm::VmError;

/// Who a VM was booted for.
///
/// Sandboxes are provisioned for two kinds of worker: an `Agent`, activated for one piece of
/// work, and a `Team`, started once and kept running. Both are identified by their own
/// prefixed id (`A-3`, `TM-1`), so the persisted form is just that string — the prefix says
/// which kind it is and no migration was needed to widen the column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VmOwner {
    Agent(AgentId),
    Team(TeamId),
}

impl fmt::Display for VmOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Agent(id) => write!(f, "{id}"),
            Self::Team(id) => write!(f, "{id}"),
        }
    }
}

impl FromStr for VmOwner {
    type Err = VmError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        // `TM-` first: it is the longer prefix, and neither parser accepts the other's ids.
        if let Ok(id) = value.parse::<TeamId>() {
            return Ok(Self::Team(id));
        }
        value
            .parse::<AgentId>()
            .map(Self::Agent)
            .map_err(|_| VmError::InvalidVmOwner(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_an_agent() {
        let owner = VmOwner::Agent(AgentId::from_number(3));
        assert_eq!(owner.to_string(), "A-3");
        assert_eq!("A-3".parse::<VmOwner>().unwrap(), owner);
    }

    #[test]
    fn round_trips_a_team() {
        let owner = VmOwner::Team(TeamId::from_number(1));
        assert_eq!(owner.to_string(), "TM-1");
        assert_eq!("TM-1".parse::<VmOwner>().unwrap(), owner);
    }

    #[test]
    fn rejects_anything_else() {
        // A work id is the kind of thing a caller could plausibly pass by mistake.
        assert_eq!(
            "W-1".parse::<VmOwner>().unwrap_err(),
            VmError::InvalidVmOwner("W-1".to_owned())
        );
        assert!("".parse::<VmOwner>().is_err());
    }
}
