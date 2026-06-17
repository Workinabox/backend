use std::fmt;
use std::str::FromStr;

use crate::auth::{AuthError, PrincipalId};

/// What a verification token authorizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationPurpose {
    /// A forgotten-password reset link.
    PasswordReset,
    /// An admin invite — the recipient sets a password to finish setup.
    Invite,
    /// A signup email-confirmation link — the recipient already set a password.
    EmailVerify,
}

impl VerificationPurpose {
    pub fn as_str(&self) -> &'static str {
        match self {
            VerificationPurpose::PasswordReset => "password_reset",
            VerificationPurpose::Invite => "invite",
            VerificationPurpose::EmailVerify => "email_verify",
        }
    }
}

impl fmt::Display for VerificationPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for VerificationPurpose {
    type Err = AuthError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "password_reset" => Ok(VerificationPurpose::PasswordReset),
            "invite" => Ok(VerificationPurpose::Invite),
            "email_verify" => Ok(VerificationPurpose::EmailVerify),
            other => Err(AuthError::Backend(format!(
                "'{other}' is not a valid verification purpose"
            ))),
        }
    }
}

/// A single-use, expiring token tying a secret (stored only as its hash) to a principal and
/// a purpose. Issued for a password reset and consumed when the new password is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationToken {
    purpose: VerificationPurpose,
    token_hash: String,
    principal: PrincipalId,
    expires_at: String,
}

impl VerificationToken {
    pub fn new(
        purpose: VerificationPurpose,
        token_hash: String,
        principal: PrincipalId,
        expires_at: String,
    ) -> Self {
        Self {
            purpose,
            token_hash,
            principal,
            expires_at,
        }
    }

    pub fn purpose(&self) -> VerificationPurpose {
        self.purpose
    }

    pub fn token_hash(&self) -> &str {
        &self.token_hash
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }

    pub fn expires_at(&self) -> &str {
        &self.expires_at
    }

    pub fn is_expired(&self, now_rfc3339: &str) -> bool {
        now_rfc3339 >= self.expires_at.as_str()
    }
}
