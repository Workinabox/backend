use serde::{Deserialize, Serialize};

/// Serializable read view of a `Repo`. HTTP responses use this rather than the
/// domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSnapshot {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub description: String,
}
