use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::pipeline::{
    Pipeline, PipelineId, PipelineNumbering, PipelineRepository, PipelineSnapshot,
};
use wiab_core::project::{ProjectId, ProjectRepository};
use wiab_core::repository::{SaveError, Version};

use crate::pipeline_requests::{CreatePipelineRequest, UpdatePipelineRequest};

/// Orchestrates use cases over the `Pipeline` aggregate.
///
/// Methods are async and fallible: persistence may be remote. Lost updates are prevented by
/// optimistic concurrency — a mutation loads the aggregate with its version, applies the
/// change, and retries when a concurrent save advanced the version in between. Holds the
/// project repository to verify the parent project exists.
pub struct PipelineApplicationService<L: PipelineRepository, P: ProjectRepository> {
    pipeline_repository: L,
    project_repository: P,
    numbering: Arc<dyn PipelineNumbering>,
}

impl<L: PipelineRepository, P: ProjectRepository> PipelineApplicationService<L, P> {
    pub fn new(
        pipeline_repository: L,
        project_repository: P,
        numbering: Arc<dyn PipelineNumbering>,
    ) -> Self {
        Self {
            pipeline_repository,
            project_repository,
            numbering,
        }
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub async fn list_pipelines(
        &self,
        project_id: &str,
    ) -> anyhow::Result<Option<Vec<PipelineSnapshot>>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let mut pipelines = self
            .pipeline_repository
            .list()
            .await?
            .into_iter()
            .filter(|pipeline| pipeline.project_id() == id)
            .collect::<Vec<_>>();
        pipelines.sort_by_key(|pipeline| pipeline.id().number());
        Ok(Some(
            pipelines
                .into_iter()
                .map(|pipeline| pipeline.snapshot())
                .collect(),
        ))
    }

