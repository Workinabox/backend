use std::sync::{Arc, Mutex};

use wiab_core::project::{ProjectId, ProjectRepository};
use wiab_core::repo::{Repo, RepoId, RepoNumbering, RepoRepository, RepoSnapshot};

use crate::repo_requests::{CreateRepoRequest, UpdateRepoRequest};

/// Orchestrates use cases over the `Repo` aggregate.
///
/// Methods are synchronous: `Repo` has no external/async collaborators, so a plain
/// `std::sync::Mutex` guard held across each load-mutate-save is enough to prevent lost
/// updates. Holds the project repository to verify the parent project exists.
pub struct RepoApplicationService<R: RepoRepository, P: ProjectRepository> {
    repo_repository: R,
    project_repository: P,
    numbering: Arc<dyn RepoNumbering>,
    mutation_guard: Mutex<()>,
}

impl<R: RepoRepository, P: ProjectRepository> RepoApplicationService<R, P> {
    pub fn new(
        repo_repository: R,
        project_repository: P,
        numbering: Arc<dyn RepoNumbering>,
    ) -> Self {
        Self {
            repo_repository,
            project_repository,
            numbering,
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

    /// Returns `Ok(None)` when no project with the given id exists.
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
        let repo = Repo::new(self.numbering.next(), id, request.name, request.description)?;
        let snapshot = repo.snapshot();
        self.repo_repository.save(repo);
        Ok(Some(snapshot))
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

    fn service() -> RepoApplicationService<TestRepoRepository, TestProjectRepository> {
        RepoApplicationService::new(
            TestRepoRepository::default(),
            TestProjectRepository::default(),
            Arc::new(TestRepoNumbering::default()),
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
}
