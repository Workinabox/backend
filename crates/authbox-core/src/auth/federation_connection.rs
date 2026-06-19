/// Configuration for one inbound OIDC connection (Google, an enterprise IdP, …).
///
/// Google and the enterprise IdP are the same relying-party code path with different
/// config; only `slug` distinguishes them in URLs and storage.
#[derive(Debug, Clone)]
pub struct FederationConnection {
    pub slug: String,
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: Vec<String>,
    /// The callback URL registered with the IdP (`<base>/api/auth/oidc/<slug>/callback`).
    pub redirect_uri: String,
    /// Whether a verified-email match to an existing local user may be auto-linked. Safe
    /// for an enterprise IdP whose users are pre-provisioned; left false for consumer
    /// providers (e.g. Google) to avoid takeover via an attacker-asserted email.
    pub auto_link_verified_email: bool,
    /// Whether the IdP's `email_verified` claim must be true before the email is trusted.
    /// True for consumer providers (Google sends it). False for an enterprise IdP that is
    /// authoritative for its own users and may omit the claim entirely (e.g. Microsoft
    /// Entra) — the org's IdP vouching for the user is the verification.
    pub require_email_verified: bool,
}
