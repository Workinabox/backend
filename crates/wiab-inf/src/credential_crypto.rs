//! Infrastructure impls of the credential seams: random token generation, SHA-256
//! hashing, and SSH-key fingerprinting.

use base64::Engine;
use rand::Rng;
use sha2::{Digest, Sha256};
use wiab_core::user::{GeneratedToken, KeyFingerprinter, TokenFactory, TokenHasher};

/// Mints `wiab_pat_<base64url(32 random bytes)><crc32>` tokens.
pub struct RandomTokenFactory;

impl TokenFactory for RandomTokenFactory {
    fn generate(&self) -> GeneratedToken {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let body = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        // CRC over the random body lets secret scanners reject typos cheaply.
        let crc = crc32fast::hash(body.as_bytes());
        let plaintext = format!("wiab_pat_{body}{crc:08x}");
        let tail = &plaintext[plaintext.len() - 4..];
        let display = format!("wiab_pat_…{tail}");
        GeneratedToken { plaintext, display }
    }
}

/// Hashes token plaintexts with SHA-256 (correct for a high-entropy secret; a slow KDF
/// would only add latency without security gain, and a deterministic hash enables lookup).
pub struct Sha256TokenHasher;

impl TokenHasher for Sha256TokenHasher {
    fn hash(&self, plaintext: &str) -> String {
        let digest = Sha256::digest(plaintext.as_bytes());
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

/// Computes a key's `SHA256:…` fingerprint from its OpenSSH public-key text.
pub struct Sha256KeyFingerprinter;

impl KeyFingerprinter for Sha256KeyFingerprinter {
    fn fingerprint(&self, openssh_public_key: &str) -> Option<String> {
        russh::keys::ssh_key::PublicKey::from_openssh(openssh_public_key.trim())
            .ok()
            .map(|key| {
                key.fingerprint(russh::keys::ssh_key::HashAlg::Sha256)
                    .to_string()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_displayed_safely() {
        let factory = RandomTokenFactory;
        let a = factory.generate();
        let b = factory.generate();
        assert_ne!(a.plaintext, b.plaintext);
        assert!(a.plaintext.starts_with("wiab_pat_"));
        assert!(a.display.starts_with("wiab_pat_…"));
        assert!(!a.display.contains(&a.plaintext[9..20]));
    }

    #[test]
    fn hash_is_stable_and_distinct() {
        let hasher = Sha256TokenHasher;
        assert_eq!(hasher.hash("abc"), hasher.hash("abc"));
        assert_ne!(hasher.hash("abc"), hasher.hash("abd"));
        assert_eq!(hasher.hash("abc").len(), 64);
    }

    #[test]
    fn fingerprints_a_real_key_and_rejects_junk() {
        let fp = Sha256KeyFingerprinter;
        // A valid ed25519 public key.
        let key =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIxhPjN5e0V6r2sV4Qe5wL9p0aQ2mWn5sIY7k3oF2q0p test";
        assert!(fp.fingerprint(key).unwrap().starts_with("SHA256:"));
        assert!(fp.fingerprint("not a key").is_none());
    }
}
