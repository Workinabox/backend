use std::sync::{Arc, Mutex};

use wiab_core::project::{ProjectId, ProjectRepository};
use wiab_core::repo::{
    BranchName, BranchSnapshot, CommitSnapshot, FileEntrySnapshot, GitBackend, Repo, RepoId,
    RepoNumbering, RepoRepository, RepoSnapshot, Visibility,
};

use crate::repo_requests::{
    CommitChangesRequest, CreateRepoRequest, SetVisibilityRequest, UpdateRepoRequest,
};

/// Orchestrates use cases over the `Repo` aggregate.
///
/// Metadata methods are synchronous: `Repo` has no async collaborators, so a plain
/// `std::sync::Mutex` guard held across each load-mutate-save is enough to prevent lost
/// updates. Git object operations are delegated to the `GitBackend` port; they are
/// blocking and in-process, so callers on an async runtime offload these calls. Holds
/// the project repository to verify the parent project exists.
pub struct RepoApplicationService<R: RepoRepository, P: ProjectRepository> {
    repo_repository: R,
    project_repository: P,
    numbering: Arc<dyn RepoNumbering>,
    git_backend: Arc<dyn GitBackend>,
    mutation_guard: Mutex<()>,
}

impl<R: RepoRepository, P: ProjectRepository> RepoApplicationService<R, P> {
    pub fn new(
        repo_repository: R,
        project_repository: P,
        numbering: Arc<dyn RepoNumbering>,
        git_backend: Arc<dyn GitBackend>,
    ) -> Self {
        Self {
            repo_repository,
            project_repository,
            numbering,
            git_backend,
            mutation_guard: Mutex::new(()),
        }
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub fn list_repos(&self, project_id: &str) -> anyhow::Result<Option<Vec<RepoSnapshot>>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).is_none() {
            return Ok(None);
        }
        let mut repos = self
            .repo_repository
            .list()
            .into_iter()
            .filter(|repo| repo.project_id() == id)
            .collect::<Vec<_>>();
        repos.sort_by_key(|repo| repo.id().number());
        Ok(Some(
            repos.into_iter().map(|repo| repo.snapshot()).collect(),
        ))
    }

    pub fn repo_snapshot(&self, repo_id: &str) -> anyhow::Result<Option<RepoSnapshot>> {
        let id: RepoId = repo_id.parse()?;
        Ok(self.repo_repository.get(&id).map(|repo| repo.snapshot()))
    }

    /// Creates the aggregate and initializes its hosted bare git repository. Returns
    /// `Ok(None)` when no project with the given id exists. If the bare repo cannot be
    /// initialized, no metadata is saved, keeping the two consistent.
    pub fn create_repo(
        &self,
        project_id: &str,
        request: CreateRepoRequest,
    ) -> anyhow::Result<Option<RepoSnapshot>> {
        let _guard = self.lock();
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).is_none() {
            return Ok(None);
        }
        let visibility = match request.visibility {
            Some(value) => value.parse::<Visibility>()?,
            None => Visibility::Private,
        };
        let repo = Repo::new(
            self.numbering.next(),
            id,
            request.name,
            request.description,
            visibility,
        )?;
        self.git_backend.init_bare(&repo.id())?;
        let snapshot = repo.snapshot();
        self.repo_repository.save(repo);
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no repo with the given id exists.
    pub fn set_visibility(
        &self,
        repo_id: &str,
        request: SetVisibilityRequest,
    ) -> anyhow::Result<Option<RepoSnapshot>> {
        let _guard = self.lock();
        let id: RepoId = repo_id.parse()?;
        let Some(mut repo) = self.repo_repository.get(&id) else {
            return Ok(None);
        };
        repo.set_visibility(request.visibility.parse()?);
        let snapshot = repo.snapshot();
        self.repo_repository.save(repo);
        Ok(Some(snapshot))
    }

    /// The repo's visibility for the anonymous-read decision. `Ok(None)` if missing.
    pub fn repo_visibility(&self, repo_id: &str) -> anyhow::Result<Option<Visibility>> {
        let id: RepoId = repo_id.parse()?;
        Ok(self.repo_repository.get(&id).map(|repo| repo.visibility()))
    }

    /// Local branches and their tips. `Ok(None)` when the repo does not exist.
    pub fn list_branches(&self, repo_id: &str) -> anyhow::Result<Option<Vec<BranchSnapshot>>> {
        let id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&id).is_none() {
            return Ok(None);
        }
        Ok(Some(self.git_backend.branches(&id)?))
    }

    /// Entries directly under `dir` (root when empty) at the tip of `branch`.
    /// `Ok(None)` when the repo does not exist.
    pub fn list_files(
        &self,
        repo_id: &str,
        branch: &str,
        dir: &str,
    ) -> anyhow::Result<Option<Vec<FileEntrySnapshot>>> {
        let id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&id).is_none() {
            return Ok(None);
        }
        let branch: BranchName = branch.parse()?;
        Ok(Some(self.git_backend.list_files(&id, &branch, dir)?))
    }

    /// Raw bytes of `path` at the tip of `branch`. `Ok(None)` when the repo does not exist.
    pub fn read_file(
        &self,
        repo_id: &str,
        branch: &str,
        path: &str,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&id).is_none() {
            return Ok(None);
        }
        let branch: BranchName = branch.parse()?;
        Ok(Some(self.git_backend.read_file(&id, &branch, path)?))
    }

    /// Most recent commits on `branch`, newest first. `Ok(None)` when the repo does not exist.
    pub fn recent_commits(
        &self,
        repo_id: &str,
        branch: &str,
        limit: usize,
    ) -> anyhow::Result<Option<Vec<CommitSnapshot>>> {
        let id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&id).is_none() {
            return Ok(None);
        }
        let branch: BranchName = branch.parse()?;
        Ok(Some(self.git_backend.recent_commits(&id, &branch, limit)?))
    }

    /// Applies a server-side commit and returns the resulting commit. `Ok(None)` when
    /// the repo does not exist. The mutation guard serializes this against concurrent
    /// REST commits to the same on-disk repo.
    pub fn commit_changes(
        &self,
        repo_id: &str,
        request: CommitChangesRequest,
    ) -> anyhow::Result<Option<CommitSnapshot>> {
        let _guard = self.lock();
        let id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&id).is_none() {
            return Ok(None);
        }
        let branch: BranchName = request.branch.parse()?;
        self.git_backend.commit_changes(
            &id,
            &branch,
            &request.author_name,
            &request.author_email,
            &request.message,
            request.changes,
        )?;
        let head = self
            .git_backend
            .recent_commits(&id, &branch, 1)?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("commit created but not found on branch"))?;
        Ok(Some(head))
    }

    /// Returns `Ok(None)` when no repo with the given id exists.
    pub fn update_repo(
        &self,
        repo_id: &str,
        request: UpdateRepoRequest,
    ) -> anyhow::Result<Option<RepoSnapshot>> {
        let _guard = self.lock();
        let id: RepoId = repo_id.parse()?;
        let Some(mut repo) = self.repo_repository.get(&id) else {
            return Ok(None);
        };
        repo.update(request.name, request.description)?;
        let snapshot = repo.snapshot();
        self.repo_repository.save(repo);
        Ok(Some(snapshot))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_guard
            .lock()
            .expect("repo mutation guard poisoned")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::organization::OrganizationId;
    use wiab_core::project::Project;
    use wiab_core::repo::GitBackendError;

    use super::*;

    #[derive(Default)]
    struct TestRepoRepository {
        repos: RwLock<HashMap<RepoId, Repo>>,
    }

    impl RepoRepository for TestRepoRepository {
        fn save(&self, repo: Repo) {
            self.repos
                .write()
                .expect("test repository write lock poisoned")
                .insert(repo.id(), repo);
        }

        fn get(&self, id: &RepoId) -> Option<Repo> {
            self.repos
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .cloned()
        }

        fn list(&self) -> Vec<Repo> {
            self.repos
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .cloned()
                .collect()
        }
    }

    #[derive(Default)]
    struct TestProjectRepository {
        projects: RwLock<HashMap<ProjectId, Project>>,
    }

    impl ProjectRepository for TestProjectRepository {
        fn save(&self, project: Project) {
            self.projects
                .write()
                .expect("test repository write lock poisoned")
                .insert(project.id(), project);
        }

        fn get(&self, id: &ProjectId) -> Option<Project> {
            self.projects
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .cloned()
        }

        fn list(&self) -> Vec<Project> {
            self.projects
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .cloned()
                .collect()
        }
    }

    #[derive(Default)]
    struct TestRepoNumbering {
        counter: AtomicU64,
    }

    impl RepoNumbering for TestRepoNumbering {
        fn next(&self) -> RepoId {
            RepoId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    /// In-memory `GitBackend` used to assert routing without touching disk. Keyed by
    /// (repo, branch); commits are stored newest-last and returned newest-first.
    #[derive(Default)]
    struct TestGitBackend {
        inited: Mutex<Vec<RepoId>>,
        files: Mutex<HashMap<(RepoId, String), HashMap<String, Vec<u8>>>>,
        commits: Mutex<HashMap<(RepoId, String), Vec<CommitSnapshot>>>,
        next_hash: AtomicU64,
    }

    impl GitBackend for TestGitBackend {
        fn init_bare(&self, id: &RepoId) -> Result<(), GitBackendError> {
            self.inited.lock().unwrap().push(*id);
            Ok(())
        }

        fn branches(&self, id: &RepoId) -> Result<Vec<BranchSnapshot>, GitBackendError> {
            let commits = self.commits.lock().unwrap();
            Ok(commits
                .iter()
                .filter(|((rid, _), _)| rid == id)
                .map(|((_, branch), list)| BranchSnapshot {
                    name: branch.clone(),
                    target: list.last().map(|c| c.hash.clone()).unwrap_or_default(),
                })
                .collect())
        }

        fn list_files(
            &self,
            id: &RepoId,
            branch: &BranchName,
            _dir: &str,
        ) -> Result<Vec<FileEntrySnapshot>, GitBackendError> {
            let files = self.files.lock().unwrap();
            let map = files.get(&(*id, branch.as_str().to_owned())).cloned();
            Ok(map
                .unwrap_or_default()
                .into_keys()
                .map(|path| FileEntrySnapshot {
                    path,
                    is_dir: false,
                })
                .collect())
        }

        fn read_file(
            &self,
            id: &RepoId,
            branch: &BranchName,
            path: &str,
        ) -> Result<Vec<u8>, GitBackendError> {
            self.files
                .lock()
                .unwrap()
                .get(&(*id, branch.as_str().to_owned()))
                .and_then(|m| m.get(path).cloned())
                .ok_or_else(|| GitBackendError::PathNotFound(path.to_owned()))
        }

        fn recent_commits(
            &self,
            id: &RepoId,
            branch: &BranchName,
            limit: usize,
        ) -> Result<Vec<CommitSnapshot>, GitBackendError> {
            let commits = self.commits.lock().unwrap();
            Ok(commits
                .get(&(*id, branch.as_str().to_owned()))
                .map(|list| list.iter().rev().take(limit).cloned().collect())
                .unwrap_or_default())
        }

        fn commit_changes(
            &self,
            id: &RepoId,
            branch: &BranchName,
            author_name: &str,
            _author_email: &str,
            message: &str,
            changes: Vec<wiab_core::repo::FileChange>,
        ) -> Result<wiab_core::repo::CommitHash, GitBackendError> {
            let key = (*id, branch.as_str().to_owned());
            let mut files = self.files.lock().unwrap();
            let map = files.entry(key.clone()).or_default();
            for change in changes {
                map.insert(change.path, change.content);
            }
            let hash = format!("{:040x}", self.next_hash.fetch_add(1, Ordering::SeqCst) + 1);
            self.commits
                .lock()
                .unwrap()
                .entry(key)
                .or_default()
                .push(CommitSnapshot {
                    hash: hash.clone(),
                    message: message.to_owned(),
                    author: author_name.to_owned(),
                    time_unix: 0,
                    parents: Vec::new(),
                });
            Ok(wiab_core::repo::CommitHash::new(hash).unwrap())
        }
    }

    fn service() -> RepoApplicationService<TestRepoRepository, TestProjectRepository> {
        RepoApplicationService::new(
            TestRepoRepository::default(),
            TestProjectRepository::default(),
            Arc::new(TestRepoNumbering::default()),
            Arc::new(TestGitBackend::default()),
        )
    }

    fn seed_project(
        service: &RepoApplicationService<TestRepoRepository, TestProjectRepository>,
        number: u64,
    ) -> String {
        let project = Project::new(
            ProjectId::from_number(number),
            OrganizationId::from_number(1),
            format!("Project {number}"),
            String::new(),
        )
        .unwrap();
        let id = project.id().to_string();
        service.project_repository.save(project);
        id
    }

    fn create(
        service: &RepoApplicationService<TestRepoRepository, TestProjectRepository>,
        project_id: &str,
        name: &str,
    ) -> RepoSnapshot {
        service
            .create_repo(
                project_id,
                CreateRepoRequest {
                    name: name.to_owned(),
                    description: String::new(),
                    visibility: None,
                },
            )
            .expect("project id should be valid")
            .expect("project should exist")
    }

    #[test]
    fn create_repo_assigns_incrementing_ids() {
        let service = service();
        let project_id = seed_project(&service, 1);
        assert_eq!(create(&service, &project_id, "First").id, "R-1");
        assert_eq!(create(&service, &project_id, "Second").id, "R-2");
    }

    #[test]
    fn create_repo_records_project_id() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let repo = create(&service, &project_id, "backend");
        assert_eq!(repo.project_id, project_id);
    }

    #[test]
    fn create_repo_under_missing_project_returns_none() {
        let service = service();
        let result = service
            .create_repo(
                "P-9",
                CreateRepoRequest {
                    name: "backend".to_owned(),
                    description: String::new(),
                    visibility: None,
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn create_repo_rejects_malformed_project_id() {
        let service = service();
        assert!(
            service
                .create_repo(
                    "bogus",
                    CreateRepoRequest {
                        name: "backend".to_owned(),
                        description: String::new(),
                        visibility: None,
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn create_repo_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1);
        assert!(
            service
                .create_repo(
                    &project_id,
                    CreateRepoRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                        visibility: None,
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn list_repos_partitions_by_project() {
        let service = service();
        let first_project = seed_project(&service, 1);
        let second_project = seed_project(&service, 2);
        create(&service, &first_project, "First");
        create(&service, &second_project, "Second");
        create(&service, &first_project, "Third");
        service.repo_repository.save(
            Repo::new(
                RepoId::from_number(10),
                ProjectId::from_number(1),
                "Tenth".to_owned(),
                String::new(),
                Visibility::Private,
            )
            .unwrap(),
        );

        let first_ids = service
            .list_repos(&first_project)
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|repo| repo.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["R-1", "R-3", "R-10"]);

        let second_ids = service
            .list_repos(&second_project)
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|repo| repo.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["R-2"]);
    }

    #[test]
    fn list_repos_for_missing_project_returns_none() {
        let service = service();
        assert!(service.list_repos("P-9").unwrap().is_none());
    }

    #[test]
    fn list_repos_rejects_malformed_project_id() {
        let service = service();
        assert!(service.list_repos("bogus").is_err());
    }

    #[test]
    fn repo_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.repo_snapshot("R-9").unwrap().is_none());
    }

    #[test]
    fn repo_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.repo_snapshot("bogus").is_err());
    }

    #[test]
    fn update_repo_replaces_fields_but_not_project() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let repo = create(&service, &project_id, "backend");
        let updated = service
            .update_repo(
                &repo.id,
                UpdateRepoRequest {
                    name: "frontend".to_owned(),
                    description: "react app".to_owned(),
                },
            )
            .unwrap()
            .expect("repo should exist");
        assert_eq!(updated.name, "frontend");
        assert_eq!(updated.description, "react app");
        assert_eq!(updated.project_id, project_id);

        let reloaded = service
            .repo_snapshot(&repo.id)
            .unwrap()
            .expect("repo should exist");
        assert_eq!(reloaded.name, "frontend");
    }

    #[test]
    fn update_missing_repo_returns_none() {
        let service = service();
        let result = service
            .update_repo(
                "R-9",
                UpdateRepoRequest {
                    name: "frontend".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_repo_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let repo = create(&service, &project_id, "backend");
        assert!(
            service
                .update_repo(
                    &repo.id,
                    UpdateRepoRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn create_repo_initializes_bare_repo_and_defaults_private() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let repo = create(&service, &project_id, "backend");
        assert_eq!(repo.id, "R-1");
        assert_eq!(repo.visibility, "private");
    }

    #[test]
    fn create_repo_honors_requested_visibility() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let repo = service
            .create_repo(
                &project_id,
                CreateRepoRequest {
                    name: "public-repo".to_owned(),
                    description: String::new(),
                    visibility: Some("public".to_owned()),
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(repo.visibility, "public");
    }

    #[test]
    fn set_visibility_updates_the_repo() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let repo = create(&service, &project_id, "backend");
        let updated = service
            .set_visibility(
                &repo.id,
                SetVisibilityRequest {
                    visibility: "public".to_owned(),
                },
            )
            .unwrap()
            .expect("repo should exist");
        assert_eq!(updated.visibility, "public");
        assert_eq!(
            service
                .repo_visibility(&repo.id)
                .unwrap()
                .map(|v| v.is_public()),
            Some(true)
        );
    }

    #[test]
    fn set_visibility_for_missing_repo_returns_none() {
        let service = service();
        assert!(
            service
                .set_visibility(
                    "R-9",
                    SetVisibilityRequest {
                        visibility: "public".to_owned(),
                    },
                )
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn commit_changes_then_browse_round_trips() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let repo = create(&service, &project_id, "backend");

        let commit = service
            .commit_changes(
                &repo.id,
                CommitChangesRequest {
                    branch: "main".to_owned(),
                    author_name: "Ada".to_owned(),
                    author_email: "ada@example.com".to_owned(),
                    message: "initial".to_owned(),
                    changes: vec![wiab_core::repo::FileChange {
                        path: "README.md".to_owned(),
                        content: b"hello".to_vec(),
                    }],
                },
            )
            .unwrap()
            .expect("repo should exist");
        assert_eq!(commit.message, "initial");

        let branches = service.list_branches(&repo.id).unwrap().unwrap();
        assert_eq!(
            branches.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            vec!["main"]
        );

        let bytes = service
            .read_file(&repo.id, "main", "README.md")
            .unwrap()
            .unwrap();
        assert_eq!(bytes, b"hello");

        let commits = service
            .recent_commits(&repo.id, "main", 10)
            .unwrap()
            .unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].message, "initial");
    }

    #[test]
    fn git_browse_for_missing_repo_returns_none() {
        let service = service();
        assert!(service.list_branches("R-9").unwrap().is_none());
        assert!(service.recent_commits("R-9", "main", 5).unwrap().is_none());
    }
}
