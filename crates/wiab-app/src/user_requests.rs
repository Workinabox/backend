use serde::{Deserialize, Serialize};
use wiab_core::user::TokenSnapshot;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserRequest {
    /// "human" or "agent".
    pub kind: String,
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddSshKeyRequest {
    pub label: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueTokenRequest {
    pub label: String,
    #[serde(default)]
    pub read_only: bool,
    /// `None` = all repos; `Some([...])` = restricted to these repo ids.
    pub repos: Option<Vec<String>>,
    pub orgs: Option<Vec<String>>,
    pub expires_at: Option<String>,
}

/// Returned once, at token creation, carrying the one-time plaintext.
#[derive(Debug, Clone, Serialize)]
pub struct IssuedTokenSnapshot {
    #[serde(flatten)]
    pub token: TokenSnapshot,
    pub plaintext: String,
}
