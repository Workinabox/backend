use crate::repository::{RepoError, SaveError, Version};
use crate::user::{User, UserId};

/// Port for persisting user aggregates. One repository per aggregate root.
#[allow(async_fn_in_trait)]
pub trait UserRepository: Send + Sync + 'static {
    async fn save(&self, user: User, expected: Version) -> Result<Version, SaveError>;
    async fn get(&self, id: &UserId) -> Result<Option<(User, Version)>, RepoError>;
    async fn list(&self) -> Result<Vec<User>, RepoError>;
}
