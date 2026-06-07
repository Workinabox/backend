use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::work::{Work, WorkId, WorkRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryWorkRepository {
    works: Arc<RwLock<HashMap<WorkId, Work>>>,
}

impl InMemoryWorkRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl WorkRepository for InMemoryWorkRepository {
    fn save(&self, work: Work) {
        self.works
            .write()
            .expect("work repository write lock poisoned")
            .insert(work.id(), work);
    }

    fn get(&self, id: &WorkId) -> Option<Work> {
        self.works
            .read()
            .expect("work repository read lock poisoned")
            .get(id)
            .cloned()
    }

    fn list(&self) -> Vec<Work> {
        self.works
            .read()
            .expect("work repository read lock poisoned")
            .values()
            .cloned()
            .collect()
    }
}
