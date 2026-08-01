use serde::{Deserialize, Serialize};

/// Serializable read view of a `Team`. HTTP responses use this rather than the domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSnapshot {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub description: String,
    /// Where the team's work queues up.
    pub board_id: String,
    /// The codebase the team works in.
    pub repo_id: String,
    /// The team's own identity; it authenticates to the backend as this user.
    pub user_id: String,
    pub vm_template: String,
    pub state: String,
    /// Set while the team has a container; `None` once stopped.
    pub vm_id: Option<String>,
}
