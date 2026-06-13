use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use wiab_core::user::{User, UserId, UserRepository};

#[derive(Clone, Default)]
pub struct InMemoryUserRepository {
    users: Arc<RwLock<HashMap<UserId, User>>>,
}

impl InMemoryUserRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl UserRepository for InMemoryUserRepository {
    fn save(&self, user: User) {
        self.users
            .write()
            .expect("user repository write lock poisoned")
            .insert(user.id(), user);
    }

    fn get(&self, id: &UserId) -> Option<User> {
        self.users
            .read()
            .expect("user repository read lock poisoned")
            .get(id)
            .cloned()
    }

    fn list(&self) -> Vec<User> {
        self.users
            .read()
            .expect("user repository read lock poisoned")
            .values()
            .cloned()
            .collect()
    }
}
