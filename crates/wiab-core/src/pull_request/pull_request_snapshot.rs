use serde::{Deserialize, Serialize};

/// Serializable read view of a `PullRequest`. HTTP responses use this rather than the
/// domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestSnapshot {
    pub id: String,
    pub repo_id: String,
    pub author_id: String,
    pub title: String,
    pub description: String,
    pub source_branch: String,
    pub target_branch: String,
    pub state: String,
    /// Set exactly when `state` is `merged`.
    pub merge_commit: Option<String>,
    pub opened_at: String,
}
