use crate::pull_request::{PullRequestError, PullRequestId, PullRequestSnapshot, PullRequestState};
use crate::repo::{BranchName, CommitHash, RepoId};
use crate::user::UserId;

/// A proposal to integrate `source_branch` into `target_branch` of a repo: a `PR-###` id, the
/// repo it belongs to, who opened it, a title and description, and its lifecycle state.
///
/// `PullRequest` is an aggregate root; it references the repo, the author, and the merge
/// commit by identity only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    id: PullRequestId,
    repo_id: RepoId,
    author_id: UserId,
    title: String,
    description: String,
    source_branch: BranchName,
    target_branch: BranchName,
    state: PullRequestState,
    /// The commit produced by the merge. `Some` exactly when `state` is `Merged`.
    merge_commit: Option<CommitHash>,
    /// RFC3339 instant the request was opened, supplied by the `Clock` seam.
    opened_at: String,
}

impl PullRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        id: PullRequestId,
        repo_id: RepoId,
        author_id: UserId,
        title: String,
        description: String,
        source_branch: BranchName,
        target_branch: BranchName,
        opened_at: String,
    ) -> Result<Self, PullRequestError> {
        if title.trim().is_empty() {
            return Err(PullRequestError::EmptyTitle);
        }
        if source_branch == target_branch {
            return Err(PullRequestError::SameSourceAndTarget(
                source_branch.to_string(),
            ));
        }
        Ok(Self {
            id,
            repo_id,
            author_id,
            title,
            description,
            source_branch,
            target_branch,
            state: PullRequestState::Open,
            merge_commit: None,
            opened_at,
        })
    }

    /// Rebuild a `PullRequest` from persisted state, including its terminal states (used by
    /// repository implementations). Bypasses the invariants enforced by `open`.
    #[allow(clippy::too_many_arguments)]
    pub fn from_persistence(
        id: PullRequestId,
        repo_id: RepoId,
        author_id: UserId,
        title: String,
        description: String,
        source_branch: BranchName,
        target_branch: BranchName,
        state: PullRequestState,
        merge_commit: Option<CommitHash>,
        opened_at: String,
    ) -> Self {
        Self {
            id,
            repo_id,
            author_id,
            title,
            description,
            source_branch,
            target_branch,
            state,
            merge_commit,
            opened_at,
        }
    }

    pub fn id(&self) -> PullRequestId {
        self.id
    }

    pub fn repo_id(&self) -> RepoId {
        self.repo_id
    }

    pub fn author_id(&self) -> UserId {
        self.author_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn source_branch(&self) -> &BranchName {
        &self.source_branch
    }

    pub fn target_branch(&self) -> &BranchName {
        &self.target_branch
    }

    pub fn state(&self) -> PullRequestState {
        self.state
    }

    pub fn merge_commit(&self) -> Option<&CommitHash> {
        self.merge_commit.as_ref()
    }

    pub fn opened_at(&self) -> &str {
        &self.opened_at
    }

    /// Abandon the request without integrating it. Only an open request can be closed;
    /// `Merged` and `Closed` are terminal.
    pub fn close(&mut self) -> Result<(), PullRequestError> {
        if !self.state.is_open() {
            return Err(PullRequestError::NotOpen(self.state));
        }
        self.state = PullRequestState::Closed;
        Ok(())
    }

    /// Record that the source branch was integrated into the target as `merge_commit`.
    /// The caller performs the git-level merge; the aggregate records the outcome.
    pub fn mark_merged(&mut self, merge_commit: CommitHash) -> Result<(), PullRequestError> {
        if !self.state.is_open() {
            return Err(PullRequestError::NotOpen(self.state));
        }
        self.state = PullRequestState::Merged;
        self.merge_commit = Some(merge_commit);
        Ok(())
    }

    pub fn snapshot(&self) -> PullRequestSnapshot {
        PullRequestSnapshot {
            id: self.id.to_string(),
            repo_id: self.repo_id.to_string(),
            author_id: self.author_id.to_string(),
            title: self.title.clone(),
            description: self.description.clone(),
            source_branch: self.source_branch.to_string(),
            target_branch: self.target_branch.to_string(),
            state: self.state.to_string(),
            merge_commit: self.merge_commit.as_ref().map(CommitHash::to_string),
            opened_at: self.opened_at.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENED_AT: &str = "2026-07-31T12:00:00Z";
    const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

    fn branch(name: &str) -> BranchName {
        BranchName::new(name.to_owned()).unwrap()
    }

    fn commit() -> CommitHash {
        CommitHash::new(HASH.to_owned()).unwrap()
    }

    fn open_request() -> PullRequest {
        PullRequest::open(
            PullRequestId::from_number(1),
            RepoId::from_number(3),
            UserId::from_number(2),
            "Add rate limiting".to_owned(),
            "Long form context.".to_owned(),
            branch("feature"),
            branch("main"),
            OPENED_AT.to_owned(),
        )
        .unwrap()
    }

    #[test]
    fn opens_in_the_open_state_without_a_merge_commit() {
        let pr = open_request();
        assert_eq!(pr.state(), PullRequestState::Open);
        assert_eq!(pr.merge_commit(), None);
    }

    #[test]
    fn exposes_getters() {
        let pr = open_request();
        assert_eq!(pr.id(), PullRequestId::from_number(1));
        assert_eq!(pr.repo_id(), RepoId::from_number(3));
        assert_eq!(pr.author_id(), UserId::from_number(2));
        assert_eq!(pr.title(), "Add rate limiting");
        assert_eq!(pr.description(), "Long form context.");
        assert_eq!(pr.source_branch(), &branch("feature"));
        assert_eq!(pr.target_branch(), &branch("main"));
        assert_eq!(pr.opened_at(), OPENED_AT);
    }

    #[test]
    fn rejects_a_blank_title() {
        let error = PullRequest::open(
            PullRequestId::from_number(1),
            RepoId::from_number(3),
            UserId::from_number(2),
            "   ".to_owned(),
            String::new(),
            branch("feature"),
            branch("main"),
            OPENED_AT.to_owned(),
        )
        .unwrap_err();
        assert_eq!(error, PullRequestError::EmptyTitle);
    }

    #[test]
    fn rejects_a_request_onto_its_own_branch() {
        let error = PullRequest::open(
            PullRequestId::from_number(1),
            RepoId::from_number(3),
            UserId::from_number(2),
            "Title".to_owned(),
            String::new(),
            branch("main"),
            branch("main"),
            OPENED_AT.to_owned(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            PullRequestError::SameSourceAndTarget("main".to_owned())
        );
    }

    #[test]
    fn closing_an_open_request_closes_it() {
        let mut pr = open_request();
        pr.close().unwrap();
        assert_eq!(pr.state(), PullRequestState::Closed);
    }

    #[test]
    fn closing_a_closed_request_is_rejected() {
        let mut pr = open_request();
        pr.close().unwrap();
        assert_eq!(
            pr.close().unwrap_err(),
            PullRequestError::NotOpen(PullRequestState::Closed)
        );
        assert_eq!(pr.state(), PullRequestState::Closed);
    }

    #[test]
    fn closing_a_merged_request_is_rejected_and_keeps_the_merge_commit() {
        let mut pr = open_request();
        pr.mark_merged(commit()).unwrap();
        assert_eq!(
            pr.close().unwrap_err(),
            PullRequestError::NotOpen(PullRequestState::Merged)
        );
        assert_eq!(pr.state(), PullRequestState::Merged);
        assert_eq!(pr.merge_commit(), Some(&commit()));
    }

    #[test]
    fn merging_an_open_request_records_the_commit() {
        let mut pr = open_request();
        pr.mark_merged(commit()).unwrap();
        assert_eq!(pr.state(), PullRequestState::Merged);
        assert_eq!(pr.merge_commit(), Some(&commit()));
    }

    #[test]
    fn merging_a_merged_request_is_rejected() {
        let mut pr = open_request();
        pr.mark_merged(commit()).unwrap();
        assert_eq!(
            pr.mark_merged(commit()).unwrap_err(),
            PullRequestError::NotOpen(PullRequestState::Merged)
        );
    }

    #[test]
    fn merging_a_closed_request_is_rejected_and_records_no_commit() {
        let mut pr = open_request();
        pr.close().unwrap();
        assert_eq!(
            pr.mark_merged(commit()).unwrap_err(),
            PullRequestError::NotOpen(PullRequestState::Closed)
        );
        assert_eq!(pr.merge_commit(), None);
    }

    #[test]
    fn snapshot_mirrors_an_open_request() {
        let snapshot = open_request().snapshot();
        assert_eq!(snapshot.id, "PR-1");
        assert_eq!(snapshot.repo_id, "R-3");
        assert_eq!(snapshot.author_id, "U-2");
        assert_eq!(snapshot.title, "Add rate limiting");
        assert_eq!(snapshot.description, "Long form context.");
        assert_eq!(snapshot.source_branch, "feature");
        assert_eq!(snapshot.target_branch, "main");
        assert_eq!(snapshot.state, "open");
        assert_eq!(snapshot.merge_commit, None);
        assert_eq!(snapshot.opened_at, OPENED_AT);
    }

    #[test]
    fn snapshot_carries_the_merge_commit_once_merged() {
        let mut pr = open_request();
        pr.mark_merged(commit()).unwrap();
        let snapshot = pr.snapshot();
        assert_eq!(snapshot.state, "merged");
        assert_eq!(snapshot.merge_commit, Some(HASH.to_owned()));
    }

    #[test]
    fn from_persistence_round_trips_a_merged_request() {
        let pr = PullRequest::from_persistence(
            PullRequestId::from_number(5),
            RepoId::from_number(3),
            UserId::from_number(2),
            "Title".to_owned(),
            String::new(),
            branch("feature"),
            branch("main"),
            PullRequestState::Merged,
            Some(commit()),
            OPENED_AT.to_owned(),
        );
        assert_eq!(pr.state(), PullRequestState::Merged);
        assert_eq!(pr.merge_commit(), Some(&commit()));
    }

    #[test]
    fn from_persistence_round_trips_a_closed_request() {
        let pr = PullRequest::from_persistence(
            PullRequestId::from_number(5),
            RepoId::from_number(3),
            UserId::from_number(2),
            "Title".to_owned(),
            String::new(),
            branch("feature"),
            branch("main"),
            PullRequestState::Closed,
            None,
            OPENED_AT.to_owned(),
        );
        assert_eq!(pr.state(), PullRequestState::Closed);
        assert_eq!(pr.merge_commit(), None);
    }
}
