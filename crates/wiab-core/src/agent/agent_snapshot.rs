use serde::{Deserialize, Serialize};

/// Serializable read view of an `Agent`. HTTP responses use this rather than the
/// domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub description: String,
    /// The assigned VM type (template name), if any.
    pub vm_type: Option<String>,
    pub active: bool,
    /// The VM booted for this agent while active.
    pub vm_id: Option<String>,
    /// The active VM's guest IP; filled by the application layer (not carried on the aggregate).
    pub guest_ip: Option<String>,
}
