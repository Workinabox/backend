//! argon2id implementation of the [`PasswordHasher`] seam.

use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, Params, PasswordHash, Version};
// Bring the argon2 trait methods into scope without colliding with our own
// `authbox_core::auth::PasswordHasher`.
use argon2::{PasswordHasher as _, PasswordVerifier as _};
use authbox_core::auth::PasswordHasher;
use rand::Rng;

/// Hashes passwords with argon2id at the OWASP-recommended baseline
/// (m = 19 MiB, t = 2, p = 1). The PHC string it returns embeds the salt and these params,
/// so `verify` reads them back and a later params bump can be detected per-hash.
pub struct Argon2idPasswordHasher;

fn argon2() -> Argon2<'static> {
    let params = Params::new(19_456, 2, 1, None).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

impl PasswordHasher for Argon2idPasswordHasher {
    fn hash(&self, plaintext: &str) -> String {
        // 16 random salt bytes, drawn the same way as the PAT factory (rand::rng()).
        let mut salt_bytes = [0u8; 16];
        rand::rng().fill_bytes(&mut salt_bytes);
        let salt = SaltString::encode_b64(&salt_bytes).expect("valid salt");
        argon2()
            .hash_password(plaintext.as_bytes(), &salt)
            .expect("hash password")
            .to_string()
    }

    fn verify(&self, plaintext: &str, phc_hash: &str) -> bool {
        match PasswordHash::new(phc_hash) {
            Ok(parsed) => argon2()
                .verify_password(plaintext.as_bytes(), &parsed)
                .is_ok(),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_round_trips() {
        let hasher = Argon2idPasswordHasher;
        let phc = hasher.hash("correct horse battery staple");
        assert!(phc.starts_with("$argon2id$"));
        assert!(hasher.verify("correct horse battery staple", &phc));
        assert!(!hasher.verify("wrong", &phc));
    }

    #[test]
    fn distinct_salts_yield_distinct_hashes() {
        let hasher = Argon2idPasswordHasher;
        assert_ne!(hasher.hash("same"), hasher.hash("same"));
    }

    #[test]
    fn malformed_hash_does_not_verify() {
        let hasher = Argon2idPasswordHasher;
        assert!(!hasher.verify("anything", "not-a-phc-string"));
    }
}
