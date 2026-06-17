use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

use crate::auth::AuthError;

/// Stable identity for a session. UUID, like other owned-entity ids. Note the cookie value
/// is a separate high-entropy secret (only its hash is stored); the id is not a credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(Uuid);

impl SessionId {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SessionId {
    type Err = AuthError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value)
            .map(SessionId)
            .map_err(|_| AuthError::InvalidSessionId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_string() {
        let id = SessionId::new();
        assert_eq!(id.to_string().parse::<SessionId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed() {
        assert!("not-a-uuid".parse::<SessionId>().is_err());
    }
}
