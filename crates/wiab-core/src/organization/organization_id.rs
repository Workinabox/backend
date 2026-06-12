use std::fmt;
use std::str::FromStr;

use crate::organization::OrganizationError;

/// Human-readable organization identifier, rendered "O-7".
///
/// The number is minted by the `OrganizationNumbering` seam at the application layer and
/// passed into `Organization::new`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OrganizationId(u64);

impl OrganizationId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for OrganizationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "O-{}", self.0)
    }
}

impl FromStr for OrganizationId {
    type Err = OrganizationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("O-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(OrganizationId)
            .ok_or_else(|| OrganizationError::InvalidOrganizationId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_o_prefix() {
        assert_eq!(OrganizationId::from_number(7).to_string(), "O-7");
    }

    #[test]
    fn exposes_number() {
        assert_eq!(OrganizationId::from_number(9).number(), 9);
    }

    #[test]
    fn parses_valid_id() {
        assert_eq!(
            "O-42".parse::<OrganizationId>().unwrap(),
            OrganizationId::from_number(42)
        );
    }

    #[test]
    fn round_trips_through_string() {
        let id = OrganizationId::from_number(123);
        assert_eq!(id.to_string().parse::<OrganizationId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<OrganizationId>().unwrap_err(),
            OrganizationError::InvalidOrganizationId("42".to_owned())
        );
        assert_eq!(
            "O-abc".parse::<OrganizationId>().unwrap_err(),
            OrganizationError::InvalidOrganizationId("O-abc".to_owned())
        );
    }
}
