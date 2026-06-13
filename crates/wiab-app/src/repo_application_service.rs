use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::project::{ProjectId, ProjectRepository};
use wiab_core::repo::{
    BranchName, BranchSnapshot, CommitSnapshot, FileEntrySnapshot, GitBackend, Repo, RepoId,
    RepoNumbering, RepoRepository, RepoSnapshot, Visibility,
};
use wiab_core::repository::{SaveError, Version};

use crate::repo_requests::{
    CommitChangesRequest, CreateRepoRequest, SetVisibilityRequest, UpdateRepoRequest,
};

/// Orchestrates use cases over the `Repo` aggregate.
///
/// Metadata methods are async and fallible: persistence may be remote, and lost updates are
/// prevented by optimistic concurrency — a mutation loads the aggregate with its version,
/// applies the change, and retries when a concurrent save advanced the version in between.
/// Git object operations are delegated to the `GitBackend` port; they are blocking and
/// in-process, so callers on an async runtime offload these calls. Holds the project
/// repository to verify the parent project exists.
pub struct RepoApplicationService<R: RepoRepository, P: ProjectRepository> {
    repo_repository: R,
    project_repository: P,
    numbering: Arc<dyn RepoNumbering>,
    git_backend: Arc<dyn GitBackend>,
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
        }
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub async fn list_repos(&self, project_id: &str) -> anyhow::Result<Option<Vec<RepoSnapshot>>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let mut repos = self
            .repo_repository
            .list()
            .await?
            .into_iter()
            .filter(|repo| repo.project_id() == id)
            .collect::<Vec<_>>();
        repos.sort_by_key(|repo| repo.id().number());
        Ok(Some(
            repos.into_iter().map(|repo| repo.snapshot()).collect(),
        ))
    }

    pub async fn repo_snapshot(&self, repo_id: &str) -> anyhow::Result<Option<RepoSnapshot>> {
        let id: RepoId = repo_id.parse()?;
        Ok(self
            .repo_repository
            .get(&id)
            .await?
            .map(|(repo, _)| repo.snapshot()))
    }

    /// Creates the aggregate and initializes its hosted bare git repository. Returns
    /// `Ok(None)` when no project with the given id exists. If the bare repo cannot be
    /// initialized, no metadata is saved, keeping the two consistent.
    pub async fn create_repo(
        &self,
        project_id: &str,
        request: CreateRepoRequest,
    ) -> anyhow::Result<Option<RepoSnapshot>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).await?.is_none() {
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
        let git_backend = self.git_backend.clone();
        let repo_id = repo.id();
        tokio::task::spawn_blocking(move || git_backend.init_bare(&repo_id)).await??;
        let snapshot = repo.snapshot();
        self.repo_repository.save(repo, Version::NEW).await?;
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no repo with the given id exists.
    pub async fn set_visibility(
        &self,
        repo_id: &str,
        request: SetVisibilityRequest,
    ) -> anyhow::Result<Option<RepoSnapshot>> {
        let id: RepoId = repo_id.parse()?;
        loop {
            let Some((mut repo, version)) = self.repo_repository.get(&id).await? else {
                return Ok(None);
            };
            repo.set_visibility(request.visibility.parse()?);
            let snapshot = repo.snapshot();
            match self.repo_repository.save(repo, version).await {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    /// The repo's visibility for the anonymous-read decision. `Ok(None)` if missing.
    pub async fn repo_visibility(&self, repo_id: &str) -> anyhow::Result<Option<Visibility>> {
        let id: RepoId = repo_id.parse()?;
        Ok(self
            .repo_repository
            .get(&id)
            .await?
            .map(|(repo, _)| repo.visibility()))
    }

    /// Local branches and their tips. `Ok(None)` when the repo does not exist.
    pub async fn list_branches(
        &self,
        repo_id: &str,
    ) -> anyhow::Result<Option<Vec<BranchSnapshot>>> {
        let id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let git_backend = self.git_backend.clone();
        let branches = tokio::task::spawn_blocking(move || git_backend.branches(&id)).await??;
        Ok(Some(branches))
    }

    /// Entries directly under `dir` (root when empty) at the tip of `branch`.
    /// `Ok(None)` when the repo does not exist.
    pub async fn list_files(
        &self,
        repo_id: &str,
        branch: &str,
        dir: &str,
    ) -> anyhow::Result<Option<Vec<FileEntrySnapshot>>> {
        let id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let branch: BranchName = branch.parse()?;
        let git_backend = self.git_backend.clone();
        let dir = dir.to_owned();
        let entries =
            tokio::task::spawn_blocking(move || git_backend.list_files(&id, &branch, &dir))
                .await??;
        Ok(Some(entries))
    }

    /// Raw bytes of `path` at the tip of `branch`. `Ok(None)` when the repo does not exist.
    pub async fn read_file(
        &self,
        repo_id: &str,
        branch: &str,
        path: &str,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let branch: BranchName = branch.parse()?;
        let git_backend = self.git_backend.clone();
        let path = path.to_owned();
        let bytes = tokio::task::spawn_blocking(move || git_backend.read_file(&id, &branch, &path))
            .await??;
        Ok(Some(bytes))
    }

    /// Most recent commits on `branch`, newest first. `Ok(None)` when the repo does not exist.
    pub async fn recent_commits(
        &self,
        repo_id: &str,
        branch: &str,
        limit: usize,
    ) -> anyhow::Result<Option<Vec<CommitSnapshot>>> {
        let id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let branch: BranchName = branch.parse()?;
        let git_backend = self.git_backend.clone();
        let commits =
            tokio::task::spawn_blocking(move || git_backend.recent_commits(&id, &branch, limit))
                .await??;
        Ok(Some(commits))
    }

    /// Applies a server-side commit and returns the resulting commit. `Ok(None)` when
    /// the repo does not exist.
    pub async fn commit_changes(
        &self,
        repo_id: &str,
        request: CommitChangesRequest,
    ) -> anyhow::Result<Option<CommitSnapshot>> {
        let id: RepoId = repo_id.parse()?;
        if self.repo_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let branch: BranchName = request.branch.parse()?;
        let git_backend = self.git_backend.clone();
        let head = tokio::task::spawn_blocking(move || -> anyhow::Result<CommitSnapshot> {
            git_backend.commit_changes(
                &id,
                &branch,
                &request.author_name,
                &request.author_email,
                &request.message,
                request.changes,
            )?;
            git_backend
                .recent_commits(&id, &branch, 1)?
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("commit created but not found on branch"))
        })
        .await??;
        Ok(Some(head))
    }

    /// Returns `Ok(None)` when no repo with the given id exists.
    pub async fn update_repo(
        &self,
        repo_id: &str,
        request: UpdateRepoRequest,
    ) -> anyhow::Result<Option<RepoSnapshot>> {
        let id: RepoId = repo_id.parse()?;
        loop {
            let Some((mut repo, version)) = self.repo_repository.get(&id).await? else {
                return Ok(None);
            };
            repo.update(request.name.clone(), request.description.clone())?;
            let snapshot = repo.snapshot();
            match self.repo_repository.save(repo, version).await {
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
    use std::sync::Mutex;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::organization::OrganizationId;
    use wiab_core::project::Project;
    use wiab_core::repo::GitBackendError;
    use wiab_core::repository::{RepoError, SaveError, Version};

    use super::*;

    #[derive(Default)]
    struct TestRepoRepository {
        repos: RwLock<HashMap<RepoId, (Repo, u64)>>,
    }

    impl RepoRepository for TestRepoRepository {
        async fn save(&self, repo: Repo, expected: Version) -> Result<Version, SaveError> {
            let mut repos = self
                .repos
                .write()
                .expect("test repository write lock poisoned");
            let current = repos
                .get(&repo.id())
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            repos.insert(repo.id(), (repo, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &RepoId) -> Result<Option<(Repo, Version)>, RepoError> {
            Ok(self
                .repos
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(repo, version)| (repo.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Repo>, RepoError> {
            Ok(self
                .repos
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(repo, _)| repo.clone())
                .collect())
        }
    }

    #[derive(Default)]
    struct TestProjectRepository {
        projects: RwLock<HashMap<ProjectId, (Project, u64)>>,
    }

    impl ProjectRepository for TestProjectRepository {
        async fn save(&self, project: Project, expected: Version) -> Result<Version, SaveError> {
            let mut projects = self
                .projects
                .write()
                .expect("test repository write lock poisoned");
            let current = projects
                .get(&project.id())
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            projects.insert(project.id(), (project, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &ProjectId) -> Result<Option<(Project, Version)>, RepoError> {
            Ok(self
                .projects
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(project, version)| (project.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Project>, RepoError> {
            Ok(self
                .projects
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(project, _)| project.clone())
                .collect())
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

    async fn seed_project(
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
        service
            .project_repository
            .save(project, Version::NEW)
            .await
            .unwrap();
        id
    }

    async fn create(
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
            .await
            .expect("project id should be valid")
            .expect("project should exist")
    }

    #[tokio::test]
    async fn create_repo_assigns_incrementing_ids() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        assert_eq!(create(&service, &project_id, "First").await.id, "R-1");
        assert_eq!(create(&service, &project_id, "Second").await.id, "R-2");
    }

    #[tokio::test]
    async fn create_repo_records_project_id() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let repo = create(&service, &project_id, "backend").await;
        assert_eq!(repo.project_id, project_id);
    }

    #[tokio::test]
    async fn create_repo_under_missing_project_returns_none() {
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
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn create_repo_rejects_malformed_project_id() {
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
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn create_repo_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
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
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn list_repos_partitions_by_project() {
        let service = service();
        let first_project = seed_project(&service, 1).await;
        let second_project = seed_project(&service, 2).await;
        create(&service, &first_project, "First").await;
        create(&service, &second_project, "Second").await;
        create(&service, &first_project, "Third").await;
        service
            .repo_repository
            .save(
                Repo::new(
                    RepoId::from_number(10),
                    ProjectId::from_number(1),
                    "Tenth".to_owned(),
                    String::new(),
                    Visibility::Private,
                )
                .unwrap(),
                Version::NEW,
            )
            .await
            .unwrap();

        let first_ids = service
            .list_repos(&first_project)
            .await
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|repo| repo.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["R-1", "R-3", "R-10"]);

        let second_ids = service
            .list_repos(&second_project)
            .await
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|repo| repo.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["R-2"]);
    }

    #[tokio::test]
    async fn list_repos_for_missing_project_returns_none() {
        let service = service();
        assert!(service.list_repos("P-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_repos_rejects_malformed_project_id() {
        let service = service();
        assert!(service.list_repos("bogus").await.is_err());
    }

    #[tokio::test]
    async fn repo_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.repo_snapshot("R-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn repo_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.repo_snapshot("bogus").await.is_err());
    }

    #[tokio::test]
    async fn update_repo_replaces_fields_but_not_project() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let repo = create(&service, &project_id, "backend").await;
        let updated = service
            .update_repo(
                &repo.id,
                UpdateRepoRequest {
                    name: "frontend".to_owned(),
                    description: "react app".to_owned(),
                },
            )
            .await
            .unwrap()
            .expect("repo should exist");
        assert_eq!(updated.name, "frontend");
        assert_eq!(updated.description, "react app");
        assert_eq!(updated.project_id, project_id);

        let reloaded = service
            .repo_snapshot(&repo.id)
            .await
            .unwrap()
            .expect("repo should exist");
        assert_eq!(reloaded.name, "frontend");
    }

    #[tokio::test]
    async fn update_missing_repo_returns_none() {
        let service = service();
        let result = service
            .update_repo(
                "R-9",
                UpdateRepoRequest {
                    name: "frontend".to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_repo_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let repo = create(&service, &project_id, "backend").await;
        assert!(
            service
                .update_repo(
                    &repo.id,
                    UpdateRepoRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn create_repo_initializes_bare_repo_and_defaults_private() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let repo = create(&service, &project_id, "backend").await;
        assert_eq!(repo.id, "R-1");
        assert_eq!(repo.visibility, "private");
    }

    #[tokio::test]
    async fn create_repo_honors_requested_visibility() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let repo = service
            .create_repo(
                &project_id,
                CreateRepoRequest {
                    name: "public-repo".to_owned(),
                    description: String::new(),
                    visibility: Some("public".to_owned()),
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(repo.visibility, "public");
    }

    #[tokio::test]
    async fn set_visibility_updates_the_repo() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let repo = create(&service, &project_id, "backend").await;
        let updated = service
            .set_visibility(
                &repo.id,
                SetVisibilityRequest {
                    visibility: "public".to_owned(),
                },
            )
            .await
            .unwrap()
            .expect("repo should exist");
        assert_eq!(updated.visibility, "public");
        assert_eq!(
            service
                .repo_visibility(&repo.id)
                .await
                .unwrap()
                .map(|v| v.is_public()),
            Some(true)
        );
    }

    #[tokio::test]
    async fn set_visibility_for_missing_repo_returns_none() {
        let service = service();
        assert!(
            service
                .set_visibility(
                    "R-9",
                    SetVisibilityRequest {
                        visibility: "public".to_owned(),
                    },
                )
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn commit_changes_then_browse_round_trips() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let repo = create(&service, &project_id, "backend").await;

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
            .await
            .unwrap()
            .expect("repo should exist");
        assert_eq!(commit.message, "initial");

        let branches = service.list_branches(&repo.id).await.unwrap().unwrap();
        assert_eq!(
            branches.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            vec!["main"]
        );

        let bytes = service
            .read_file(&repo.id, "main", "README.md")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bytes, b"hello");

        let commits = service
            .recent_commits(&repo.id, "main", 10)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].message, "initial");
    }

    #[tokio::test]
    async fn git_browse_for_missing_repo_returns_none() {
        let service = service();
        assert!(service.list_branches("R-9").await.unwrap().is_none());
        assert!(
            service
                .recent_commits("R-9", "main", 5)
                .await
                .unwrap()
                .is_none()
        );
    }
}
