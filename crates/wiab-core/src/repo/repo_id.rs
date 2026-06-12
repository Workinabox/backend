use std::fmt;
use std::str::FromStr;

use crate::repo::RepoError;

/// Human-readable repo identifier, rendered "R-7".
///
/// The number is minted by the `RepoNumbering` seam at the application layer and
/// passed into `Repo::new`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RepoId(u64);

impl RepoId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for RepoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "R-{}", self.0)
    }
}

impl FromStr for RepoId {
    type Err = RepoError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("R-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(RepoId)
            .ok_or_else(|| RepoError::InvalidRepoId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_r_prefix() {
        assert_eq!(RepoId::from_number(7).to_string(), "R-7");
    }

    #[test]
    fn exposes_number() {
        assert_eq!(RepoId::from_number(9).number(), 9);
    }

    #[test]
    fn parses_valid_id() {
        assert_eq!("R-42".parse::<RepoId>().unwrap(), RepoId::from_number(42));
    }

    #[test]
    fn round_trips_through_string() {
        let id = RepoId::from_number(123);
        assert_eq!(id.to_string().parse::<RepoId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<RepoId>().unwrap_err(),
            RepoError::InvalidRepoId("42".to_owned())
        );
        assert_eq!(
            "R-abc".parse::<RepoId>().unwrap_err(),
            RepoError::InvalidRepoId("R-abc".to_owned())
        );
        assert_eq!(
            "P-1".parse::<RepoId>().unwrap_err(),
            RepoError::InvalidRepoId("P-1".to_owned())
        );
    }
}
