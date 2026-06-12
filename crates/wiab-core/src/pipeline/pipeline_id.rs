use std::fmt;
use std::str::FromStr;

use crate::pipeline::PipelineError;

/// Human-readable pipeline identifier, rendered "PL-7". The "PL-" prefix avoids
/// colliding with the project "P-" prefix.
///
/// The number is minted by the `PipelineNumbering` seam at the application layer and
/// passed into `Pipeline::new`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PipelineId(u64);

impl PipelineId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PL-{}", self.0)
    }
}

impl FromStr for PipelineId {
    type Err = PipelineError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("PL-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(PipelineId)
            .ok_or_else(|| PipelineError::InvalidPipelineId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_pl_prefix() {
        assert_eq!(PipelineId::from_number(7).to_string(), "PL-7");
    }

    #[test]
    fn exposes_number() {
        assert_eq!(PipelineId::from_number(9).number(), 9);
    }

    #[test]
    fn parses_valid_id() {
        assert_eq!(
            "PL-42".parse::<PipelineId>().unwrap(),
            PipelineId::from_number(42)
        );
    }

    #[test]
    fn round_trips_through_string() {
        let id = PipelineId::from_number(123);
        assert_eq!(id.to_string().parse::<PipelineId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<PipelineId>().unwrap_err(),
            PipelineError::InvalidPipelineId("42".to_owned())
        );
        assert_eq!(
            "PL-abc".parse::<PipelineId>().unwrap_err(),
            PipelineError::InvalidPipelineId("PL-abc".to_owned())
        );
        assert_eq!(
            "B-1".parse::<PipelineId>().unwrap_err(),
            PipelineError::InvalidPipelineId("B-1".to_owned())
        );
    }

    #[test]
    fn rejects_project_prefixed_id() {
        assert_eq!(
            "P-1".parse::<PipelineId>().unwrap_err(),
            PipelineError::InvalidPipelineId("P-1".to_owned())
        );
    }
}
