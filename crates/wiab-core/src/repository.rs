//! Shared persistence-port types: the optimistic-concurrency token and the error types
//! returned by every repository.
//!
//! Concurrency control is optimistic and lives in the persistence layer (not the domain):
//! `get` returns an aggregate's current [`Version`], `save` is gated on the expected
//! version and returns the next one, and a stale save yields [`SaveError::Conflict`] so the
//! caller can reload and retry. This mirrors a SQL `rowversion`/`WHERE version = ?` guard.

use thiserror::Error;

/// Optimistic-concurrency token for a persisted aggregate.
///
/// A brand-new aggregate that has never been persisted is saved with [`Version::NEW`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version(u64);

impl Version {
    /// The version of an aggregate that has not yet been persisted.
    pub const NEW: Version = Version(0);

    /// Rebuild a version from its stored value (used by repository implementations).
    pub fn from_value(value: u64) -> Self {
        Version(value)
    }

    /// The stored value, for persistence.
    pub fn value(self) -> u64 {
        self.0
    }

    /// The version a successful save advances to.
    pub fn next(self) -> Version {
        Version(self.0 + 1)
    }
}

/// Failure reading from a repository.
#[derive(Debug, Error)]
pub enum RepoError {
    #[error("repository backend error: {0}")]
    Backend(String),
}

/// Failure saving to a repository.
#[derive(Debug, Error)]
pub enum SaveError {
    /// The aggregate was modified concurrently — the expected version no longer matches.
    #[error("version conflict: the aggregate was modified concurrently")]
    Conflict,
    #[error("repository backend error: {0}")]
    Backend(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_version_starts_at_zero() {
        assert_eq!(Version::NEW.value(), 0);
    }

    #[test]
    fn next_of_new_is_one() {
        assert_eq!(Version::NEW.next().value(), 1);
    }

    #[test]
    fn next_increments() {
        assert_eq!(Version::from_value(41).next().value(), 42);
    }

    #[test]
    fn from_value_value_round_trip() {
        assert_eq!(Version::from_value(7).value(), 7);
    }

    #[test]
    fn repo_error_displays_backend() {
        assert_eq!(
            RepoError::Backend("boom".to_owned()).to_string(),
            "repository backend error: boom"
        );
    }

    #[test]
    fn save_error_displays_conflict_and_backend() {
        assert_eq!(
            SaveError::Conflict.to_string(),
            "version conflict: the aggregate was modified concurrently"
        );
        assert_eq!(
            SaveError::Backend("boom".to_owned()).to_string(),
            "repository backend error: boom"
        );
    }
}
