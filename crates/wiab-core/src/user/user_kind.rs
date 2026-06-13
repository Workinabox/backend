use std::fmt;
use std::str::FromStr;

use crate::user::UserError;

/// Whether a user identity belongs to a human or an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserKind {
    Human,
    Agent,
}

impl UserKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserKind::Human => "human",
            UserKind::Agent => "agent",
        }
    }
}

impl fmt::Display for UserKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for UserKind {
    type Err = UserError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "human" => Ok(UserKind::Human),
            "agent" => Ok(UserKind::Agent),
            other => Err(UserError::InvalidUserKind(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        assert_eq!("human".parse::<UserKind>().unwrap(), UserKind::Human);
        assert_eq!(UserKind::Agent.to_string(), "agent");
        assert!("robot".parse::<UserKind>().is_err());
    }
}
