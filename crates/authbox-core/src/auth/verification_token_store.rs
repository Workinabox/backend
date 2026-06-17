use crate::auth::{AuthError, VerificationToken};

/// Port for single-use verification tokens (password reset, …). `consume` returns and
/// removes the token in one step, so a reset link can be used at most once.
#[allow(async_fn_in_trait)]
pub trait VerificationTokenStore: Send + Sync + 'static {
    async fn put(&self, token: VerificationToken) -> Result<(), AuthError>;
    async fn consume(&self, token_hash: &str) -> Result<Option<VerificationToken>, AuthError>;
}