    pub async fn pipeline_snapshot(
        &self,
        pipeline_id: &str,
    ) -> anyhow::Result<Option<PipelineSnapshot>> {
        let id: PipelineId = pipeline_id.parse()?;
        Ok(self
            .pipeline_repository
            .get(&id)
            .await?
            .map(|(pipeline, _)| pipeline.snapshot()))
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub async fn create_pipeline(
        &self,
        project_id: &str,
        request: CreatePipelineRequest,
    ) -> anyhow::Result<Option<PipelineSnapshot>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let pipeline = Pipeline::new(self.numbering.next(), id, request.name, request.description)?;
        let snapshot = pipeline.snapshot();
        self.pipeline_repository
            .save(pipeline, Version::NEW)
            .await?;
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no pipeline with the given id exists.
    pub async fn update_pipeline(
        &self,
        pipeline_id: &str,
        request: UpdatePipelineRequest,
    ) -> anyhow::Result<Option<PipelineSnapshot>> {
        let id: PipelineId = pipeline_id.parse()?;
        loop {
            let Some((mut pipeline, version)) = self.pipeline_repository.get(&id).await? else {
                return Ok(None);
            };
            pipeline.update(request.name.clone(), request.description.clone())?;
            let snapshot = pipeline.snapshot();
            match self.pipeline_repository.save(pipeline, version).await {
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
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::organization::OrganizationId;
    use wiab_core::project::Project;
    use wiab_core::repository::{RepoError, SaveError, Version};

    use super::*;

    #[derive(Default)]
    struct TestPipelineRepository {
        pipelines: RwLock<HashMap<PipelineId, (Pipeline, u64)>>,
    }

    impl PipelineRepository for TestPipelineRepository {
        async fn save(&self, pipeline: Pipeline, expected: Version) -> Result<Version, SaveError> {
            let mut pipelines = self
                .pipelines
                .write()
                .expect("test repository write lock poisoned");
            let current = pipelines
                .get(&pipeline.id())
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            pipelines.insert(pipeline.id(), (pipeline, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &PipelineId) -> Result<Option<(Pipeline, Version)>, RepoError> {
            Ok(self
                .pipelines
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(pipeline, version)| (pipeline.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Pipeline>, RepoError> {
            Ok(self
                .pipelines
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(pipeline, _)| pipeline.clone())
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
    struct TestPipelineNumbering {
        counter: AtomicU64,
    }

    impl PipelineNumbering for TestPipelineNumbering {
        fn next(&self) -> PipelineId {
            PipelineId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    fn service() -> PipelineApplicationService<TestPipelineRepository, TestProjectRepository> {
        PipelineApplicationService::new(
            TestPipelineRepository::default(),
            TestProjectRepository::default(),
            Arc::new(TestPipelineNumbering::default()),
        )
    }

    async fn seed_project(
        service: &PipelineApplicationService<TestPipelineRepository, TestProjectRepository>,
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
        service: &PipelineApplicationService<TestPipelineRepository, TestProjectRepository>,
        project_id: &str,
        name: &str,
    ) -> PipelineSnapshot {
        service
            .create_pipeline(
                project_id,
                CreatePipelineRequest {
                    name: name.to_owned(),
                    description: String::new(),
                },
            )
            .await
            .expect("project id should be valid")
            .expect("project should exist")
    }

    #[tokio::test]
    async fn create_pipeline_assigns_incrementing_ids() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        assert_eq!(create(&service, &project_id, "First").await.id, "PL-1");
        assert_eq!(create(&service, &project_id, "Second").await.id, "PL-2");
    }

    #[tokio::test]
    async fn create_pipeline_records_project_id() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let pipeline = create(&service, &project_id, "Deploy").await;
        assert_eq!(pipeline.project_id, project_id);
    }

    #[tokio::test]
    async fn create_pipeline_under_missing_project_returns_none() {
        let service = service();
        let result = service
            .create_pipeline(
                "P-9",
                CreatePipelineRequest {
                    name: "Deploy".to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn create_pipeline_rejects_malformed_project_id() {
        let service = service();
        assert!(
            service
                .create_pipeline(
                    "bogus",
                    CreatePipelineRequest {
                        name: "Deploy".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn create_pipeline_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        assert!(
            service
                .create_pipeline(
                    &project_id,
                    CreatePipelineRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn list_pipelines_partitions_by_project() {
        let service = service();
        let first_project = seed_project(&service, 1).await;
        let second_project = seed_project(&service, 2).await;
        create(&service, &first_project, "First").await;
        create(&service, &second_project, "Second").await;
        create(&service, &first_project, "Third").await;
        service
            .pipeline_repository
            .save(
                Pipeline::new(
                    PipelineId::from_number(10),
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
            .list_pipelines(&first_project)
            .await
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|pipeline| pipeline.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["PL-1", "PL-3", "PL-10"]);

        let second_ids = service
            .list_pipelines(&second_project)
            .await
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|pipeline| pipeline.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["PL-2"]);
    }

    #[tokio::test]
    async fn list_pipelines_for_missing_project_returns_none() {
        let service = service();
        assert!(service.list_pipelines("P-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_pipelines_rejects_malformed_project_id() {
        let service = service();
        assert!(service.list_pipelines("bogus").await.is_err());
    }

    #[tokio::test]
    async fn pipeline_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.pipeline_snapshot("PL-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn pipeline_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.pipeline_snapshot("bogus").await.is_err());
    }

    #[tokio::test]
    async fn update_pipeline_replaces_fields_but_not_project() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let pipeline = create(&service, &project_id, "Deploy").await;
        let updated = service
            .update_pipeline(
                &pipeline.id,
                UpdatePipelineRequest {
                    name: "Release".to_owned(),
                    description: "to prod".to_owned(),
                },
            )
            .await
            .unwrap()
            .expect("pipeline should exist");
        assert_eq!(updated.name, "Release");
        assert_eq!(updated.description, "to prod");
        assert_eq!(updated.project_id, project_id);

        let reloaded = service
            .pipeline_snapshot(&pipeline.id)
            .await
            .unwrap()
            .expect("pipeline should exist");
        assert_eq!(reloaded.name, "Release");
    }

    #[tokio::test]
    async fn update_missing_pipeline_returns_none() {
        let service = service();
        let result = service
            .update_pipeline(
                "PL-9",
                UpdatePipelineRequest {
                    name: "Release".to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_pipeline_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let pipeline = create(&service, &project_id, "Deploy").await;
        assert!(
            service
                .update_pipeline(
                    &pipeline.id,
                    UpdatePipelineRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }
}
