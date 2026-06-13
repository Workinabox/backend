//! Infrastructure seams for credential crypto the domain can't carry itself (random
//! generation, hashing, SSH-key fingerprinting). Ports here; impls in the infra layer.

/// A freshly generated token: the one-time plaintext and a non-secret display string.
pub struct GeneratedToken {
    pub plaintext: String,
    pub display: String,
}

/// Mints new access-token plaintexts (e.g. `wiab_pat_…` + random + checksum).
pub trait TokenFactory: Send + Sync {
    fn generate(&self) -> GeneratedToken;
}

/// Hashes a token plaintext for storage and for constant-key lookup on resolution.
pub trait TokenHasher: Send + Sync {
    fn hash(&self, plaintext: &str) -> String;
}

/// Computes the fingerprint of an OpenSSH public key. `None` if the key can't be parsed.
pub trait KeyFingerprinter: Send + Sync {
    fn fingerprint(&self, openssh_public_key: &str) -> Option<String>;
}
