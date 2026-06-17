use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::agent::AgentId;
use wiab_core::meeting_traits::Clock;
use wiab_core::organization::OrganizationId;
use wiab_core::repo::RepoId;
use wiab_core::repository::{SaveError, Version};
use wiab_core::user::{
    AccessToken, ExternalRef, KeyFingerprinter, SshKey, SshKeyId, TokenFactory, TokenHasher,
    TokenId, TokenScope, User, UserError, UserId, UserKind, UserNumbering, UserRepository,
    UserSnapshot,
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
        }
    }

    pub async fn list_users(&self) -> anyhow::Result<Vec<UserSnapshot>> {
        let mut users = self.user_repository.list().await?;
        users.sort_by_key(|user| user.id().number());
        Ok(users.iter().map(|user| user.snapshot()).collect())
    }

    pub async fn user_snapshot(&self, user_id: &str) -> anyhow::Result<Option<UserSnapshot>> {
        let id: UserId = user_id.parse()?;
        Ok(self
            .user_repository
            .get(&id)
            .await?
            .map(|(user, _)| user.snapshot()))
    }

    /// Resolve a login email to its user id, if one exists **and is active**. Used by the
    /// auth layer's directory — pending (invited/unverified) and deactivated users cannot
    /// authenticate, so they resolve to `None`.
    pub async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<UserId>> {
        let users = self.user_repository.list().await?;
        Ok(users
            .into_iter()
            .find(|user| user.is_active() && user.email() == Some(email))
            .map(|user| user.id()))
    }

    pub async fn create_user(&self, request: CreateUserRequest) -> anyhow::Result<UserSnapshot> {
        let kind: UserKind = request.kind.parse()?;
        let user = User::new(self.numbering.next(), kind, request.name, request.email)?;
        let snapshot = user.snapshot();
        self.user_repository.save(user, Version::NEW).await?;
        Ok(snapshot)
    }

    /// Create a human user in the `Pending` state (invited, or signed up but not yet
    /// verified), with no password. Rejects a duplicate email in any state.
    pub async fn create_pending_user(
        &self,
        name: String,
        email: String,
    ) -> anyhow::Result<UserSnapshot> {
        if self
            .user_repository
            .list()
            .await?
            .iter()
            .any(|user| user.email() == Some(email.as_str()))
        {
            return Err(anyhow!("a user with that email already exists"));
        }
        let mut user = User::new(self.numbering.next(), UserKind::Human, name, Some(email))?;
        user.mark_pending();
        let snapshot = user.snapshot();
        self.user_repository.save(user, Version::NEW).await?;
        Ok(snapshot)
    }

    /// Activate a user (e.g. after they accept an invite or verify their email).
    pub async fn activate_user(&self, user_id: &str) -> anyhow::Result<Option<UserSnapshot>> {
        self.transition(user_id, User::activate).await
    }

    /// Deactivate a user — keeps their record and role grants but bars them from logging in.
    pub async fn deactivate_user(&self, user_id: &str) -> anyhow::Result<Option<UserSnapshot>> {
        self.transition(user_id, User::deactivate).await
    }

    async fn transition(
        &self,
        user_id: &str,
        apply: impl Fn(&mut User),
    ) -> anyhow::Result<Option<UserSnapshot>> {
        let id: UserId = user_id.parse()?;
        loop {
            let Some((mut user, version)) = self.user_repository.get(&id).await? else {
                return Ok(None);
            };
            apply(&mut user);
            let snapshot = user.snapshot();
            match self.user_repository.save(user, version).await {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    /// Creates the `User` identity for an agent. Used when an agent is created. The agent
    /// link is recorded as an external reference under the `"agent"` system.
    pub async fn provision_agent_user(
        &self,
        name: String,
        agent_id: AgentId,
    ) -> anyhow::Result<UserSnapshot> {
        let mut user = User::new(self.numbering.next(), UserKind::Agent, name, None)?;
        user.add_external_ref(ExternalRef::new("agent", agent_id.to_string()));
        let snapshot = user.snapshot();
        self.user_repository.save(user, Version::NEW).await?;
        Ok(snapshot)
    }

    pub async fn add_ssh_key(
        &self,
        user_id: &str,
        request: AddSshKeyRequest,
    ) -> anyhow::Result<Option<UserSnapshot>> {
        let id: UserId = user_id.parse()?;
        loop {
            let Some((mut user, version)) = self.user_repository.get(&id).await? else {
                return Ok(None);
            };
            let fingerprint = self
                .fingerprinter
                .fingerprint(&request.public_key)
                .ok_or_else(|| UserError::InvalidSshKey(request.label.clone()))?;
            let key = SshKey::new(
                SshKeyId::new(),
                request.label.clone(),
                request.public_key.clone(),
                fingerprint,
            )?;
            user.add_ssh_key(key);
            let snapshot = user.snapshot();
            match self.user_repository.save(user, version).await {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    pub async fn remove_ssh_key(
        &self,
        user_id: &str,
        key_id: &str,
    ) -> anyhow::Result<Option<UserSnapshot>> {
        let id: UserId = user_id.parse()?;
        let key_id: SshKeyId = key_id.parse()?;
        loop {
            let Some((mut user, version)) = self.user_repository.get(&id).await? else {
                return Ok(None);
            };
            user.remove_ssh_key(&key_id)?;
            let snapshot = user.snapshot();
            match self.user_repository.save(user, version).await {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    pub async fn issue_token(
        &self,
        user_id: &str,
        request: IssueTokenRequest,
    ) -> anyhow::Result<Option<IssuedTokenSnapshot>> {
        let id: UserId = user_id.parse()?;
        loop {
            let Some((mut user, version)) = self.user_repository.get(&id).await? else {
                return Ok(None);
            };
            let scope = parse_scope(&request)?;
            let generated = self.token_factory.generate();
            let hash = self.token_hasher.hash(&generated.plaintext);
            let token = AccessToken::new(
                TokenId::new(),
                request.label.clone(),
                hash,
                generated.display,
                self.clock.now_rfc3339(),
                request.expires_at.clone(),
                scope,
            )?;
            let snapshot = token.snapshot();
            user.add_token(token);
            match self.user_repository.save(user, version).await {
                Ok(_) => {
                    return Ok(Some(IssuedTokenSnapshot {
                        token: snapshot,
                        plaintext: generated.plaintext,
                    }));
                }
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    pub async fn revoke_token(
        &self,
        user_id: &str,
        token_id: &str,
    ) -> anyhow::Result<Option<UserSnapshot>> {
        let id: UserId = user_id.parse()?;
        let token_id: TokenId = token_id.parse()?;
        loop {
            let Some((mut user, version)) = self.user_repository.get(&id).await? else {
                return Ok(None);
            };
            user.revoke_token(&token_id)?;
            let snapshot = user.snapshot();
            match self.user_repository.save(user, version).await {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    /// Resolves a presented token plaintext to its owning user and scope, rejecting an
    /// expired token, and records the use. Used by the HTTPS auth path.
    pub async fn resolve_token(
        &self,
        plaintext: &str,
    ) -> anyhow::Result<Option<(UserId, TokenScope)>> {
        let hash = self.token_hasher.hash(plaintext);
        let id = {
            let users = self.user_repository.list().await?;
            users
                .into_iter()
                .find(|user| user.token_by_hash(&hash).is_some())
                .map(|user| user.id())
        };
        let Some(id) = id else {
            return Ok(None);
        };
        loop {
            let Some((mut user, version)) = self.user_repository.get(&id).await? else {
                return Ok(None);
            };
            let now = self.clock.now_rfc3339();
            let Some((expired, scope)) = user
                .token_by_hash(&hash)
                .map(|token| (token.is_expired(&now), token.scope().clone()))
            else {
                return Ok(None);
            };
            if expired {
                return Ok(None);
            }
            let user_id = user.id();
            if let Some(token) = user.token_by_hash_mut(&hash) {
                token.mark_used(now.clone());
            }
            match self.user_repository.save(user, version).await {
                Ok(_) => return Ok(Some((user_id, scope))),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    /// Resolves an SSH key fingerprint to its owning user. Used by the SSH auth path.
    pub async fn resolve_user_by_fingerprint(
        &self,
        fingerprint: &str,
    ) -> anyhow::Result<Option<UserId>> {
        let users = self.user_repository.list().await?;
        Ok(users
            .into_iter()
            .find(|user| user.ssh_key_by_fingerprint(fingerprint).is_some())
            .map(|user| user.id()))
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

    use wiab_core::repository::{RepoError, SaveError, Version};
    use wiab_core::user::GeneratedToken;

    use super::*;

    #[derive(Default)]
    struct TestUserRepository {
        users: RwLock<HashMap<UserId, (User, u64)>>,
    }
    impl UserRepository for TestUserRepository {
        async fn save(&self, user: User, expected: Version) -> Result<Version, SaveError> {
            let mut users = self.users.write().unwrap();
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
                .unwrap()
                .get(id)
                .map(|(user, version)| (user.clone(), Version::from_value(*version))))
        }
        async fn list(&self) -> Result<Vec<User>, RepoError> {
            Ok(self
                .users
                .read()
                .unwrap()
                .values()
                .map(|(user, _)| user.clone())
                .collect())
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

    async fn create(service: &UserApplicationService<TestUserRepository>) -> String {
        service
            .create_user(CreateUserRequest {
                kind: "human".to_owned(),
                name: "Ada".to_owned(),
                email: None,
            })
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn create_user_assigns_incrementing_ids() {
        let service = service();
        assert_eq!(create(&service).await, "U-1");
        assert_eq!(create(&service).await, "U-2");
    }

    #[tokio::test]
    async fn add_key_then_resolve_by_fingerprint() {
        let service = service();
        let user_id = create(&service).await;
        service
            .add_ssh_key(
                &user_id,
                AddSshKeyRequest {
                    label: "laptop".to_owned(),
                    public_key: "ssh-ed25519 AAAA".to_owned(),
                },
            )
            .await
            .unwrap()
            .unwrap();
        let resolved = service
            .resolve_user_by_fingerprint("fp(ssh-ed25519 AAAA)")
            .await
            .unwrap();
        assert_eq!(resolved.map(|id| id.to_string()), Some(user_id));
    }

    #[tokio::test]
    async fn add_invalid_key_is_rejected() {
        let service = service();
        let user_id = create(&service).await;
        assert!(
            service
                .add_ssh_key(
                    &user_id,
                    AddSshKeyRequest {
                        label: "bad".to_owned(),
                        public_key: "invalid".to_owned(),
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn issue_token_returns_plaintext_once_then_resolves() {
        let service = service();
        let user_id = create(&service).await;
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
            .await
            .unwrap()
            .unwrap();
        assert_eq!(issued.plaintext, "wiab_pat_PLAINTEXT");
        // The snapshot must not leak the plaintext or hash.
        assert!(!issued.token.display.contains("PLAINTEXT"));

        let (resolved, scope) = service
            .resolve_token("wiab_pat_PLAINTEXT")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.to_string(), user_id);
        assert!(scope.is_read_only());
        assert!(service.resolve_token("wrong").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn expired_token_does_not_resolve() {
        let service = service();
        let user_id = create(&service).await;
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
            .await
            .unwrap();
        assert!(
            service
                .resolve_token("wiab_pat_PLAINTEXT")
                .await
                .unwrap()
                .is_none()
        );
    }
}
