/// Short-lived server-side state for an in-flight OIDC login, keyed by the `state`
/// parameter. Holds the PKCE verifier and nonce (validated on callback) and where to send
/// the user afterward. Single-use and TTL-bounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthFlow {
    state: String,
    connection: String,
    nonce: String,
    pkce_verifier: String,
    return_to: String,
    expires_at: String,
}

impl AuthFlow {
    pub fn new(
        state: String,
        connection: String,
        nonce: String,
        pkce_verifier: String,
        return_to: String,
        expires_at: String,
    ) -> Self {
        Self {
            state,
            connection,
            nonce,
            pkce_verifier,
            return_to,
            expires_at,
        }
    }

    pub fn state(&self) -> &str {
        &self.state
    }

    pub fn connection(&self) -> &str {
        &self.connection
    }

    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    pub fn pkce_verifier(&self) -> &str {
        &self.pkce_verifier
    }

    pub fn return_to(&self) -> &str {
        &self.return_to
    }

    pub fn expires_at(&self) -> &str {
        &self.expires_at
    }

    pub fn is_expired(&self, now_rfc3339: &str) -> bool {
        now_rfc3339 >= self.expires_at.as_str()
    }
}
