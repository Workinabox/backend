/// Generates opaque high-entropy secrets — session cookie values, CSRF tokens, and (in
/// later slices) email verification / reset / invite tokens. The infrastructure impl uses a
/// CSPRNG; only hashes of these secrets are ever stored.
pub trait SecretGenerator: Send + Sync {
    fn generate(&self) -> String;
}
