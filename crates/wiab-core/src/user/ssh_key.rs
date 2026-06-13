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

    /// Reconstitute an SSH key from persisted state (used by repository implementations).
    pub fn from_persistence(
        id: SshKeyId,
        label: String,
        openssh_public_key: String,
        fingerprint: String,
    ) -> SshKey {
        Self {
            id,
            label,
            openssh_public_key,
            fingerprint,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> SshKey {
        SshKey::new(
            SshKeyId::new(),
            "laptop".to_owned(),
            "ssh-ed25519 AAAA".to_owned(),
            "SHA256:abc".to_owned(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_blank_label_or_empty_key() {
        assert_eq!(
            SshKey::new(
                SshKeyId::new(),
                "  ".to_owned(),
                "k".to_owned(),
                "f".to_owned()
            )
            .unwrap_err(),
            UserError::EmptySshKeyLabel
        );
        assert_eq!(
            SshKey::new(
                SshKeyId::new(),
                "label".to_owned(),
                "  ".to_owned(),
                "f".to_owned()
            )
            .unwrap_err(),
            UserError::EmptySshKey
        );
    }

    #[test]
    fn exposes_fields_and_snapshot() {
        let key = key();
        assert_eq!(key.id(), key.id());
        assert_eq!(key.label(), "laptop");
        assert_eq!(key.openssh_public_key(), "ssh-ed25519 AAAA");
        assert_eq!(key.fingerprint(), "SHA256:abc");
        let snapshot = key.snapshot();
        assert_eq!(snapshot.label, "laptop");
        assert_eq!(snapshot.fingerprint, "SHA256:abc");
    }
}
