use std::fmt;
use std::str::FromStr;

use crate::user::UserError;

/// Human-readable user identifier, rendered "U-7". Minted by the `UserNumbering` seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(u64);

impl UserId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U-{}", self.0)
    }
}

impl FromStr for UserId {
    type Err = UserError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("U-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(UserId)
            .ok_or_else(|| UserError::InvalidUserId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_parses() {
        assert_eq!(UserId::from_number(7).to_string(), "U-7");
        assert_eq!(UserId::from_number(7).number(), 7);
        assert_eq!("U-42".parse::<UserId>().unwrap(), UserId::from_number(42));
    }

    #[test]
    fn rejects_malformed() {
        assert!("42".parse::<UserId>().is_err());
        assert!("A-1".parse::<UserId>().is_err());
    }
}
