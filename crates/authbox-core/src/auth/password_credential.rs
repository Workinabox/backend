use std::fmt;
use std::str::FromStr;

use crate::auth::{AuthError, PrincipalId};

/// Lifecycle state of a password credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordState {
    Active,
    /// Set by an admin/reset flow: the credential exists but the user must set a new one.
    MustReset,
}

impl PasswordState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PasswordState::Active => "active",
            PasswordState::MustReset => "must_reset",
        }
    }
}

impl fmt::Display for PasswordState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PasswordState {
    type Err = AuthError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(PasswordState::Active),
            "must_reset" => Ok(PasswordState::MustReset),
            other => Err(AuthError::Backend(format!(
                "'{other}' is not a valid password state"
            ))),
        }
    }
}

/// A user's password credential: the argon2id PHC hash (salt + params embedded) plus its
/// lifecycle state. One per principal. The plaintext is never stored; verification goes
/// through the [`PasswordHasher`](crate::auth::PasswordHasher) seam in the app layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordCredential {
    principal: PrincipalId,
    phc_hash: String,
    state: PasswordState,
    updated_at: String,
}

impl PasswordCredential {
    pub fn new(principal: PrincipalId, phc_hash: String, updated_at: String) -> Self {
        Self {
            principal,
            phc_hash,
            state: PasswordState::Active,
            updated_at,
        }
    }

    pub fn from_persistence(
        principal: PrincipalId,
        phc_hash: String,
        state: PasswordState,
        updated_at: String,
    ) -> Self {
        Self {
            principal,
            phc_hash,
            state,
            updated_at,
        }
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }

    pub fn phc_hash(&self) -> &str {
        &self.phc_hash
    }

    pub fn state(&self) -> PasswordState {
        self.state
    }

    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }

    pub fn must_reset(&self) -> bool {
        self.state == PasswordState::MustReset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_credential_is_active() {
        let credential = PasswordCredential::new(
            PrincipalId::new("U-1"),
            "$argon2id$…".to_owned(),
            "2026-06-01T00:00:00Z".to_owned(),
        );
        assert_eq!(credential.state(), PasswordState::Active);
        assert!(!credential.must_reset());
        assert_eq!(credential.principal().as_str(), "U-1");
    }

    #[test]
    fn state_round_trips_through_string() {
        for state in [PasswordState::Active, PasswordState::MustReset] {
            assert_eq!(state.to_string().parse::<PasswordState>().unwrap(), state);
        }
        assert!("bogus".parse::<PasswordState>().is_err());
    }
}
