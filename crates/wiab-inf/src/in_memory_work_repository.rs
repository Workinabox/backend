use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::work::{Work, WorkId, WorkRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryWorkRepository {
    works: Arc<RwLock<HashMap<WorkId, (Work, u64)>>>,
}

impl InMemoryWorkRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl WorkRepository for InMemoryWorkRepository {
    async fn save(&self, work: Work, expected: Version) -> Result<Version, SaveError> {
        let mut works = self
            .works
            .write()
            .expect("work repository write lock poisoned");
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
            .expect("work repository read lock poisoned")
            .get(id)
            .map(|(work, version)| (work.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Work>, RepoError> {
        Ok(self
            .works
            .read()
            .expect("work repository read lock poisoned")
            .values()
            .map(|(work, _)| work.clone())
            .collect())
    }
}
