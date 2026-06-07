use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use wiab_core::work::{DoneId, Work, WorkId, WorkNumbering, WorkRepository, WorkSnapshot};

use crate::create_work_request::{AddDoneRequest, CreateWorkRequest};

/// Orchestrates use cases over the `Work` aggregate.
///
/// Methods are synchronous: `Work` has no external/async collaborators, so a plain
/// `std::sync::Mutex` guard held across each load-mutate-save is enough to prevent lost
/// updates. Only root works live in the repository, so mutations on a nested sub-work load
/// the containing root tree, locate the node, mutate, and save the whole tree.
pub struct WorkApplicationService<R: WorkRepository> {
    work_repository: R,
    numbering: Arc<dyn WorkNumbering>,
    mutation_guard: Mutex<()>,
}

impl<R: WorkRepository> WorkApplicationService<R> {
    pub fn new(work_repository: R, numbering: Arc<dyn WorkNumbering>) -> Self {
        Self {
            work_repository,
            numbering,
            mutation_guard: Mutex::new(()),
        }
    }

    pub fn list_works(&self) -> Vec<WorkSnapshot> {
        let mut works = self
            .work_repository
            .list()
            .into_iter()
            .map(|work| work.snapshot())
            .collect::<Vec<_>>();
        works.sort_by(|left, right| left.id.cmp(&right.id));
        works
    }

    pub fn work_snapshot(&self, work_id: &str) -> anyhow::Result<Option<WorkSnapshot>> {
        let id: WorkId = work_id.parse()?;
        Ok(self.work_repository.get(&id).map(|work| work.snapshot()))
    }

    pub fn create_work(&self, request: CreateWorkRequest) -> anyhow::Result<WorkSnapshot> {
        let _guard = self.lock();
        let work = Work::new(self.numbering.next(), request.title, request.description)?;
        let snapshot = work.snapshot();
        self.work_repository.save(work);
        Ok(snapshot)
    }

    pub fn add_child(
        &self,
        parent_work_id: &str,
        request: CreateWorkRequest,
    ) -> anyhow::Result<WorkSnapshot> {
        let _guard = self.lock();
        let (mut root, parent_id) = self.locate(parent_work_id)?;
        let child = Work::new(self.numbering.next(), request.title, request.description)?;
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
    struct TestWorkNumbering {
        counter: AtomicU64,
    }

    impl WorkNumbering for TestWorkNumbering {
        fn next(&self) -> WorkId {
            WorkId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    fn service() -> WorkApplicationService<TestWorkRepository> {
        WorkApplicationService::new(
            TestWorkRepository::default(),
            Arc::new(TestWorkNumbering::default()),
        )
    }

    fn create(service: &WorkApplicationService<TestWorkRepository>, title: &str) -> WorkSnapshot {
        service
            .create_work(CreateWorkRequest {
                title: title.to_owned(),
                description: String::new(),
            })
            .expect("work should be created")
    }

    #[test]
    fn create_work_assigns_incrementing_ids() {
        let service = service();
        assert_eq!(create(&service, "First").id, "W-1");
        assert_eq!(create(&service, "Second").id, "W-2");
    }

    #[test]
    fn add_done_then_fulfill_flips_is_done() {
        let service = service();
        let work = create(&service, "Ship v1");
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
        let parent = create(&service, "Epic");
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
        let work = create(&service, "Ship v1");
        let missing = uuid::Uuid::new_v4().to_string();
        assert!(service.fulfill_done(&work.id, &missing).is_err());
    }
}
