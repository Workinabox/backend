use std::fmt;
use std::str::FromStr;

use crate::board::BoardError;

/// Human-readable board identifier, rendered "B-7".
///
/// The number is minted by the `BoardNumbering` seam at the application layer and
/// passed into `Board::new`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoardId(u64);

impl BoardId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for BoardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B-{}", self.0)
    }
}

impl FromStr for BoardId {
    type Err = BoardError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("B-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(BoardId)
            .ok_or_else(|| BoardError::InvalidBoardId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_b_prefix() {
        assert_eq!(BoardId::from_number(7).to_string(), "B-7");
    }

    #[test]
    fn exposes_number() {
        assert_eq!(BoardId::from_number(9).number(), 9);
    }

    #[test]
    fn parses_valid_id() {
        assert_eq!("B-42".parse::<BoardId>().unwrap(), BoardId::from_number(42));
    }

    #[test]
    fn round_trips_through_string() {
        let id = BoardId::from_number(123);
        assert_eq!(id.to_string().parse::<BoardId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<BoardId>().unwrap_err(),
            BoardError::InvalidBoardId("42".to_owned())
        );
        assert_eq!(
            "B-abc".parse::<BoardId>().unwrap_err(),
            BoardError::InvalidBoardId("B-abc".to_owned())
        );
        assert_eq!(
            "P-1".parse::<BoardId>().unwrap_err(),
            BoardError::InvalidBoardId("P-1".to_owned())
        );
    }
}
