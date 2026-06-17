use crate::auth::{AuthError, PrincipalId, Session};

/// Port for persisting sessions. Resolution is by the hash of the cookie secret (indexed),
/// not by id. `put` is an upsert (sessions are last-write-wins, not versioned).
#[allow(async_fn_in_trait)]
pub trait SessionStore: Send + Sync + 'static {
    async fn put(&self, session: Session) -> Result<(), AuthError>;
    async fn find_by_token_hash(&self, token_hash: &str) -> Result<Option<Session>, AuthError>;
    /// Revoke every session for a principal — used on password change/reset.
    async fn revoke_all_for_principal(&self, principal: &PrincipalId) -> Result<(), AuthError>;
}
