use std::sync::{Arc, Mutex};

use wiab_core::agent::AgentId;
use wiab_core::meeting_traits::Clock;
use wiab_core::organization::OrganizationId;
use wiab_core::repo::RepoId;
use wiab_core::user::{
    AccessToken, KeyFingerprinter, SshKey, SshKeyId, TokenFactory, TokenHasher, TokenId,
    TokenScope, User, UserError, UserId, UserKind, UserNumbering, UserRepository, UserSnapshot,
};

use crate::user_requests::{
    AddSshKeyRequest, CreateUserRequest, IssueTokenRequest, IssuedTokenSnapshot,
};

/// Orchestrates use cases over the `User` aggregate and its credentials. Credential crypto
/// (random generation, hashing, fingerprinting) and timestamps come from injected seams.
pub struct UserApplicationService<U: UserRepository> {
    user_repository: U,
    numbering: Arc<dyn UserNumbering>,
    token_factory: Arc<dyn TokenFactory>,
    token_hasher: Arc<dyn TokenHasher>,
    fingerprinter: Arc<dyn KeyFingerprinter>,
    clock: Arc<dyn Clock>,
    mutation_guard: Mutex<()>,
}

impl<U: UserRepository> UserApplicationService<U> {
    pub fn new(
        user_repository: U,
        numbering: Arc<dyn UserNumbering>,
        token_factory: Arc<dyn TokenFactory>,
        token_hasher: Arc<dyn TokenHasher>,
        fingerprinter: Arc<dyn KeyFingerprinter>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            user_repository,
            numbering,
            token_factory,
            token_hasher,
            fingerprinter,
            clock,
            mutation_guard: Mutex::new(()),
        }
    }

    pub fn list_users(&self) -> Vec<UserSnapshot> {
        let mut users = self.user_repository.list();
        users.sort_by_key(|user| user.id().number());
        users.iter().map(|user| user.snapshot()).collect()
    }

    pub fn user_snapshot(&self, user_id: &str) -> anyhow::Result<Option<UserSnapshot>> {
        let id: UserId = user_id.parse()?;
        Ok(self.user_repository.get(&id).map(|user| user.snapshot()))
    }

    pub fn create_user(&self, request: CreateUserRequest) -> anyhow::Result<UserSnapshot> {
        let _guard = self.lock();
        let kind: UserKind = request.kind.parse()?;
        let user = User::new(
            self.numbering.next(),
            kind,
            request.name,
            request.email,
            None,
        )?;
        let snapshot = user.snapshot();
        self.user_repository.save(user);
        Ok(snapshot)
    }

    /// Creates the `User` identity for an agent. Used when an agent is created.
    pub fn provision_agent_user(
        &self,
        name: String,
        agent_id: AgentId,
    ) -> anyhow::Result<UserSnapshot> {
        let _guard = self.lock();
        let user = User::new(
            self.numbering.next(),
            UserKind::Agent,
            name,
            None,
            Some(agent_id),
        )?;
        let snapshot = user.snapshot();
        self.user_repository.save(user);
        Ok(snapshot)
    }

    pub fn add_ssh_key(
        &self,
        user_id: &str,
        request: AddSshKeyRequest,
    ) -> anyhow::Result<Option<UserSnapshot>> {
        let _guard = self.lock();
        let id: UserId = user_id.parse()?;
        let Some(mut user) = self.user_repository.get(&id) else {
            return Ok(None);
        };
        let fingerprint = self
            .fingerprinter
            .fingerprint(&request.public_key)
            .ok_or_else(|| UserError::InvalidSshKey(request.label.clone()))?;
        let key = SshKey::new(
            SshKeyId::new(),
            request.label,
            request.public_key,
            fingerprint,
        )?;
        user.add_ssh_key(key);
        let snapshot = user.snapshot();
        self.user_repository.save(user);
        Ok(Some(snapshot))
    }

    pub fn remove_ssh_key(
        &self,
        user_id: &str,
        key_id: &str,
    ) -> anyhow::Result<Option<UserSnapshot>> {
        let _guard = self.lock();
        let id: UserId = user_id.parse()?;
        let key_id: SshKeyId = key_id.parse()?;
        let Some(mut user) = self.user_repository.get(&id) else {
            return Ok(None);
        };
        user.remove_ssh_key(&key_id)?;
        let snapshot = user.snapshot();
        self.user_repository.save(user);
        Ok(Some(snapshot))
    }

    pub fn issue_token(
        &self,
        user_id: &str,
        request: IssueTokenRequest,
    ) -> anyhow::Result<Option<IssuedTokenSnapshot>> {
        let _guard = self.lock();
        let id: UserId = user_id.parse()?;
        let Some(mut user) = self.user_repository.get(&id) else {
            return Ok(None);
        };
        let scope = parse_scope(&request)?;
        let generated = self.token_factory.generate();
        let hash = self.token_hasher.hash(&generated.plaintext);
        let token = AccessToken::new(
            TokenId::new(),
            request.label,
            hash,
            generated.display,
            self.clock.now_rfc3339(),
            request.expires_at,
            scope,
        )?;
        let snapshot = token.snapshot();
        user.add_token(token);
        self.user_repository.save(user);
        Ok(Some(IssuedTokenSnapshot {
            token: snapshot,
            plaintext: generated.plaintext,
        }))
    }

    pub fn revoke_token(
        &self,
        user_id: &str,
        token_id: &str,
    ) -> anyhow::Result<Option<UserSnapshot>> {
        let _guard = self.lock();
        let id: UserId = user_id.parse()?;
        let token_id: TokenId = token_id.parse()?;
        let Some(mut user) = self.user_repository.get(&id) else {
            return Ok(None);
        };
        user.revoke_token(&token_id)?;
        let snapshot = user.snapshot();
        self.user_repository.save(user);
        Ok(Some(snapshot))
    }

    /// Resolves a presented token plaintext to its owning user and scope, rejecting an
    /// expired token, and records the use. Used by the HTTPS auth path.
    pub fn resolve_token(&self, plaintext: &str) -> Option<(UserId, TokenScope)> {
        let _guard = self.lock();
        let hash = self.token_hasher.hash(plaintext);
        let now = self.clock.now_rfc3339();
        for mut user in self.user_repository.list() {
            let Some((expired, scope)) = user
                .token_by_hash(&hash)
                .map(|token| (token.is_expired(&now), token.scope().clone()))
            else {
                continue;
            };
            if expired {
                return None;
            }
            let user_id = user.id();
            if let Some(token) = user.token_by_hash_mut(&hash) {
                token.mark_used(now.clone());
            }
            self.user_repository.save(user);
            return Some((user_id, scope));
        }
        None
    }

    /// Resolves an SSH key fingerprint to its owning user. Used by the SSH auth path.
    pub fn resolve_user_by_fingerprint(&self, fingerprint: &str) -> Option<UserId> {
        self.user_repository
            .list()
            .into_iter()
            .find(|user| user.ssh_key_by_fingerprint(fingerprint).is_some())
            .map(|user| user.id())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_guard
            .lock()
            .expect("user mutation guard poisoned")
    }
}

