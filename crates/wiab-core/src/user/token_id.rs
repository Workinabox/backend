use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

use crate::user::UserError;

/// Stable identity for an access token owned by a `User`. UUID, like other owned entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenId(Uuid);

impl TokenId {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for TokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TokenId {
    type Err = UserError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value)
            .map(TokenId)
            .map_err(|_| UserError::InvalidTokenId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_string() {
        let id = TokenId::new();
        assert_eq!(id.to_string().parse::<TokenId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed() {
        assert!("not-a-uuid".parse::<TokenId>().is_err());
    }
}
