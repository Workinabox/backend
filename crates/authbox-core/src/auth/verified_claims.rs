/// Identity claims extracted and verified from an OIDC ID token by the relying-party
/// adapter (signature, issuer, audience, expiry, and nonce already checked).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaims {
    pub issuer: String,
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
}