fn parse_scope(request: &IssueTokenRequest) -> anyhow::Result<TokenScope> {
    let repos = match &request.repos {
        Some(list) => Some(
            list.iter()
                .map(|id| id.parse::<RepoId>())
                .collect::<Result<Vec<_>, _>>()?,
        ),
        None => None,
    };
    let orgs = match &request.orgs {
        Some(list) => Some(
            list.iter()
                .map(|id| id.parse::<OrganizationId>())
                .collect::<Result<Vec<_>, _>>()?,
        ),
        None => None,
    };
    Ok(TokenScope::new(request.read_only, repos, orgs))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::user::GeneratedToken;

    use super::*;

    #[derive(Default)]
    struct TestUserRepository {
        users: RwLock<HashMap<UserId, User>>,
    }
    impl UserRepository for TestUserRepository {
        fn save(&self, user: User) {
            self.users.write().unwrap().insert(user.id(), user);
        }
        fn get(&self, id: &UserId) -> Option<User> {
            self.users.read().unwrap().get(id).cloned()
        }
        fn list(&self) -> Vec<User> {
            self.users.read().unwrap().values().cloned().collect()
        }
    }

    #[derive(Default)]
    struct TestNumbering {
        counter: AtomicU64,
    }
    impl UserNumbering for TestNumbering {
        fn next(&self) -> UserId {
            UserId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    struct TestFactory;
    impl TokenFactory for TestFactory {
        fn generate(&self) -> GeneratedToken {
            GeneratedToken {
                plaintext: "wiab_pat_PLAINTEXT".to_owned(),
                display: "wiab_pat_…TEXT".to_owned(),
            }
        }
    }

    struct TestHasher;
    impl TokenHasher for TestHasher {
        fn hash(&self, plaintext: &str) -> String {
            format!("hash({plaintext})")
        }
    }

    struct TestFingerprinter;
    impl KeyFingerprinter for TestFingerprinter {
        fn fingerprint(&self, key: &str) -> Option<String> {
            if key.contains("invalid") {
                None
            } else {
                Some(format!("fp({key})"))
            }
        }
    }

    struct FixedClock;
    impl Clock for FixedClock {
        fn now_rfc3339(&self) -> String {
            "2026-06-12T00:00:00Z".to_owned()
        }
    }

    fn service() -> UserApplicationService<TestUserRepository> {
        UserApplicationService::new(
            TestUserRepository::default(),
            Arc::new(TestNumbering::default()),
            Arc::new(TestFactory),
            Arc::new(TestHasher),
            Arc::new(TestFingerprinter),
            Arc::new(FixedClock),
        )
    }

    fn create(service: &UserApplicationService<TestUserRepository>) -> String {
        service
            .create_user(CreateUserRequest {
                kind: "human".to_owned(),
                name: "Ada".to_owned(),
                email: None,
            })
            .unwrap()
            .id
    }

    #[test]
    fn create_user_assigns_incrementing_ids() {
        let service = service();
        assert_eq!(create(&service), "U-1");
        assert_eq!(create(&service), "U-2");
    }

    #[test]
    fn add_key_then_resolve_by_fingerprint() {
        let service = service();
        let user_id = create(&service);
        service
            .add_ssh_key(
                &user_id,
                AddSshKeyRequest {
                    label: "laptop".to_owned(),
                    public_key: "ssh-ed25519 AAAA".to_owned(),
                },
            )
            .unwrap()
            .unwrap();
        let resolved = service.resolve_user_by_fingerprint("fp(ssh-ed25519 AAAA)");
        assert_eq!(resolved.map(|id| id.to_string()), Some(user_id));
    }

    #[test]
    fn add_invalid_key_is_rejected() {
        let service = service();
        let user_id = create(&service);
        assert!(
            service
                .add_ssh_key(
                    &user_id,
                    AddSshKeyRequest {
                        label: "bad".to_owned(),
                        public_key: "invalid".to_owned(),
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn issue_token_returns_plaintext_once_then_resolves() {
        let service = service();
        let user_id = create(&service);
        let issued = service
            .issue_token(
                &user_id,
                IssueTokenRequest {
                    label: "ci".to_owned(),
                    read_only: true,
                    repos: None,
                    orgs: None,
                    expires_at: None,
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(issued.plaintext, "wiab_pat_PLAINTEXT");
        // The snapshot must not leak the plaintext or hash.
        assert!(!issued.token.display.contains("PLAINTEXT"));

        let (resolved, scope) = service.resolve_token("wiab_pat_PLAINTEXT").unwrap();
        assert_eq!(resolved.to_string(), user_id);
        assert!(scope.is_read_only());
        assert!(service.resolve_token("wrong").is_none());
    }

    #[test]
    fn expired_token_does_not_resolve() {
        let service = service();
        let user_id = create(&service);
        service
            .issue_token(
                &user_id,
                IssueTokenRequest {
                    label: "old".to_owned(),
                    read_only: false,
                    repos: None,
                    orgs: None,
                    expires_at: Some("2020-01-01T00:00:00Z".to_owned()),
                },
            )
            .unwrap();
        assert!(service.resolve_token("wiab_pat_PLAINTEXT").is_none());
    }
}
