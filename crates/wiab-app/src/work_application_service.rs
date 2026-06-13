use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::project::{ProjectId, ProjectRepository};
use wiab_core::repository::{SaveError, Version};
use wiab_core::work::{DoneId, Work, WorkId, WorkNumbering, WorkRepository, WorkSnapshot};

use crate::create_work_request::{AddDoneRequest, CreateWorkRequest, UpdateWorkRequest};

/// Orchestrates use cases over the `Work` aggregate.
///
/// Holds the project repository to verify the parent project exists.
pub struct WorkApplicationService<R: WorkRepository, P: ProjectRepository> {
    work_repository: R,
    project_repository: P,
    numbering: Arc<dyn WorkNumbering>,
}

impl<R: WorkRepository, P: ProjectRepository> WorkApplicationService<R, P> {
    pub fn new(
        work_repository: R,
        project_repository: P,
        numbering: Arc<dyn WorkNumbering>,
    ) -> Self {
        Self {
            work_repository,
            project_repository,
            numbering,
        }
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub async fn list_works_by_project(
        &self,
        project_id: &str,
    ) -> anyhow::Result<Option<Vec<WorkSnapshot>>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let mut works = self
            .work_repository
            .list()
            .await?
            .into_iter()
            .filter(|work| work.project_id() == id)
            .collect::<Vec<_>>();
        works.sort_by_key(|work| work.id().number());
        Ok(Some(
            works.into_iter().map(|work| work.snapshot()).collect(),
        ))
    }

