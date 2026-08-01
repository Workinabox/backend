use serde::{Deserialize, Serialize};

/// Serializable read view of a `Team`. HTTP responses use this rather than the domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSnapshot {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub description: String,
    pub vm_template: String,
    pub state: String,
    /// Set while the team has a container; `None` once stopped.
    pub vm_id: Option<String>,
}
