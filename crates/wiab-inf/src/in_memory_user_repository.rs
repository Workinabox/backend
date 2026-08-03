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

    async fn find_id_by_token_hash(&self, hash: &str) -> Result<Option<UserId>, RepoError> {
        Ok(self.lowest_id_matching(|user| user.token_by_hash(hash).is_some()))
    }

    async fn find_id_by_ssh_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<Option<UserId>, RepoError> {
        Ok(self.lowest_id_matching(|user| user.ssh_key_by_fingerprint(fingerprint).is_some()))
    }
}

impl InMemoryUserRepository {
    /// Lowest matching id rather than the first found: `HashMap` iteration order is arbitrary,
    /// and two users can hold the same SSH key today, so "first" would resolve differently
    /// between runs. The Postgres implementation orders the same way.
    fn lowest_id_matching(&self, predicate: impl Fn(&User) -> bool) -> Option<UserId> {
        self.users
            .read()
            .expect("user repository read lock poisoned")
            .values()
            .filter(|(user, _)| predicate(user))
            .map(|(user, _)| user.id())
            .min_by_key(|id| id.number())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiab_core::user::{SshKey, SshKeyId, UserKind};

    fn user_with_key(number: u64, fingerprint: &str) -> User {
        let mut user = User::new(
            UserId::from_number(number),
            UserKind::Human,
            format!("user-{number}"),
            Some(format!("user-{number}@example.test")),
        )
        .expect("valid user");
        user.add_ssh_key(
            SshKey::new(
                SshKeyId::new(),
                "laptop".to_owned(),
                "ssh-ed25519 AAAA...".to_owned(),
                fingerprint.to_owned(),
            )
            .expect("valid key"),
        );
        user
    }

    async fn store(users: Vec<User>) -> InMemoryUserRepository {
        let repository = InMemoryUserRepository::new();
        for user in users {
            repository
                .save(user, Version::NEW)
                .await
                .expect("saves a fresh user");
        }
        repository
    }

    #[tokio::test]
    async fn resolves_a_key_to_its_owner() {
        let repository = store(vec![
            user_with_key(1, "SHA256:aaa"),
            user_with_key(2, "SHA256:bbb"),
        ])
        .await;
        assert_eq!(
            repository
                .find_id_by_ssh_fingerprint("SHA256:bbb")
                .await
                .unwrap(),
            Some(UserId::from_number(2))
        );
    }

    #[tokio::test]
    async fn an_unknown_credential_resolves_to_nobody() {
        let repository = store(vec![user_with_key(1, "SHA256:aaa")]).await;
        assert_eq!(
            repository
                .find_id_by_ssh_fingerprint("SHA256:nope")
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            repository.find_id_by_token_hash("nope").await.unwrap(),
            None
        );
    }

    /// Nothing stops two users registering the same public key, so the tie has to break the
    /// same way every time — `HashMap` order would otherwise vary between runs.
    #[tokio::test]
    async fn a_shared_key_resolves_deterministically() {
        let repository = store(vec![
            user_with_key(7, "SHA256:shared"),
            user_with_key(3, "SHA256:shared"),
            user_with_key(9, "SHA256:shared"),
        ])
        .await;
        for _ in 0..8 {
            assert_eq!(
                repository
                    .find_id_by_ssh_fingerprint("SHA256:shared")
                    .await
                    .unwrap(),
                Some(UserId::from_number(3))
            );
        }
    }
}
