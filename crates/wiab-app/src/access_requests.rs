use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct GrantRoleRequest {
    pub user_id: String,
    /// "org" | "project" | "repo".
    pub scope_kind: String,
    pub scope_id: String,
    /// "read" | "write" | "admin" | "owner".
    pub role: String,
}