    pub async fn work_snapshot(&self, work_id: &str) -> anyhow::Result<Option<WorkSnapshot>> {
        let id: WorkId = work_id.parse()?;
        Ok(self
            .work_repository
            .get(&id)
            .await?
            .map(|(work, _)| work.snapshot()))
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub async fn create_work(
        &self,
        project_id: &str,
        request: CreateWorkRequest,
    ) -> anyhow::Result<Option<WorkSnapshot>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let work = Work::new(
            self.numbering.next(),
            id,
            request.title,
            request.description,
        )?;
        let snapshot = work.snapshot();
        self.work_repository.save(work, Version::NEW).await?;
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no work with the given id exists.
    pub async fn update_work(
        &self,
        work_id: &str,
        request: UpdateWorkRequest,
    ) -> anyhow::Result<Option<WorkSnapshot>> {
        let id: WorkId = work_id.parse()?;
        loop {
            let Some((mut work, version)) = self.work_repository.get(&id).await? else {
                return Ok(None);
            };
            work.update(request.title.clone(), request.description.clone())?;
            let snapshot = work.snapshot();
            match self.work_repository.save(work, version).await {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    pub async fn add_done(
        &self,
        work_id: &str,
        request: AddDoneRequest,
    ) -> anyhow::Result<WorkSnapshot> {
        let id: WorkId = work_id.parse()?;
        loop {
            let Some((mut work, version)) = self.work_repository.get(&id).await? else {
                return Err(anyhow!("work '{work_id}' not found"));
            };
            work.add_done(request.criterion.clone())?;
            let snapshot = work.snapshot();
            match self.work_repository.save(work, version).await {
                Ok(_) => return Ok(snapshot),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    pub async fn fulfill_done(&self, work_id: &str, done_id: &str) -> anyhow::Result<WorkSnapshot> {
        let id: WorkId = work_id.parse()?;
        let done_id: DoneId = done_id.parse()?;
        loop {
            let Some((mut work, version)) = self.work_repository.get(&id).await? else {
                return Err(anyhow!("work '{work_id}' not found"));
            };
            work.fulfill_done(&done_id)?;
            let snapshot = work.snapshot();
            match self.work_repository.save(work, version).await {
                Ok(_) => return Ok(snapshot),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    pub async fn unfulfill_done(
        &self,
        work_id: &str,
        done_id: &str,
    ) -> anyhow::Result<WorkSnapshot> {
        let id: WorkId = work_id.parse()?;
        let done_id: DoneId = done_id.parse()?;
        loop {
            let Some((mut work, version)) = self.work_repository.get(&id).await? else {
                return Err(anyhow!("work '{work_id}' not found"));
            };
            work.unfulfill_done(&done_id)?;
            let snapshot = work.snapshot();
            match self.work_repository.save(work, version).await {
                Ok(_) => return Ok(snapshot),
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
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::organization::OrganizationId;
    use wiab_core::project::Project;
    use wiab_core::repository::{RepoError, SaveError, Version};

    use super::*;

    #[derive(Default)]
    struct TestWorkRepository {
        works: RwLock<HashMap<WorkId, (Work, u64)>>,
    }

    impl WorkRepository for TestWorkRepository {
        async fn save(&self, work: Work, expected: Version) -> Result<Version, SaveError> {
            let mut works = self
                .works
                .write()
                .expect("test repository write lock poisoned");
            let current = works
                .get(&work.id())
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            works.insert(work.id(), (work, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &WorkId) -> Result<Option<(Work, Version)>, RepoError> {
            Ok(self
                .works
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(work, version)| (work.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Work>, RepoError> {
            Ok(self
                .works
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(work, _)| work.clone())
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
    struct TestWorkNumbering {
        counter: AtomicU64,
    }

    impl WorkNumbering for TestWorkNumbering {
        fn next(&self) -> WorkId {
            WorkId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    fn service() -> WorkApplicationService<TestWorkRepository, TestProjectRepository> {
        WorkApplicationService::new(
            TestWorkRepository::default(),
            TestProjectRepository::default(),
            Arc::new(TestWorkNumbering::default()),
        )
    }

    async fn seed_project(
        service: &WorkApplicationService<TestWorkRepository, TestProjectRepository>,
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
        service: &WorkApplicationService<TestWorkRepository, TestProjectRepository>,
        project_id: &str,
        title: &str,
    ) -> WorkSnapshot {
        service
            .create_work(
                project_id,
                CreateWorkRequest {
                    title: title.to_owned(),
                    description: String::new(),
                },
            )
            .await
            .expect("project id should be valid")
            .expect("project should exist")
    }

    #[tokio::test]
    async fn create_work_assigns_incrementing_ids() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        assert_eq!(create(&service, &project_id, "First").await.id, "W-1");
        assert_eq!(create(&service, &project_id, "Second").await.id, "W-2");
    }

    #[tokio::test]
    async fn create_work_records_project_id() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let work = create(&service, &project_id, "Ship v1").await;
        assert_eq!(work.project_id, project_id);
    }

    #[tokio::test]
    async fn create_work_in_missing_project_returns_none() {
        let service = service();
        let result = service
            .create_work(
                "P-9",
                CreateWorkRequest {
                    title: "Ship v1".to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn create_work_rejects_malformed_project_id() {
        let service = service();
        assert!(
            service
                .create_work(
                    "bogus",
                    CreateWorkRequest {
                        title: "Ship v1".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn list_works_partitions_by_project() {
        let service = service();
        let first_project = seed_project(&service, 1).await;
        let second_project = seed_project(&service, 2).await;
        create(&service, &first_project, "First").await;
        create(&service, &second_project, "Second").await;
        create(&service, &first_project, "Third").await;
        service
            .work_repository
            .save(
                Work::new(
                    WorkId::from_number(10),
                    ProjectId::from_number(1),
                    "Tenth".to_owned(),
                    String::new(),
                )
                .unwrap(),
                Version::NEW,
            )
            .await
            .unwrap();

        let first_ids = service
            .list_works_by_project(&first_project)
            .await
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|work| work.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["W-1", "W-3", "W-10"]);

        let second_ids = service
            .list_works_by_project(&second_project)
            .await
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|work| work.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["W-2"]);
    }

    #[tokio::test]
    async fn list_works_for_missing_project_returns_none() {
        let service = service();
        assert!(
            service
                .list_works_by_project("P-9")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn list_works_rejects_malformed_project_id() {
        let service = service();
        assert!(service.list_works_by_project("bogus").await.is_err());
    }

    #[tokio::test]
    async fn update_work_replaces_fields() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let work = create(&service, &project_id, "Ship v1").await;
        let updated = service
            .update_work(
                &work.id,
                UpdateWorkRequest {
                    title: "Ship v2".to_owned(),
                    description: "the sequel".to_owned(),
                },
            )
            .await
            .unwrap()
            .expect("work should exist");
        assert_eq!(updated.title, "Ship v2");
        assert_eq!(updated.description, "the sequel");

        let reloaded = service
            .work_snapshot(&work.id)
            .await
            .unwrap()
            .expect("work should exist");
        assert_eq!(reloaded.title, "Ship v2");
    }

    #[tokio::test]
    async fn update_missing_work_returns_none() {
        let service = service();
        let result = service
            .update_work(
                "W-99",
                UpdateWorkRequest {
                    title: "Ghost".to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_work_rejects_empty_title() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let work = create(&service, &project_id, "Ship v1").await;
        assert!(
            service
                .update_work(
                    &work.id,
                    UpdateWorkRequest {
                        title: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn add_done_then_fulfill_flips_is_done() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let work = create(&service, &project_id, "Ship v1").await;
        let snapshot = service
            .add_done(
                &work.id,
                AddDoneRequest {
                    criterion: "tests pass".to_owned(),
                },
            )
            .await
            .unwrap();
        assert!(!snapshot.is_done);
        let done_id = snapshot.dones[0].id.clone();

        let snapshot = service.fulfill_done(&work.id, &done_id).await.unwrap();
        assert!(snapshot.is_done);
    }

    #[tokio::test]
    async fn unknown_work_id_errors() {
        let service = service();
        assert!(
            service
                .add_done(
                    "W-99",
                    AddDoneRequest {
                        criterion: "x".to_owned()
                    }
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn unknown_done_id_errors() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let work = create(&service, &project_id, "Ship v1").await;
        let missing = uuid::Uuid::new_v4().to_string();
        assert!(service.fulfill_done(&work.id, &missing).await.is_err());
    }
}
