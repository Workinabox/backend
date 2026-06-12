use std::fmt;
use std::str::FromStr;

use crate::agent::AgentError;

/// Human-readable agent identifier, rendered "A-7".
///
/// The number is minted by the `AgentNumbering` seam at the application layer and
/// passed into `Agent::new`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentId(u64);

impl AgentId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "A-{}", self.0)
    }
}

impl FromStr for AgentId {
    type Err = AgentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("A-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(AgentId)
            .ok_or_else(|| AgentError::InvalidAgentId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_a_prefix() {
        assert_eq!(AgentId::from_number(7).to_string(), "A-7");
    }

    #[test]
    fn exposes_number() {
        assert_eq!(AgentId::from_number(9).number(), 9);
    }

    #[test]
    fn parses_valid_id() {
        assert_eq!("A-42".parse::<AgentId>().unwrap(), AgentId::from_number(42));
    }

    #[test]
    fn round_trips_through_string() {
        let id = AgentId::from_number(123);
        assert_eq!(id.to_string().parse::<AgentId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<AgentId>().unwrap_err(),
            AgentError::InvalidAgentId("42".to_owned())
        );
        assert_eq!(
            "A-abc".parse::<AgentId>().unwrap_err(),
            AgentError::InvalidAgentId("A-abc".to_owned())
        );
        assert_eq!(
            "O-1".parse::<AgentId>().unwrap_err(),
            AgentError::InvalidAgentId("O-1".to_owned())
        );
    }
}
