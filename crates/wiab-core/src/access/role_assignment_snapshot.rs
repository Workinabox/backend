use serde::{Deserialize, Serialize};

/// Serializable read view of a `RoleAssignment`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleAssignmentSnapshot {
    pub id: String,
    pub user_id: String,
    pub scope_kind: String,
    pub scope_id: String,
    pub role: String,
}
