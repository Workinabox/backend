use std::fmt;
use std::str::FromStr;

use crate::access::AccessError;

/// Human-readable role-assignment (grant) identifier, rendered "G-7".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoleAssignmentId(u64);

impl RoleAssignmentId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for RoleAssignmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "G-{}", self.0)
    }
}

impl FromStr for RoleAssignmentId {
    type Err = AccessError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("G-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(RoleAssignmentId)
            .ok_or_else(|| AccessError::InvalidRoleAssignmentId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_parses() {
        assert_eq!(RoleAssignmentId::from_number(7).to_string(), "G-7");
        assert_eq!(RoleAssignmentId::from_number(7).number(), 7);
        assert_eq!(
            "G-42".parse::<RoleAssignmentId>().unwrap(),
            RoleAssignmentId::from_number(42)
        );
        assert!("R-1".parse::<RoleAssignmentId>().is_err());
    }
}
