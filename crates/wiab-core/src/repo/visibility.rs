use std::fmt;
use std::str::FromStr;

use crate::repo::RepoError;

/// Whether a repo is readable without authentication. Public repos can be cloned/fetched
/// anonymously over HTTPS; private repos require an authorized credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Private,
    Public,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Private => "private",
            Visibility::Public => "public",
        }
    }

    pub fn is_public(&self) -> bool {
        matches!(self, Visibility::Public)
    }
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Visibility {
    type Err = RepoError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "private" => Ok(Visibility::Private),
            "public" => Ok(Visibility::Public),
            other => Err(RepoError::InvalidVisibility(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_defaults_private() {
        assert_eq!(Visibility::default(), Visibility::Private);
        assert_eq!("public".parse::<Visibility>().unwrap(), Visibility::Public);
        assert!(Visibility::Public.is_public());
        assert!("internal".parse::<Visibility>().is_err());
    }
}
