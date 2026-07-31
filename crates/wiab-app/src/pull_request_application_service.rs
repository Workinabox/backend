use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::meeting_traits::Clock;
use wiab_core::pull_request::{
    PullRequest, PullRequestId, PullRequestNumbering, PullRequestRepository, PullRequestSnapshot,
};
use wiab_core::repo::{BranchName, GitBackend, RepoId, RepoRepository};
use wiab_core::repository::{SaveError, Version};
use wiab_core::user::UserId;

use crate::pull_request_requests::{MergePullRequestRequest, OpenPullRequestRequest};

/// Orchestrates use cases over the `PullRequest` aggregate.
///
/// Holds the repo repository to verify the parent repo exists, and the `GitBackend` to
/// check that both branches are real before a request is opened, and to perform the merge
/// when one is accepted. Git calls are blocking, so they are offloaded with `spawn_blocking`
/// exactly as `RepoApplicationService` does.
pub struct PullRequestApplicationService<P: PullRequestRepository, R: RepoRepository> {
    pull_request_repository: P,
    repo_repository: R,
    numbering: Arc<dyn PullRequestNumbering>,
    git_backend: Arc<dyn GitBackend>,
    clock: Arc<dyn Clock>,
}

impl<P: PullRequestRepository, R: RepoRepository> PullRequestApplicationService<P, R> {
    pub fn new(
        pull_request_repository: P,
        repo_repository: R,
        numbering: Arc<dyn PullRequestNumbering>,
        git_backend: Arc<dyn GitBackend>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            pull_request_repository,
            repo_repository,
            numbering,
            git_backend,
            clock,
        }
    }

    /// Open a request against an existing repo. Returns `Ok(None)` when no repo with the
    /// given id exists.
    ///
    /// Both branches are checked against the git repository first: a request naming a
    /// branch that does not exist can never be merged, so it is rejected rather than
    /// stored.
    pub async fn open(
        &self,
        repo_id: &str,
        author_id: &str,
        request: OpenPullRequestRequest,
    ) -> anyhow::Result<Option<PullRequestSnapshot>> {
        let repo_id: RepoId = repo_id.parse()?;
        let author_id: UserId = author_id.parse()?;
        if self.repo_repository.get(&repo_id).await?.is_none() {
            return Ok(None);
        }

        let source_branch = BranchName::new(request.source_branch)?;
        let target_branch = BranchName::new(request.target_branch)?;

        let git_backend = self.git_backend.clone();
        let branches =
            tokio::task::spawn_blocking(move || git_backend.branches(&repo_id)).await??;
        for branch in [&source_branch, &target_branch] {
            if !branches.iter().any(|b| b.name == branch.as_str()) {
                return Err(anyhow!("branch '{}' does not exist", branch.as_str()));
            }
        }

        let pull_request = PullRequest::open(
            self.numbering.next(),
            repo_id,
            author_id,
            request.title,
            request.description,
            source_branch,
            target_branch,
            self.clock.now_rfc3339(),
        )?;
        let snapshot = pull_request.snapshot();
        self.pull_request_repository
            .save(pull_request, Version::NEW)
            .await?;
        Ok(Some(snapshot))
    }

    /// Requests against `repo_id`, newest first. `Ok(None)` when the repo does not exist.
    pub async fn list_by_repo(
        &self,
        repo_id: &str,
    ) -> anyhow::Result<Option<Vec<PullRequestSnapshot>>> {
        let repo_id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&repo_id).await?.is_none() {
            return Ok(None);
        }
        let mut pull_requests: Vec<PullRequest> = self
            .pull_request_repository
            .list()
            .await?
            .into_iter()
            .filter(|pr| pr.repo_id() == repo_id)
            .collect();
        pull_requests.sort_by_key(|pr| std::cmp::Reverse(pr.id().number()));
        Ok(Some(
            pull_requests.iter().map(PullRequest::snapshot).collect(),
        ))
    }

    /// `Ok(None)` when no request with the given id exists.
    pub async fn get(&self, id: &str) -> anyhow::Result<Option<PullRequestSnapshot>> {
        let id: PullRequestId = id.parse()?;
        Ok(self
            .pull_request_repository
            .get(&id)
            .await?
            .map(|(pull_request, _)| pull_request.snapshot()))
    }

    /// Abandon a request. `Ok(None)` when no request with the given id exists.
    pub async fn close(&self, id: &str) -> anyhow::Result<Option<PullRequestSnapshot>> {
        let id: PullRequestId = id.parse()?;
        loop {
            let Some((mut pull_request, version)) = self.pull_request_repository.get(&id).await?
            else {
                return Ok(None);
            };
            pull_request.close()?;
            let snapshot = pull_request.snapshot();
            match self
                .pull_request_repository
                .save(pull_request, version)
                .await
            {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    /// Integrate the source branch into the target and record the merge commit.
    /// `Ok(None)` when no request with the given id exists.
    ///
    /// The git merge happens first: if it conflicts, the aggregate is left `Open` and the
    /// error surfaced, so a failed merge never leaves a request claiming to be merged.
    pub async fn merge(
        &self,
        id: &str,
        request: MergePullRequestRequest,
    ) -> anyhow::Result<Option<PullRequestSnapshot>> {
        let id: PullRequestId = id.parse()?;
        loop {
            let Some((mut pull_request, version)) = self.pull_request_repository.get(&id).await?
            else {
                return Ok(None);
            };

            let message = request.message.clone().unwrap_or_else(|| {
                format!("Merge {}: {}", pull_request.id(), pull_request.title())
            });
            let git_backend = self.git_backend.clone();
            let repo_id = pull_request.repo_id();
            let source = pull_request.source_branch().clone();
            let target = pull_request.target_branch().clone();
            let author_name = request.author_name.clone();
            let author_email = request.author_email.clone();
            let merge_commit = tokio::task::spawn_blocking(move || {
                git_backend.merge_branch(
                    &repo_id,
                    &source,
                    &target,
                    &author_name,
                    &author_email,
                    &message,
                )
            })
            .await??;

            pull_request.mark_merged(merge_commit)?;
            let snapshot = pull_request.snapshot();
            match self
                .pull_request_repository
                .save(pull_request, version)
                .await
            {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    use wiab_core::project::ProjectId;
    use wiab_core::repo::{
        BranchSnapshot, CommitHash, CommitSnapshot, FileEntrySnapshot, GitBackendError, Repo,
        Visibility,
    };
    // `wiab_core::repo` also exports a `RepoError`; the repository ports use this one.
    use wiab_core::repository::RepoError;

    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

    #[derive(Default)]
    struct TestPullRequestRepository {
        pull_requests: RwLock<HashMap<PullRequestId, (PullRequest, u64)>>,
        /// When non-zero, the next `save` reports a conflict and decrements. Lets a test
        /// drive the retry loop without a second thread.
        conflicts: AtomicUsize,
    }

    impl PullRequestRepository for TestPullRequestRepository {
        async fn save(
            &self,
            pull_request: PullRequest,
            expected: Version,
        ) -> Result<Version, SaveError> {
            if self
                .conflicts
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                return Err(SaveError::Conflict);
            }
            let mut pull_requests = self
                .pull_requests
                .write()
                .expect("test repository write lock poisoned");
            let current = pull_requests
                .get(&pull_request.id())
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            pull_requests.insert(pull_request.id(), (pull_request, next.value()));
            Ok(next)
        }

        async fn get(
            &self,
            id: &PullRequestId,
        ) -> Result<Option<(PullRequest, Version)>, RepoError> {
            Ok(self
                .pull_requests
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(pr, version)| (pr.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<PullRequest>, RepoError> {
            Ok(self
                .pull_requests
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(pr, _)| pr.clone())
                .collect())
        }
    }

    #[derive(Default)]
    struct TestRepoRepository {
        repos: RwLock<HashMap<RepoId, (Repo, u64)>>,
    }

    impl TestRepoRepository {
        fn with_repo(id: RepoId) -> Self {
            let repo = Repo::new(
                id,
                ProjectId::from_number(1),
                "demo".to_owned(),
                String::new(),
                Visibility::Private,
            )
            .unwrap();
            let this = Self::default();
            this.repos.write().unwrap().insert(id, (repo, 1));
            this
        }
    }

    impl RepoRepository for TestRepoRepository {
        async fn save(&self, repo: Repo, expected: Version) -> Result<Version, SaveError> {
            let mut repos = self.repos.write().unwrap();
            let next = expected.next();
            repos.insert(repo.id(), (repo, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &RepoId) -> Result<Option<(Repo, Version)>, RepoError> {
            Ok(self
                .repos
                .read()
                .unwrap()
                .get(id)
                .map(|(repo, version)| (repo.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Repo>, RepoError> {
            Ok(self
                .repos
                .read()
                .unwrap()
                .values()
                .map(|(repo, _)| repo.clone())
                .collect())
        }
    }

    #[derive(Default)]
    struct TestNumbering {
        counter: AtomicU64,
    }

    impl PullRequestNumbering for TestNumbering {
        fn next(&self) -> PullRequestId {
            PullRequestId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    struct TestClock;

    impl Clock for TestClock {
        fn now_rfc3339(&self) -> String {
            "2026-07-31T12:00:00Z".to_owned()
        }
    }

    /// Reports a fixed branch list, and either merges or conflicts on demand.
    struct TestGitBackend {
        branches: Vec<String>,
        conflict: bool,
        merges: AtomicUsize,
    }

    impl Default for TestGitBackend {
        fn default() -> Self {
            Self {
                branches: vec!["main".to_owned(), "feature".to_owned()],
                conflict: false,
                merges: AtomicUsize::new(0),
            }
        }
    }

    impl TestGitBackend {
        fn conflicting() -> Self {
            Self {
                conflict: true,
                ..Default::default()
            }
        }
    }

    impl GitBackend for TestGitBackend {
        fn init_bare(&self, _id: &RepoId) -> Result<(), GitBackendError> {
            Ok(())
        }

        fn branches(&self, _id: &RepoId) -> Result<Vec<BranchSnapshot>, GitBackendError> {
            Ok(self
                .branches
                .iter()
                .map(|name| BranchSnapshot {
                    name: name.clone(),
                    target: HASH.to_owned(),
                })
                .collect())
        }

        fn list_files(
            &self,
            _id: &RepoId,
            _branch: &BranchName,
            _dir: &str,
        ) -> Result<Vec<FileEntrySnapshot>, GitBackendError> {
            Ok(Vec::new())
        }

        fn read_file(
            &self,
            _id: &RepoId,
            _branch: &BranchName,
            path: &str,
        ) -> Result<Vec<u8>, GitBackendError> {
            Err(GitBackendError::PathNotFound(path.to_owned()))
        }

        fn recent_commits(
            &self,
            _id: &RepoId,
            _branch: &BranchName,
            _limit: usize,
        ) -> Result<Vec<CommitSnapshot>, GitBackendError> {
            Ok(Vec::new())
        }

        fn commit_changes(
            &self,
            _id: &RepoId,
            _branch: &BranchName,
            _author_name: &str,
            _author_email: &str,
            _message: &str,
            _changes: Vec<wiab_core::repo::FileChange>,
        ) -> Result<CommitHash, GitBackendError> {
            unimplemented!("not exercised by these tests")
        }

        fn merge_branch(
            &self,
            _id: &RepoId,
            source: &BranchName,
            target: &BranchName,
            _author_name: &str,
            _author_email: &str,
            _message: &str,
        ) -> Result<CommitHash, GitBackendError> {
            self.merges.fetch_add(1, Ordering::SeqCst);
            if self.conflict {
                return Err(GitBackendError::MergeConflict {
                    source_branch: source.as_str().to_owned(),
                    target_branch: target.as_str().to_owned(),
                });
            }
            Ok(CommitHash::new(HASH.to_owned()).unwrap())
        }
    }

    fn service_with(
        pull_requests: TestPullRequestRepository,
        repos: TestRepoRepository,
        git: TestGitBackend,
    ) -> PullRequestApplicationService<TestPullRequestRepository, TestRepoRepository> {
        PullRequestApplicationService::new(
            pull_requests,
            repos,
            Arc::new(TestNumbering::default()),
            Arc::new(git),
            Arc::new(TestClock),
        )
    }

    fn service() -> PullRequestApplicationService<TestPullRequestRepository, TestRepoRepository> {
        service_with(
            TestPullRequestRepository::default(),
            TestRepoRepository::with_repo(RepoId::from_number(3)),
            TestGitBackend::default(),
        )
    }

    fn open_request() -> OpenPullRequestRequest {
        OpenPullRequestRequest {
            title: "Add rate limiting".to_owned(),
            description: "context".to_owned(),
            source_branch: "feature".to_owned(),
            target_branch: "main".to_owned(),
        }
    }

    fn merge_request() -> MergePullRequestRequest {
        MergePullRequestRequest {
            author_name: "Ada".to_owned(),
            author_email: "ada@example.com".to_owned(),
            message: None,
        }
    }

    #[tokio::test]
    async fn open_returns_none_for_an_unknown_repo() {
        let service = service_with(
            TestPullRequestRepository::default(),
            TestRepoRepository::default(),
            TestGitBackend::default(),
        );
        assert!(
            service
                .open("R-99", "U-1", open_request())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn open_stores_the_request() {
        let snapshot = service()
            .open("R-3", "U-1", open_request())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.id, "PR-1");
        assert_eq!(snapshot.repo_id, "R-3");
        assert_eq!(snapshot.author_id, "U-1");
        assert_eq!(snapshot.state, "open");
        assert_eq!(snapshot.opened_at, "2026-07-31T12:00:00Z");
        assert_eq!(snapshot.merge_commit, None);
    }

    #[tokio::test]
    async fn open_rejects_a_branch_that_does_not_exist() {
        let request = OpenPullRequestRequest {
            source_branch: "nope".to_owned(),
            ..open_request()
        };
        let error = service().open("R-3", "U-1", request).await.unwrap_err();
        assert!(error.to_string().contains("nope"), "{error}");
    }

    #[tokio::test]
    async fn open_propagates_a_domain_invariant() {
        let request = OpenPullRequestRequest {
            source_branch: "main".to_owned(),
            ..open_request()
        };
        let error = service().open("R-3", "U-1", request).await.unwrap_err();
        assert!(error.to_string().contains("own source branch"), "{error}");
    }

    #[tokio::test]
    async fn list_by_repo_returns_only_that_repos_requests_newest_first() {
        let service = service();
        service.open("R-3", "U-1", open_request()).await.unwrap();
        service.open("R-3", "U-1", open_request()).await.unwrap();

        let listed = service.list_by_repo("R-3").await.unwrap().unwrap();
        assert_eq!(
            listed.iter().map(|pr| pr.id.as_str()).collect::<Vec<_>>(),
            vec!["PR-2", "PR-1"]
        );
    }

    #[tokio::test]
    async fn list_by_repo_returns_none_for_an_unknown_repo() {
        assert!(service().list_by_repo("R-99").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_returns_none_for_an_unknown_id() {
        assert!(service().get("PR-99").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn close_transitions_and_persists() {
        let service = service();
        service.open("R-3", "U-1", open_request()).await.unwrap();

        let closed = service.close("PR-1").await.unwrap().unwrap();
        assert_eq!(closed.state, "closed");
        assert_eq!(service.get("PR-1").await.unwrap().unwrap().state, "closed");
    }

    #[tokio::test]
    async fn closing_twice_is_rejected() {
        let service = service();
        service.open("R-3", "U-1", open_request()).await.unwrap();
        service.close("PR-1").await.unwrap();
        assert!(service.close("PR-1").await.is_err());
    }

    #[tokio::test]
    async fn close_returns_none_for_an_unknown_id() {
        assert!(service().close("PR-99").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn merge_records_the_commit() {
        let service = service();
        service.open("R-3", "U-1", open_request()).await.unwrap();

        let merged = service
            .merge("PR-1", merge_request())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(merged.state, "merged");
        assert_eq!(merged.merge_commit, Some(HASH.to_owned()));
    }

    #[tokio::test]
    async fn a_conflicting_merge_leaves_the_request_open() {
        let service = service_with(
            TestPullRequestRepository::default(),
            TestRepoRepository::with_repo(RepoId::from_number(3)),
            TestGitBackend::conflicting(),
        );
        service.open("R-3", "U-1", open_request()).await.unwrap();

        let error = service.merge("PR-1", merge_request()).await.unwrap_err();
        assert!(error.to_string().contains("conflicts"), "{error}");
        // The aggregate must not claim to be merged when the merge did not happen.
        assert_eq!(service.get("PR-1").await.unwrap().unwrap().state, "open");
    }

    #[tokio::test]
    async fn merge_returns_none_for_an_unknown_id() {
        assert!(
            service()
                .merge("PR-99", merge_request())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn close_retries_after_a_concurrent_save() {
        let repository = TestPullRequestRepository::default();
        let service = service_with(
            repository,
            TestRepoRepository::with_repo(RepoId::from_number(3)),
            TestGitBackend::default(),
        );
        service.open("R-3", "U-1", open_request()).await.unwrap();
        // Make exactly the next save conflict; the loop should re-read and succeed.
        service
            .pull_request_repository
            .conflicts
            .store(1, Ordering::SeqCst);

        let closed = service.close("PR-1").await.unwrap().unwrap();
        assert_eq!(closed.state, "closed");
    }
}
