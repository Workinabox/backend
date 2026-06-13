use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

use crate::user::UserError;

/// Stable identity for an SSH key owned by a `User`. UUID, like other owned entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SshKeyId(Uuid);

impl SshKeyId {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for SshKeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SshKeyId {
    type Err = UserError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value)
            .map(SshKeyId)
            .map_err(|_| UserError::InvalidSshKeyId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_string() {
        let id = SshKeyId::new();
        assert_eq!(id.to_string().parse::<SshKeyId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed() {
        assert!("not-a-uuid".parse::<SshKeyId>().is_err());
    }
}
