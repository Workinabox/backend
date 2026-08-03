use crate::auth::{AuthError, VerifiedClaims};

/// What the relying party needs to start an authorization-code + PKCE flow: the URL to
/// redirect the browser to, plus the per-attempt secrets the app must persist (keyed by
/// `state`) for the callback.
#[derive(Clone)]
pub struct AuthRequest {
    pub authorize_url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

/// Redacting `Debug`: `state`, `nonce` and `pkce_verifier` are the secrets that make the
/// authorization-code flow unforgeable. Leaking them to a log turns replay and CSRF-on-login
/// protections off for whoever can read it. `authorize_url` is kept — it is what the browser
/// is sent to and is the useful half when debugging — but it embeds `state` and the PKCE
/// challenge, so it is elided too rather than reasoned about case by case.
impl std::fmt::Debug for AuthRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthRequest")
            .field("authorize_url", &"<redacted>")
            .field("state", &"<redacted>")
            .field("nonce", &"<redacted>")
            .field("pkce_verifier", &"<redacted>")
            .finish()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_flow_secrets() {
        let request = AuthRequest {
            authorize_url: "https://idp.example.com/authorize?state=st4te".to_owned(),
            state: "st4te".to_owned(),
            nonce: "n0nce".to_owned(),
            pkce_verifier: "v3rifier".to_owned(),
        };
        let rendered = format!("{request:?}");
        for secret in ["st4te", "n0nce", "v3rifier", "idp.example.com"] {
            assert!(
                !rendered.contains(secret),
                "Debug leaked {secret}: {rendered}"
            );
        }
    }
}
