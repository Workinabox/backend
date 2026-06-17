use thiserror::Error;

/// Failures in the authentication layer. `Backend` wraps store/adapter errors so the ports
/// can surface infrastructure problems without leaking their concrete error types.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("session is invalid, revoked, or expired")]
    SessionInvalid,
    #[error("an account with this email already exists; sign in and link it")]
    AccountExists,
    #[error("the SSO login could not be completed: {0}")]
    FederationFailed(String),
    #[error("'{0}' is not a valid session id")]
    InvalidSessionId(String),
    #[error("auth backend error: {0}")]
    Backend(String),
}
