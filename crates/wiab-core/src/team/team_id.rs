use std::fmt;
use std::str::FromStr;

use crate::team::TeamError;

/// Human-readable team identifier, rendered "TM-7".
///
/// The number is minted by the `TeamNumbering` seam at the application layer and passed
/// into `Team::new`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TeamId(u64);

impl TeamId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for TeamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TM-{}", self.0)
    }
}

impl FromStr for TeamId {
    type Err = TeamError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("TM-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(TeamId)
            .ok_or_else(|| TeamError::InvalidTeamId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_tm_prefix() {
        assert_eq!(TeamId::from_number(7).to_string(), "TM-7");
    }

    #[test]
    fn exposes_number() {
        assert_eq!(TeamId::from_number(9).number(), 9);
    }

    #[test]
    fn round_trips_through_string() {
        let id = TeamId::from_number(123);
        assert_eq!(id.to_string().parse::<TeamId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<TeamId>().unwrap_err(),
            TeamError::InvalidTeamId("42".to_owned())
        );
        assert!("TM-abc".parse::<TeamId>().is_err());
    }

    /// `T-` (task) is the prefix `TM-` could plausibly be confused with.
    #[test]
    fn rejects_a_task_id() {
        assert!("T-1".parse::<TeamId>().is_err());
    }
}
