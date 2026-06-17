use std::fmt;
use std::str::FromStr;

use crate::rbac::{Operation, RoleError};

/// An access role, ordered Read < Write < Admin < Owner. A higher role includes the
/// powers of every lower one (the derived `Ord` follows declaration order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Read,
    Write,
    Admin,
    Owner,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Read => "read",
            Role::Write => "write",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }

    /// Whether this role is sufficient to perform `operation`.
    pub fn allows(&self, operation: Operation) -> bool {
        *self >= operation.required_role()
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Role {
    type Err = RoleError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "read" => Ok(Role::Read),
            "write" => Ok(Role::Write),
            "admin" => Ok(Role::Admin),
            "owner" => Ok(Role::Owner),
            other => Err(RoleError::Invalid(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_read_to_owner() {
        assert!(Role::Read < Role::Write);
        assert!(Role::Write < Role::Admin);
        assert!(Role::Admin < Role::Owner);
    }

    #[test]
    fn allows_follows_the_ladder() {
        assert!(Role::Write.allows(Operation::Read));
        assert!(Role::Write.allows(Operation::Write));
        assert!(!Role::Write.allows(Operation::Administer));
        assert!(Role::Owner.allows(Operation::Own));
        assert!(!Role::Read.allows(Operation::Write));
    }

    #[test]
    fn round_trips_through_string() {
        for role in [Role::Read, Role::Write, Role::Admin, Role::Owner] {
            assert_eq!(role.to_string().parse::<Role>().unwrap(), role);
        }
        assert!("god".parse::<Role>().is_err());
    }
}
