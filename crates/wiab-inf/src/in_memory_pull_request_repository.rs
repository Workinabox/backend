use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::pull_request::{PullRequest, PullRequestId, PullRequestRepository};
use wiab_core::repository::{RepoError, SaveError, Version};

#[derive(Debug, Clone, Default)]
pub struct InMemoryPullRequestRepository {
    pull_requests: Arc<RwLock<HashMap<PullRequestId, (PullRequest, u64)>>>,
}

impl InMemoryPullRequestRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PullRequestRepository for InMemoryPullRequestRepository {
    async fn save(
        &self,
        pull_request: PullRequest,
        expected: Version,
    ) -> Result<Version, SaveError> {
        let mut pull_requests = self
            .pull_requests
            .write()
            .expect("pull request repository write lock poisoned");
        let current = pull_requests
            .get(&pull_request.id())
            .map(|(_, version)| *version)
            .unwrap_or(0);
        if current != expected.value() {
            return Err(SaveError::Conflict);
        }
        let next = expected.next();
        pull_requests.insert(pull_request.id(), (pull_request, next.value()));
        Ok(next)
    }

    async fn get(&self, id: &PullRequestId) -> Result<Option<(PullRequest, Version)>, RepoError> {
        Ok(self
            .pull_requests
            .read()
            .expect("pull request repository read lock poisoned")
            .get(id)
            .map(|(pull_request, version)| (pull_request.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<PullRequest>, RepoError> {
        Ok(self
            .pull_requests
            .read()
            .expect("pull request repository read lock poisoned")
            .values()
            .map(|(pull_request, _)| pull_request.clone())
            .collect())
    }
}
