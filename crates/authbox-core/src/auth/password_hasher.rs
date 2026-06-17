/// Password hashing seam (argon2id in the infrastructure layer).
///
/// Sync so it can be an injected `Arc<dyn PasswordHasher>`; callers run it inside
/// `spawn_blocking` because hashing is deliberately CPU/memory-bound.
pub trait PasswordHasher: Send + Sync {
    /// Hash a plaintext password into a self-describing PHC string (salt + params embedded).
    fn hash(&self, plaintext: &str) -> String;

    /// Verify a plaintext against a stored PHC hash (constant-time within the impl).
    fn verify(&self, plaintext: &str, phc_hash: &str) -> bool;
}
