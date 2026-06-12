use std::sync::{Arc, Mutex};

use wiab_core::pipeline::{
    Pipeline, PipelineId, PipelineNumbering, PipelineRepository, PipelineSnapshot,
};
use wiab_core::project::{ProjectId, ProjectRepository};

use crate::pipeline_requests::{CreatePipelineRequest, UpdatePipelineRequest};

/// Orchestrates use cases over the `Pipeline` aggregate.
///
/// Methods are synchronous: `Pipeline` has no external/async collaborators, so a plain
/// `std::sync::Mutex` guard held across each load-mutate-save is enough to prevent lost
/// updates. Holds the project repository to verify the parent project exists.
pub struct PipelineApplicationService<L: PipelineRepository, P: ProjectRepository> {
    pipeline_repository: L,
    project_repository: P,
    numbering: Arc<dyn PipelineNumbering>,
    mutation_guard: Mutex<()>,
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
            mutation_guard: Mutex::new(()),
        }
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub fn list_pipelines(
        &self,
        project_id: &str,
    ) -> anyhow::Result<Option<Vec<PipelineSnapshot>>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).is_none() {
            return Ok(None);
        }
        let mut pipelines = self
            .pipeline_repository
            .list()
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

    pub fn pipeline_snapshot(&self, pipeline_id: &str) -> anyhow::Result<Option<PipelineSnapshot>> {
        let id: PipelineId = pipeline_id.parse()?;
        Ok(self
            .pipeline_repository
            .get(&id)
            .map(|pipeline| pipeline.snapshot()))
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub fn create_pipeline(
        &self,
        project_id: &str,
        request: CreatePipelineRequest,
    ) -> anyhow::Result<Option<PipelineSnapshot>> {
        let _guard = self.lock();
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).is_none() {
            return Ok(None);
        }
        let pipeline = Pipeline::new(self.numbering.next(), id, request.name, request.description)?;
        let snapshot = pipeline.snapshot();
        self.pipeline_repository.save(pipeline);
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no pipeline with the given id exists.
    pub fn update_pipeline(
        &self,
        pipeline_id: &str,
        request: UpdatePipelineRequest,
    ) -> anyhow::Result<Option<PipelineSnapshot>> {
        let _guard = self.lock();
        let id: PipelineId = pipeline_id.parse()?;
        let Some(mut pipeline) = self.pipeline_repository.get(&id) else {
            return Ok(None);
        };
        pipeline.update(request.name, request.description)?;
        let snapshot = pipeline.snapshot();
        self.pipeline_repository.save(pipeline);
        Ok(Some(snapshot))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_guard
            .lock()
            .expect("pipeline mutation guard poisoned")
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
    struct TestPipelineRepository {
        pipelines: RwLock<HashMap<PipelineId, Pipeline>>,
    }

    impl PipelineRepository for TestPipelineRepository {
        fn save(&self, pipeline: Pipeline) {
            self.pipelines
                .write()
                .expect("test repository write lock poisoned")
                .insert(pipeline.id(), pipeline);
        }

        fn get(&self, id: &PipelineId) -> Option<Pipeline> {
            self.pipelines
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .cloned()
        }

        fn list(&self) -> Vec<Pipeline> {
            self.pipelines
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

    fn seed_project(
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
        service.project_repository.save(project);
        id
    }

    fn create(
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
            .expect("project id should be valid")
            .expect("project should exist")
    }

    #[test]
    fn create_pipeline_assigns_incrementing_ids() {
        let service = service();
        let project_id = seed_project(&service, 1);
        assert_eq!(create(&service, &project_id, "First").id, "PL-1");
        assert_eq!(create(&service, &project_id, "Second").id, "PL-2");
    }

    #[test]
    fn create_pipeline_records_project_id() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let pipeline = create(&service, &project_id, "Deploy");
        assert_eq!(pipeline.project_id, project_id);
    }

    #[test]
    fn create_pipeline_under_missing_project_returns_none() {
        let service = service();
        let result = service
            .create_pipeline(
                "P-9",
                CreatePipelineRequest {
                    name: "Deploy".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn create_pipeline_rejects_malformed_project_id() {
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
                .is_err()
        );
    }

    #[test]
    fn create_pipeline_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1);
        assert!(
            service
                .create_pipeline(
                    &project_id,
                    CreatePipelineRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn list_pipelines_partitions_by_project() {
        let service = service();
        let first_project = seed_project(&service, 1);
        let second_project = seed_project(&service, 2);
        create(&service, &first_project, "First");
        create(&service, &second_project, "Second");
        create(&service, &first_project, "Third");
        service.pipeline_repository.save(
            Pipeline::new(
                PipelineId::from_number(10),
                ProjectId::from_number(1),
                "Tenth".to_owned(),
                String::new(),
            )
            .unwrap(),
        );

        let first_ids = service
            .list_pipelines(&first_project)
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|pipeline| pipeline.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["PL-1", "PL-3", "PL-10"]);

        let second_ids = service
            .list_pipelines(&second_project)
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|pipeline| pipeline.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["PL-2"]);
    }

    #[test]
    fn list_pipelines_for_missing_project_returns_none() {
        let service = service();
        assert!(service.list_pipelines("P-9").unwrap().is_none());
    }

    #[test]
    fn list_pipelines_rejects_malformed_project_id() {
        let service = service();
        assert!(service.list_pipelines("bogus").is_err());
    }

    #[test]
    fn pipeline_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.pipeline_snapshot("PL-9").unwrap().is_none());
    }

    #[test]
    fn pipeline_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.pipeline_snapshot("bogus").is_err());
    }

    #[test]
    fn update_pipeline_replaces_fields_but_not_project() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let pipeline = create(&service, &project_id, "Deploy");
        let updated = service
            .update_pipeline(
                &pipeline.id,
                UpdatePipelineRequest {
                    name: "Release".to_owned(),
                    description: "to prod".to_owned(),
                },
            )
            .unwrap()
            .expect("pipeline should exist");
        assert_eq!(updated.name, "Release");
        assert_eq!(updated.description, "to prod");
        assert_eq!(updated.project_id, project_id);

        let reloaded = service
            .pipeline_snapshot(&pipeline.id)
            .unwrap()
            .expect("pipeline should exist");
        assert_eq!(reloaded.name, "Release");
    }

    #[test]
    fn update_missing_pipeline_returns_none() {
        let service = service();
        let result = service
            .update_pipeline(
                "PL-9",
                UpdatePipelineRequest {
                    name: "Release".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_pipeline_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let pipeline = create(&service, &project_id, "Deploy");
        assert!(
            service
                .update_pipeline(
                    &pipeline.id,
                    UpdatePipelineRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }
}
