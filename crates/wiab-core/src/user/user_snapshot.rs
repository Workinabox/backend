use serde::{Deserialize, Serialize};

/// Serializable read view of a `User`. Secrets (token hashes, key bodies beyond the
/// fingerprint) are excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSnapshot {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub email: Option<String>,
    pub agent_id: Option<String>,
    pub ssh_keys: Vec<SshKeySnapshot>,
    pub tokens: Vec<TokenSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshKeySnapshot {
    pub id: String,
    pub label: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSnapshot {
    pub id: String,
    pub label: String,
    /// Non-secret display string, e.g. `wiab_pat_…a1b2`.
    pub display: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub scope: TokenScopeSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenScopeSnapshot {
    pub read_only: bool,
    /// `None` = unrestricted; `Some([])` = restricted to nothing.
    pub repos: Option<Vec<String>>,
    pub orgs: Option<Vec<String>>,
}
