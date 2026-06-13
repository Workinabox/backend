use serde::{Deserialize, Serialize};

/// A branch and the commit it points at. Serializable read view returned by the
/// `GitBackend` port and sent straight back over HTTP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchSnapshot {
    pub name: String,
    /// Commit hash the branch tip resolves to.
    pub target: String,
}

/// A single commit in a branch's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitSnapshot {
    pub hash: String,
    pub message: String,
    pub author: String,
    /// Author timestamp as seconds since the Unix epoch.
    pub time_unix: i64,
    pub parents: Vec<String>,
}

/// One entry in a tree listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntrySnapshot {
    /// Path relative to the repository root.
    pub path: String,
    pub is_dir: bool,
}

/// An upsert of file content applied by a server-side commit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub content: Vec<u8>,
}
