use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::user::{User, UserId, UserRepository};

#[derive(Clone, Default)]
pub struct InMemoryUserRepository {
    users: Arc<RwLock<HashMap<UserId, (User, u64)>>>,
}

impl InMemoryUserRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl UserRepository for InMemoryUserRepository {
    async fn save(&self, user: User, expected: Version) -> Result<Version, SaveError> {
        let mut users = self
            .users
            .write()
            .expect("user repository write lock poisoned");
        let current = users
            .get(&user.id())
            .map(|(_, version)| *version)
            .unwrap_or(0);
        if current != expected.value() {
            return Err(SaveError::Conflict);
        }
        let next = expected.next();
        users.insert(user.id(), (user, next.value()));
        Ok(next)
    }

    async fn get(&self, id: &UserId) -> Result<Option<(User, Version)>, RepoError> {
        Ok(self
            .users
            .read()
            .expect("user repository read lock poisoned")
            .get(id)
            .map(|(user, version)| (user.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<User>, RepoError> {
        Ok(self
            .users
            .read()
            .expect("user repository read lock poisoned")
            .values()
            .map(|(user, _)| user.clone())
            .collect())
    }
}
