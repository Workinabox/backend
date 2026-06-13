use crate::user::{SshKeyId, SshKeySnapshot, UserError};

/// An SSH public key registered to a `User`, used to authenticate git over SSH.
///
/// The `fingerprint` is computed by infrastructure (it needs crypto the domain doesn't
/// carry) and passed in; the domain stores and compares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKey {
    id: SshKeyId,
    label: String,
    openssh_public_key: String,
    fingerprint: String,
}

impl SshKey {
    pub fn new(
        id: SshKeyId,
        label: String,
        openssh_public_key: String,
        fingerprint: String,
    ) -> Result<Self, UserError> {
        if label.trim().is_empty() {
            return Err(UserError::EmptySshKeyLabel);
        }
        if openssh_public_key.trim().is_empty() {
            return Err(UserError::EmptySshKey);
        }
        Ok(Self {
            id,
            label,
            openssh_public_key,
            fingerprint,
        })
    }

    pub fn id(&self) -> SshKeyId {
        self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn openssh_public_key(&self) -> &str {
        &self.openssh_public_key
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn snapshot(&self) -> SshKeySnapshot {
        SshKeySnapshot {
            id: self.id.to_string(),
            label: self.label.clone(),
            fingerprint: self.fingerprint.clone(),
        }
    }
}
