use crate::user::{User, UserId};

/// Port for persisting user aggregates. One repository per aggregate root.
pub trait UserRepository: Send + Sync + 'static {
    fn save(&self, user: User);
    fn get(&self, id: &UserId) -> Option<User>;
    fn list(&self) -> Vec<User>;
}
