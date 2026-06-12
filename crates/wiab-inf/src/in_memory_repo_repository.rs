use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::repo::{Repo, RepoId, RepoRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryRepoRepository {
    repos: Arc<RwLock<HashMap<RepoId, Repo>>>,
}

impl InMemoryRepoRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RepoRepository for InMemoryRepoRepository {
    fn save(&self, repo: Repo) {
        self.repos
            .write()
            .expect("repo repository write lock poisoned")
            .insert(repo.id(), repo);
    }

    fn get(&self, id: &RepoId) -> Option<Repo> {
        self.repos
            .read()
            .expect("repo repository read lock poisoned")
            .get(id)
            .cloned()
    }

    fn list(&self) -> Vec<Repo> {
        self.repos
            .read()
            .expect("repo repository read lock poisoned")
            .values()
            .cloned()
            .collect()
    }
}
