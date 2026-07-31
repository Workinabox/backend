use std::fmt;
use std::str::FromStr;

use crate::pull_request::PullRequestError;

/// Human-readable pull request identifier, rendered "PR-7".
///
/// The number is minted by the `PullRequestNumbering` seam at the application layer and
/// passed into `PullRequest::open`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PullRequestId(u64);

impl PullRequestId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for PullRequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PR-{}", self.0)
    }
}

impl FromStr for PullRequestId {
    type Err = PullRequestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("PR-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(PullRequestId)
            .ok_or_else(|| PullRequestError::InvalidPullRequestId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_pr_prefix() {
        assert_eq!(PullRequestId::from_number(7).to_string(), "PR-7");
    }

    #[test]
    fn exposes_number() {
        assert_eq!(PullRequestId::from_number(9).number(), 9);
    }

    #[test]
    fn parses_valid_id() {
        assert_eq!(
            "PR-42".parse::<PullRequestId>().unwrap(),
            PullRequestId::from_number(42)
        );
    }

    #[test]
    fn round_trips_through_string() {
        let id = PullRequestId::from_number(123);
        assert_eq!(id.to_string().parse::<PullRequestId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<PullRequestId>().unwrap_err(),
            PullRequestError::InvalidPullRequestId("42".to_owned())
        );
        assert_eq!(
            "PR-abc".parse::<PullRequestId>().unwrap_err(),
            PullRequestError::InvalidPullRequestId("PR-abc".to_owned())
        );
    }

    /// `P-` (project) and `R-` (repo) are the two prefixes `PR-` could plausibly be
    /// confused with, so pin that they do not parse.
    #[test]
    fn rejects_the_neighbouring_id_prefixes() {
        assert!("P-1".parse::<PullRequestId>().is_err());
        assert!("R-1".parse::<PullRequestId>().is_err());
    }
}
