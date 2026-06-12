use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use wiab_core::project::{ProjectId, ProjectRepository};
use wiab_core::work::{DoneId, Work, WorkId, WorkNumbering, WorkRepository, WorkSnapshot};

use crate::create_work_request::{AddDoneRequest, CreateWorkRequest, UpdateWorkRequest};

/// Orchestrates use cases over the `Work` aggregate.
///
/// Methods are synchronous: `Work` has no external/async collaborators, so a plain
/// `std::sync::Mutex` guard held across each load-mutate-save is enough to prevent lost
/// updates. Only root works live in the repository, so mutations on a nested sub-work load
/// the containing root tree, locate the node, mutate, and save the whole tree. Holds the
/// project repository to verify the parent project exists.
pub struct WorkApplicationService<R: WorkRepository, P: ProjectRepository> {
    work_repository: R,
    project_repository: P,
    numbering: Arc<dyn WorkNumbering>,
    mutation_guard: Mutex<()>,
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
            mutation_guard: Mutex::new(()),
        }
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub fn list_works_by_project(
        &self,
        project_id: &str,
    ) -> anyhow::Result<Option<Vec<WorkSnapshot>>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).is_none() {
            return Ok(None);
        }
        let mut works = self
            .work_repository
            .list()
            .into_iter()
            .filter(|work| work.project_id() == id)
            .collect::<Vec<_>>();
        works.sort_by_key(|work| work.id().number());
        Ok(Some(
            works.into_iter().map(|work| work.snapshot()).collect(),
        ))
    }

    pub fn work_snapshot(&self, work_id: &str) -> anyhow::Result<Option<WorkSnapshot>> {
        let id: WorkId = work_id.parse()?;
        Ok(self.work_repository.get(&id).map(|work| work.snapshot()))
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub fn create_work(
        &self,
        project_id: &str,
        request: CreateWorkRequest,
    ) -> anyhow::Result<Option<WorkSnapshot>> {
        let _guard = self.lock();
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).is_none() {
            return Ok(None);
        }
        let work = Work::new(
            self.numbering.next(),
            id,
            request.title,
            request.description,
        )?;
        let snapshot = work.snapshot();
        self.work_repository.save(work);
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no work tree contains the given id. Updating a nested
    /// sub-work saves its containing root tree and returns the root's snapshot.
    pub fn update_work(
        &self,
        work_id: &str,
        request: UpdateWorkRequest,
    ) -> anyhow::Result<Option<WorkSnapshot>> {
        let _guard = self.lock();
        let id: WorkId = work_id.parse()?;
        let Some(mut root) = self
            .work_repository
            .list()
            .into_iter()
            .find(|root| root.find(&id).is_some())
        else {
            return Ok(None);
        };
        root.find_mut(&id)
            .expect("located work present")
            .update(request.title, request.description)?;
        Ok(Some(self.save(root)))
    }

    pub fn add_child(
        &self,
        parent_work_id: &str,
        request: CreateWorkRequest,
    ) -> anyhow::Result<WorkSnapshot> {
        let _guard = self.lock();
        let (mut root, parent_id) = self.locate(parent_work_id)?;
        let child = Work::new(
            self.numbering.next(),
            root.project_id(),
            request.title,
            request.description,
        )?;
        root.find_mut(&parent_id)
            .expect("located work present")
            .add_child(child)?;
        Ok(self.save(root))
    }

    pub fn add_done(&self, work_id: &str, request: AddDoneRequest) -> anyhow::Result<WorkSnapshot> {
        let _guard = self.lock();
        let (mut root, id) = self.locate(work_id)?;
        root.find_mut(&id)
            .expect("located work present")
            .add_done(request.criterion)?;
        Ok(self.save(root))
    }

    pub fn fulfill_done(&self, work_id: &str, done_id: &str) -> anyhow::Result<WorkSnapshot> {
        let _guard = self.lock();
        let (mut root, id) = self.locate(work_id)?;
        let done_id: DoneId = done_id.parse()?;
        root.find_mut(&id)
            .expect("located work present")
            .fulfill_done(&done_id)?;
        Ok(self.save(root))
    }

    pub fn unfulfill_done(&self, work_id: &str, done_id: &str) -> anyhow::Result<WorkSnapshot> {
        let _guard = self.lock();
        let (mut root, id) = self.locate(work_id)?;
        let done_id: DoneId = done_id.parse()?;
        root.find_mut(&id)
            .expect("located work present")
            .unfulfill_done(&done_id)?;
        Ok(self.save(root))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_guard
            .lock()
            .expect("work mutation guard poisoned")
    }

    /// Parse a work id and find the owned root tree that contains it.
    fn locate(&self, work_id: &str) -> anyhow::Result<(Work, WorkId)> {
        let id: WorkId = work_id.parse()?;
        let root = self
            .work_repository
            .list()
            .into_iter()
            .find(|root| root.find(&id).is_some())
            .ok_or_else(|| anyhow!("work '{work_id}' not found"))?;
        Ok((root, id))
    }

    fn save(&self, root: Work) -> WorkSnapshot {
        let snapshot = root.snapshot();
        self.work_repository.save(root);
        snapshot
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
    struct TestWorkRepository {
        works: RwLock<HashMap<WorkId, Work>>,
    }

    impl WorkRepository for TestWorkRepository {
        fn save(&self, work: Work) {
            self.works
                .write()
                .expect("test repository write lock poisoned")
                .insert(work.id(), work);
        }

        fn get(&self, id: &WorkId) -> Option<Work> {
            self.works
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .cloned()
        }

        fn list(&self) -> Vec<Work> {
            self.works
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

    fn seed_project(
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
        service.project_repository.save(project);
        id
    }

    fn create(
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
            .expect("project id should be valid")
            .expect("project should exist")
    }

    #[test]
    fn create_work_assigns_incrementing_ids() {
        let service = service();
        let project_id = seed_project(&service, 1);
        assert_eq!(create(&service, &project_id, "First").id, "W-1");
        assert_eq!(create(&service, &project_id, "Second").id, "W-2");
    }

    #[test]
    fn create_work_records_project_id() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let work = create(&service, &project_id, "Ship v1");
        assert_eq!(work.project_id, project_id);
    }

    #[test]
    fn create_work_in_missing_project_returns_none() {
        let service = service();
        let result = service
            .create_work(
                "P-9",
                CreateWorkRequest {
                    title: "Ship v1".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn create_work_rejects_malformed_project_id() {
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
                .is_err()
        );
    }

    #[test]
    fn list_works_partitions_by_project() {
        let service = service();
        let first_project = seed_project(&service, 1);
        let second_project = seed_project(&service, 2);
        create(&service, &first_project, "First");
        create(&service, &second_project, "Second");
        create(&service, &first_project, "Third");
        service.work_repository.save(
            Work::new(
                WorkId::from_number(10),
                ProjectId::from_number(1),
                "Tenth".to_owned(),
                String::new(),
            )
            .unwrap(),
        );

        let first_ids = service
            .list_works_by_project(&first_project)
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|work| work.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["W-1", "W-3", "W-10"]);

        let second_ids = service
            .list_works_by_project(&second_project)
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|work| work.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["W-2"]);
    }

    #[test]
    fn list_works_for_missing_project_returns_none() {
        let service = service();
        assert!(service.list_works_by_project("P-9").unwrap().is_none());
    }

    #[test]
    fn list_works_rejects_malformed_project_id() {
        let service = service();
        assert!(service.list_works_by_project("bogus").is_err());
    }

    #[test]
    fn update_work_replaces_fields() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let work = create(&service, &project_id, "Ship v1");
        let updated = service
            .update_work(
                &work.id,
                UpdateWorkRequest {
                    title: "Ship v2".to_owned(),
                    description: "the sequel".to_owned(),
                },
            )
            .unwrap()
            .expect("work should exist");
        assert_eq!(updated.title, "Ship v2");
        assert_eq!(updated.description, "the sequel");

        let reloaded = service
            .work_snapshot(&work.id)
            .unwrap()
            .expect("work should exist");
        assert_eq!(reloaded.title, "Ship v2");
    }

    #[test]
    fn update_nested_child_returns_root_snapshot() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let parent = create(&service, &project_id, "Epic");
        let with_child = service
            .add_child(
                &parent.id,
                CreateWorkRequest {
                    title: "Subtask".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        let child_id = with_child.children[0].id.clone();

        let updated = service
            .update_work(
                &child_id,
                UpdateWorkRequest {
                    title: "Renamed subtask".to_owned(),
                    description: "details".to_owned(),
                },
            )
            .unwrap()
            .expect("work should exist");
        assert_eq!(updated.id, parent.id);
        assert_eq!(updated.children[0].title, "Renamed subtask");
        assert_eq!(updated.children[0].description, "details");
    }

    #[test]
    fn update_missing_work_returns_none() {
        let service = service();
        let result = service
            .update_work(
                "W-99",
                UpdateWorkRequest {
                    title: "Ghost".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_work_rejects_empty_title() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let work = create(&service, &project_id, "Ship v1");
        assert!(
            service
                .update_work(
                    &work.id,
                    UpdateWorkRequest {
                        title: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn add_child_inherits_root_project_id() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let parent = create(&service, &project_id, "Epic");
        let with_child = service
            .add_child(
                &parent.id,
                CreateWorkRequest {
                    title: "Subtask".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert_eq!(with_child.children[0].project_id, project_id);
    }

    #[test]
    fn add_done_then_fulfill_flips_is_done() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let work = create(&service, &project_id, "Ship v1");
        let snapshot = service
            .add_done(
                &work.id,
                AddDoneRequest {
                    criterion: "tests pass".to_owned(),
                },
            )
            .unwrap();
        assert!(!snapshot.is_done);
        let done_id = snapshot.dones[0].id.clone();

        let snapshot = service.fulfill_done(&work.id, &done_id).unwrap();
        assert!(snapshot.is_done);
    }

    #[test]
    fn child_rolls_up_into_parent_completion() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let parent = create(&service, &project_id, "Epic");
        let child = service
            .add_child(
                &parent.id,
                CreateWorkRequest {
                    title: "Subtask".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        let child_id = child.children[0].id.clone();

        let with_done = service
            .add_done(
                &child_id,
                AddDoneRequest {
                    criterion: "implemented".to_owned(),
                },
            )
            .unwrap();
        assert!(!with_done.is_done); // root snapshot: child not done
        let done_id = with_done.children[0].dones[0].id.clone();

        let done = service.fulfill_done(&child_id, &done_id).unwrap();
        assert!(done.is_done);
    }

    #[test]
    fn unknown_work_id_errors() {
        let service = service();
        assert!(
            service
                .add_done(
                    "W-99",
                    AddDoneRequest {
                        criterion: "x".to_owned()
                    }
                )
                .is_err()
        );
    }

    #[test]
    fn unknown_done_id_errors() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let work = create(&service, &project_id, "Ship v1");
        let missing = uuid::Uuid::new_v4().to_string();
        assert!(service.fulfill_done(&work.id, &missing).is_err());
    }
}
