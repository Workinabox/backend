use std::fmt;
use std::str::FromStr;

use crate::user::UserError;

/// A user's lifecycle state.
///
/// `Pending` covers an invited user who hasn't accepted, or a just-signed-up user who
/// hasn't confirmed their email — only an `Active` user may authenticate. `Deactivated` is
/// a disabled account (kept for its history/role grants, but barred from logging in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserState {
    Active,
    Pending,
    Deactivated,
}

impl UserState {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserState::Active => "active",
            UserState::Pending => "pending",
            UserState::Deactivated => "deactivated",
        }
    }
}

impl fmt::Display for UserState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for UserState {
    type Err = UserError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(UserState::Active),
            "pending" => Ok(UserState::Pending),
            "deactivated" => Ok(UserState::Deactivated),
            other => Err(UserError::InvalidUserState(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_string() {
        for state in [
            UserState::Active,
            UserState::Pending,
            UserState::Deactivated,
        ] {
            assert_eq!(state.to_string().parse::<UserState>().unwrap(), state);
        }
        assert!("bogus".parse::<UserState>().is_err());
    }
}
