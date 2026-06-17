//! CSPRNG implementation of the [`SecretGenerator`] seam.

use authbox_core::auth::SecretGenerator;
use base64::Engine;
use rand::Rng;

/// Generates 32-byte (256-bit) URL-safe opaque secrets for session cookies, CSRF tokens,
/// and (later) verification/reset/invite tokens. Mirrors the PAT factory's randomness.
pub struct RandomSecretGenerator;

impl SecretGenerator for RandomSecretGenerator {
    fn generate(&self) -> String {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_unique_and_url_safe() {
        let generator = RandomSecretGenerator;
        let a = generator.generate();
        let b = generator.generate();
        assert_ne!(a, b);
        assert!(!a.is_empty());
        assert!(!a.contains('+') && !a.contains('/') && !a.contains('='));
    }
}
