use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct OpenPullRequestRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub source_branch: String,
    pub target_branch: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MergePullRequestRequest {
    pub author_name: String,
    pub author_email: String,
    /// Merge commit message. Defaults to `Merge PR-<n>: <title>` when omitted.
    #[serde(default)]
    pub message: Option<String>,
}
