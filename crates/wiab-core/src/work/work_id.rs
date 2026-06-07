use std::fmt;
use std::str::FromStr;

use crate::work::WorkError;

/// Human-readable work identifier, rendered "W-7".
///
/// The number is minted by the `WorkNumbering` seam at the application layer and passed
/// into `Work::new`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkId(u64);

impl WorkId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for WorkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "W-{}", self.0)
    }
}

impl FromStr for WorkId {
    type Err = WorkError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("W-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(WorkId)
            .ok_or_else(|| WorkError::InvalidWorkId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_w_prefix() {
        assert_eq!(WorkId::from_number(7).to_string(), "W-7");
    }

    #[test]
    fn parses_valid_id() {
        assert_eq!("W-42".parse::<WorkId>().unwrap(), WorkId::from_number(42));
    }

    #[test]
    fn round_trips_through_string() {
        let id = WorkId::from_number(123);
        assert_eq!(id.to_string().parse::<WorkId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<WorkId>().unwrap_err(),
            WorkError::InvalidWorkId("42".to_owned())
        );
        assert_eq!(
            "W-abc".parse::<WorkId>().unwrap_err(),
            WorkError::InvalidWorkId("W-abc".to_owned())
        );
    }
}
