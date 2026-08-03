use crate::repository::{RepoError, SaveError, Version};
use crate::user::{User, UserId};

/// Port for persisting user aggregates. One repository per aggregate root.
#[allow(async_fn_in_trait)]
pub trait UserRepository: Send + Sync + 'static {
    async fn save(&self, user: User, expected: Version) -> Result<Version, SaveError>;
    async fn get(&self, id: &UserId) -> Result<Option<(User, Version)>, RepoError>;
    async fn list(&self) -> Result<Vec<User>, RepoError>;

    /// Id of the user holding an access token with this hash.
    ///
    /// On the authentication path, so it must not depend on the size of the user table. No
    /// default body: a `list()`-based default would let an implementation silently keep the
    /// scan, which is the thing being removed.
    async fn find_id_by_token_hash(&self, hash: &str) -> Result<Option<UserId>, RepoError>;

    /// Id of the user owning the SSH key with this fingerprint.
    async fn find_id_by_ssh_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<Option<UserId>, RepoError>;
}
