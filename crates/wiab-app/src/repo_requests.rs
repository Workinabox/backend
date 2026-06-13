use serde::Deserialize;
use wiab_core::repo::FileChange;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRepoRequest {
    pub name: String,
    pub description: String,
    /// "private" | "public"; defaults to private when omitted.
    pub visibility: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRepoRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetVisibilityRequest {
    /// "private" | "public".
    pub visibility: String,
}

/// A server-side commit: a branch, author identity, message, and the file upserts to
/// apply.
#[derive(Debug, Clone, Deserialize)]
pub struct CommitChangesRequest {
    pub branch: String,
    pub author_name: String,
    pub author_email: String,
    pub message: String,
    pub changes: Vec<FileChange>,
}
