use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::repo::{Repo, RepoId, RepoRepository};
use wiab_core::repository::{RepoError, SaveError, Version};

#[derive(Debug, Clone, Default)]
pub struct InMemoryRepoRepository {
    repos: Arc<RwLock<HashMap<RepoId, (Repo, u64)>>>,
}

impl InMemoryRepoRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RepoRepository for InMemoryRepoRepository {
    async fn save(&self, repo: Repo, expected: Version) -> Result<Version, SaveError> {
        let mut repos = self
            .repos
            .write()
            .expect("repo repository write lock poisoned");
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
            .expect("repo repository read lock poisoned")
            .get(id)
            .map(|(repo, version)| (repo.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Repo>, RepoError> {
        Ok(self
            .repos
            .read()
            .expect("repo repository read lock poisoned")
            .values()
            .map(|(repo, _)| repo.clone())
            .collect())
    }
}
