use std::fmt;
use std::str::FromStr;

use crate::repo::RepoError;

/// A validated git branch name.
///
/// Kept free of characters that could escape a filesystem path or a subprocess
/// argument when the name is later handed to git. This is a safety gate, not a
/// full `git check-ref-format` implementation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchName(String);

impl BranchName {
    pub fn new(value: impl Into<String>) -> Result<Self, RepoError> {
        let value = value.into();
        if Self::is_valid(&value) {
            Ok(Self(value))
        } else {
            Err(RepoError::InvalidBranchName(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_valid(value: &str) -> bool {
        !value.is_empty()
            && !value.contains("..")
            && !value.starts_with('-')
            && !value.starts_with('/')
            && !value.ends_with('/')
            && !value.ends_with(".lock")
            && !value.contains(|c: char| c.is_ascii_control() || c.is_whitespace())
            && !value.contains(['~', '^', ':', '?', '*', '[', '\\'])
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for BranchName {
    type Err = RepoError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_names() {
        for name in ["main", "feature/login", "release-1.2", "user/fix_bug"] {
            assert_eq!(BranchName::new(name).unwrap().as_str(), name);
        }
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(
            BranchName::new("").unwrap_err(),
            RepoError::InvalidBranchName(String::new())
        );
    }

    #[test]
    fn rejects_traversal_and_leading_dash_or_slash() {
        for bad in ["..", "a..b", "-x", "/x", "x/"] {
            assert!(BranchName::new(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn rejects_control_whitespace_and_special_chars() {
        for bad in [
            "a b", "a\tb", "a\nb", "a~b", "a^b", "a:b", "a?b", "a*b", "a[b", "a\\b",
        ] {
            assert!(BranchName::new(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn parses_via_from_str() {
        assert_eq!("main".parse::<BranchName>().unwrap().as_str(), "main");
    }

    #[test]
    fn displays_inner_value() {
        assert_eq!(BranchName::new("main").unwrap().to_string(), "main");
    }
}
