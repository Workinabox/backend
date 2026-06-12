use std::fmt;
use std::str::FromStr;

use crate::project::ProjectError;

/// Human-readable project identifier, rendered "P-7".
///
/// The number is minted by the `ProjectNumbering` seam at the application layer and
/// passed into `Project::new`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectId(u64);

impl ProjectId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P-{}", self.0)
    }
}

impl FromStr for ProjectId {
    type Err = ProjectError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("P-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(ProjectId)
            .ok_or_else(|| ProjectError::InvalidProjectId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_p_prefix() {
        assert_eq!(ProjectId::from_number(7).to_string(), "P-7");
    }

    #[test]
    fn exposes_number() {
        assert_eq!(ProjectId::from_number(9).number(), 9);
    }

    #[test]
    fn parses_valid_id() {
        assert_eq!(
            "P-42".parse::<ProjectId>().unwrap(),
            ProjectId::from_number(42)
        );
    }

    #[test]
    fn round_trips_through_string() {
        let id = ProjectId::from_number(123);
        assert_eq!(id.to_string().parse::<ProjectId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<ProjectId>().unwrap_err(),
            ProjectError::InvalidProjectId("42".to_owned())
        );
        assert_eq!(
            "P-abc".parse::<ProjectId>().unwrap_err(),
            ProjectError::InvalidProjectId("P-abc".to_owned())
        );
        assert_eq!(
            "O-1".parse::<ProjectId>().unwrap_err(),
            ProjectError::InvalidProjectId("O-1".to_owned())
        );
    }

    #[test]
    fn rejects_pipeline_prefixed_id() {
        assert_eq!(
            "PL-1".parse::<ProjectId>().unwrap_err(),
            ProjectError::InvalidProjectId("PL-1".to_owned())
        );
    }
}
