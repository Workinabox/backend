use crate::auth::{AuthError, AuthFlow};

/// Port for the short-lived OIDC login-state store. `take` is single-use (returns and
/// removes), which is what makes the `state` parameter a one-shot CSRF guard.
#[allow(async_fn_in_trait)]
pub trait AuthFlowStore: Send + Sync + 'static {
    async fn put(&self, flow: AuthFlow) -> Result<(), AuthError>;
    async fn take(&self, state: &str) -> Result<Option<AuthFlow>, AuthError>;
}
