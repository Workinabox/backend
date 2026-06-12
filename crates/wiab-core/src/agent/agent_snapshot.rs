use serde::{Deserialize, Serialize};

/// Serializable read view of an `Agent`. HTTP responses use this rather than the
/// domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub description: String,
}
