use std::fmt;

use crate::repo::RepoError;

/// A validated git object hash: lowercase hex, 40 chars (sha1) or 64 (sha256).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitHash(String);

impl CommitHash {
    pub fn new(value: impl Into<String>) -> Result<Self, RepoError> {
        let value = value.into();
        if Self::is_valid(&value) {
            Ok(Self(value))
        } else {
            Err(RepoError::InvalidCommitHash(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_valid(value: &str) -> bool {
        (value.len() == 40 || value.len() == 64)
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }
}

impl fmt::Display for CommitHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn accepts_sha1_and_sha256() {
        assert_eq!(CommitHash::new(SHA1).unwrap().as_str(), SHA1);
        let sha256 = "a".repeat(64);
        assert_eq!(CommitHash::new(&sha256).unwrap().as_str(), sha256);
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(CommitHash::new("abc").is_err());
        assert!(CommitHash::new("a".repeat(41)).is_err());
    }

    #[test]
    fn rejects_uppercase_and_non_hex() {
        assert!(CommitHash::new("0123456789ABCDEF0123456789abcdef01234567").is_err());
        assert!(CommitHash::new("z".repeat(40)).is_err());
    }

    #[test]
    fn displays_inner_value() {
        assert_eq!(CommitHash::new(SHA1).unwrap().to_string(), SHA1);
    }
}
