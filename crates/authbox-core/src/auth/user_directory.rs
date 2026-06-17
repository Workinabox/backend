use crate::auth::{AuthError, PrincipalId};

/// The seam to the host's user store.
///
/// The host (WIAB) implements this over its own `User` aggregate so the auth layer can
/// resolve a login identifier to a principal without depending on the concrete user type.
/// Federation lookups and just-in-time provisioning are added with the social/SSO slices.
#[allow(async_fn_in_trait)]
pub trait UserDirectory: Send + Sync + 'static {
    /// Resolve a login email to a principal permitted to authenticate. The host returns
    /// `None` for an unknown or deactivated user (so lifecycle policy stays host-side).
    async fn find_by_email(&self, email: &str) -> Result<Option<PrincipalId>, AuthError>;

    /// Just-in-time provision a new user from verified federated claims, returning its
    /// principal. Called only when no existing user matches (social/SSO first login).
    async fn provision(&self, email: &str, name: &str) -> Result<PrincipalId, AuthError>;
}
