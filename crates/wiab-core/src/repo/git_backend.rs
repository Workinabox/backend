use crate::repo::{
    BranchName, BranchSnapshot, CommitHash, CommitSnapshot, FileChange, FileEntrySnapshot,
    GitBackendError, RepoId,
};

/// Port over the git object store backing each [`Repo`](crate::repo::Repo).
///
/// The aggregate owns only metadata and the push token; a repo's branches, commits,
/// and files live in an on-disk git repository that this port reads and writes. The
/// implementation lives in the infrastructure layer so the domain never depends on a
/// concrete git library.
///
/// Calls are synchronous and may block; callers off the async runtime should offload
/// them (e.g. `spawn_blocking`).
pub trait GitBackend: Send + Sync + 'static {
    /// Create an empty bare repository for `id` if one does not already exist.
    /// Idempotent and non-destructive: an existing repository is left untouched.
    fn init_bare(&self, id: &RepoId) -> Result<(), GitBackendError>;

    /// List the repository's local branches and the commit each points at.
    fn branches(&self, id: &RepoId) -> Result<Vec<BranchSnapshot>, GitBackendError>;

    /// List the entries directly under `dir` (root when empty) at the tip of `branch`.
    fn list_files(
        &self,
        id: &RepoId,
        branch: &BranchName,
        dir: &str,
    ) -> Result<Vec<FileEntrySnapshot>, GitBackendError>;

    /// Read the bytes of the file at `path` at the tip of `branch`.
    fn read_file(
        &self,
        id: &RepoId,
        branch: &BranchName,
        path: &str,
    ) -> Result<Vec<u8>, GitBackendError>;

    /// The most recent commits on `branch`, newest first, capped at `limit`.
    fn recent_commits(
        &self,
        id: &RepoId,
        branch: &BranchName,
        limit: usize,
    ) -> Result<Vec<CommitSnapshot>, GitBackendError>;

    /// Apply `changes` as a single commit on `branch`, parented on the current tip
    /// (or as a root commit when the branch does not yet exist). Returns the new hash.
    fn commit_changes(
        &self,
        id: &RepoId,
        branch: &BranchName,
        author_name: &str,
        author_email: &str,
        message: &str,
        changes: Vec<FileChange>,
    ) -> Result<CommitHash, GitBackendError>;

    /// Integrate the tip of `source` into `target` as a merge commit, moving the `target`
    /// ref to it. Returns the new commit's hash.
    ///
    /// Fails with [`GitBackendError::MergeConflict`] when the three-way merge does not
    /// resolve cleanly, leaving the repository untouched — a half-merged target ref would
    /// be worse than a rejected merge.
    #[allow(clippy::too_many_arguments)]
    fn merge_branch(
        &self,
        id: &RepoId,
        source: &BranchName,
        target: &BranchName,
        author_name: &str,
        author_email: &str,
        message: &str,
    ) -> Result<CommitHash, GitBackendError>;
}
