use crate::auth::{AuthError, VerifiedClaims};

/// What the relying party needs to start an authorization-code + PKCE flow: the URL to
/// redirect the browser to, plus the per-attempt secrets the app must persist (keyed by
/// `state`) for the callback.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub authorize_url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

/// OIDC relying-party seam: discovery, authorization-URL construction, and code exchange +
/// ID-token validation. Implemented over a vetted OIDC library so protocol/crypto stay out
/// of the domain. The adapter holds each connection's issuer/client config, addressed by
/// slug.
#[allow(async_fn_in_trait)]
pub trait OidcPort: Send + Sync {
    async fn begin(&self, connection: &str) -> Result<AuthRequest, AuthError>;
    async fn complete(
        &self,
        connection: &str,
        code: &str,
        pkce_verifier: &str,
        expected_nonce: &str,
    ) -> Result<VerifiedClaims, AuthError>;
}
