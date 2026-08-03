use std::sync::Arc;

use authbox_core::auth::{
    AuthError, Clock, CredentialStore, MAX_PASSWORD_LENGTH, PasswordCredential, PasswordHasher,
    PrincipalId, SecretGenerator, Session, SessionId, SessionStore, UserDirectory,
    validate_password,
};
use authbox_core::credential::TokenHasher;
use subtle::ConstantTimeEq;

/// How many password hashes or verifies may run at once.
///
/// Each costs ~19 MiB in the blocking pool, and login is unauthenticated, so without a bound
/// the memory a stranger can make the process allocate is limited only by how fast they can
/// open connections — a few hundred concurrent logins is gigabytes. Capping concurrency bounds
/// peak memory to `permits x 19 MiB` no matter the request rate; excess attempts queue, which
/// is the right failure mode for a login.
const MAX_CONCURRENT_PASSWORD_HASHES: usize = 8;

/// Idle and absolute session lifetimes, in seconds.
#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    pub idle_seconds: i64,
    pub absolute_seconds: i64,
}

/// Returned once when a session is established: the plaintext cookie secret to set on the
/// browser and the CSRF token to hand the SPA. Only their hashes are persisted.
#[derive(Clone)]
pub struct EstablishedSession {
    pub cookie_secret: String,
    pub csrf_token: String,
}

/// Redacting `Debug`: both fields are live session credentials. Nothing logs them today, but
/// `Debug` is what a future `{:?}` or error context would reach for, and by then the secret is
/// in the log.
impl std::fmt::Debug for EstablishedSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EstablishedSession")
            .field("cookie_secret", &"<redacted>")
            .field("csrf_token", &"<redacted>")
            .finish()
    }
}

/// The outcome of resolving a session cookie: the authenticated principal and the stored
/// CSRF hash, so the caller can enforce double-submit CSRF on unsafe requests.
#[derive(Debug, Clone)]
pub struct ResolvedSession {
    pub principal: PrincipalId,
    pub csrf_hash: String,
}

/// Orchestrates password login and browser sessions over the auth ports. Generic over the
/// stores and the host's user directory; the small crypto/time seams are injected as
/// `Arc<dyn …>`, mirroring the WIAB application services.
pub struct AuthenticationService<S, C, D>
where
    S: SessionStore,
    C: CredentialStore,
    D: UserDirectory,
{
    sessions: S,
    credentials: C,
    directory: D,
    hasher: Arc<dyn PasswordHasher>,
    secrets: Arc<dyn SecretGenerator>,
    token_hasher: Arc<dyn TokenHasher>,
    clock: Arc<dyn Clock>,
    config: SessionConfig,
    /// A real PHC hash of a value nobody knows, used to spend the same Argon2 work on a login
    /// that cannot succeed. Computed once at construction so the cost per attempt is exactly
    /// one verify, whether or not the account exists.
    decoy_phc: String,
    /// Bounds concurrent Argon2 work; see [`MAX_CONCURRENT_PASSWORD_HASHES`].
    hash_permits: Arc<tokio::sync::Semaphore>,
}

impl<S, C, D> AuthenticationService<S, C, D>
where
    S: SessionStore,
    C: CredentialStore,
    D: UserDirectory,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sessions: S,
        credentials: C,
        directory: D,
        hasher: Arc<dyn PasswordHasher>,
        secrets: Arc<dyn SecretGenerator>,
        token_hasher: Arc<dyn TokenHasher>,
        clock: Arc<dyn Clock>,
        config: SessionConfig,
    ) -> Self {
        let decoy_phc = hasher.hash(&secrets.generate());
        Self {
            sessions,
            credentials,
            directory,
            hasher,
            secrets,
            token_hasher,
            clock,
            config,
            decoy_phc,
            hash_permits: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_PASSWORD_HASHES)),
        }
    }

    /// Verify an email/password pair and, on success, establish a session.
    ///
    /// Returns `InvalidCredentials` for an unknown email, a user with no password, or a bad
    /// password — the same error in every case, and after the same work. Returning early on a
    /// missing credential would skip the Argon2 verify, and that difference is measurable: it
    /// tells an attacker which addresses are registered even though every response looks
    /// identical. So the miss paths verify the presented password against a decoy hash and
    /// discard the answer.
    pub async fn login_with_password(
        &self,
        email: &str,
        password: &str,
    ) -> Result<EstablishedSession, AuthError> {
        // An over-long password is rejected before any hashing: it cannot match a stored
        // credential anyway (nothing that long was ever accepted), so refusing it early costs
        // an attacker the CPU they were trying to spend, and `InvalidCredentials` keeps the
        // response indistinguishable from any other failed login.
        if password.len() > MAX_PASSWORD_LENGTH {
            return Err(AuthError::InvalidCredentials);
        }

        let found = match self.directory.find_by_email(email).await? {
            Some(principal) => self
                .credentials
                .find_password(&principal)
                .await?
                .map(|credential| (principal, credential.phc_hash().to_owned())),
            None => None,
        };

        let (principal, phc_hash) = match found {
            Some(found) => found,
            None => {
                self.verify(password.to_owned(), self.decoy_phc.clone())
                    .await?;
                return Err(AuthError::InvalidCredentials);
            }
        };

        let ok = self.verify(password.to_owned(), phc_hash).await?;
        if !ok {
            return Err(AuthError::InvalidCredentials);
        }
        self.establish_session(principal).await
    }

    /// Spend the Argon2 cost a real password write would, and discard the result.
    ///
    /// For flows that must not reveal by their duration whether they did any work — signup
    /// returns the same 202 for a fresh and a taken address, which is undone if only one of
    /// them hashes. This matches the dominant cost, not every last microsecond: the branches
    /// still differ by a database write and an email send, both far below Argon2's ~19 MiB.
    pub async fn spend_password_cost(&self, plaintext: &str) -> Result<(), AuthError> {
        self.hash(plaintext.to_owned()).await?;
        Ok(())
    }

    /// Mint a fresh session for an already-authenticated principal (also the entry point the
    /// social/SSO slices use after verifying an external identity).
    pub async fn establish_session(
        &self,
        principal: PrincipalId,
    ) -> Result<EstablishedSession, AuthError> {
        let cookie_secret = self.secrets.generate();
        let csrf_token = self.secrets.generate();
        let token_hash = self.token_hasher.hash(&cookie_secret);
        let csrf_hash = self.token_hasher.hash(&csrf_token);
        let now = self.clock.now_rfc3339();
        let idle_expires_at = self.clock.rfc3339_in(self.config.idle_seconds);
        let absolute_expires_at = self.clock.rfc3339_in(self.config.absolute_seconds);
        let session = Session::new(
            SessionId::new(),
            principal,
            token_hash,
            csrf_hash,
            now,
            idle_expires_at,
            absolute_expires_at,
        );
        self.sessions.put(session).await?;
        Ok(EstablishedSession {
            cookie_secret,
            csrf_token,
        })
    }

    /// Resolve a presented cookie secret to its principal, rejecting an expired/revoked
    /// session, and slide the idle window forward on success.
    pub async fn resolve_session(
        &self,
        cookie_secret: &str,
    ) -> Result<Option<ResolvedSession>, AuthError> {
        let token_hash = self.token_hasher.hash(cookie_secret);
        let Some(mut session) = self.sessions.find_by_token_hash(&token_hash).await? else {
            return Ok(None);
        };
        let now = self.clock.now_rfc3339();
        if !session.is_active(&now) {
            return Ok(None);
        }
        let principal = session.principal().clone();
        let csrf_hash = session.csrf_hash().to_owned();
        // Slide the idle window; the absolute expiry is never extended.
        session.touch(now, self.clock.rfc3339_in(self.config.idle_seconds));
        self.sessions.put(session).await?;
        Ok(Some(ResolvedSession {
            principal,
            csrf_hash,
        }))
    }

    /// Double-submit CSRF check: true when the presented token hashes to the session's stored
    /// CSRF hash. Empty tokens never match.
    ///
    /// Constant-time comparison. What is compared here is a pair of SHA-256 digests, not the
    /// secret, so a prefix-timing oracle would not give an attacker a way to construct a
    /// matching token — this is consistency with the rest of the crypto discipline rather than
    /// a live vector, and it costs nothing.
    pub fn csrf_matches(&self, session: &ResolvedSession, presented: &str) -> bool {
        if presented.is_empty() {
            return false;
        }
        let presented_hash = self.token_hasher.hash(presented);
        // `ct_eq` is only constant-time for equal-length inputs; both sides are hex digests of
        // the same hash, so a length mismatch means a malformed stored value, not a secret.
        presented_hash.len() == session.csrf_hash.len()
            && bool::from(
                presented_hash
                    .as_bytes()
                    .ct_eq(session.csrf_hash.as_bytes()),
            )
    }

    /// Revoke the session a cookie secret resolves to. Idempotent.
    pub async fn logout(&self, cookie_secret: &str) -> Result<(), AuthError> {
        let token_hash = self.token_hasher.hash(cookie_secret);
        if let Some(mut session) = self.sessions.find_by_token_hash(&token_hash).await? {
            session.revoke();
            self.sessions.put(session).await?;
        }
        Ok(())
    }

    /// Set (or replace) a principal's password. Used by the dev owner seed now, and by
    /// signup / invite-accept / reset later.
    pub async fn set_password(
        &self,
        principal: PrincipalId,
        plaintext: &str,
    ) -> Result<(), AuthError> {
        validate_password(plaintext)?;
        let phc_hash = self.hash(plaintext.to_owned()).await?;
        let credential = PasswordCredential::new(principal, phc_hash, self.clock.now_rfc3339());
        self.credentials.save_password(credential).await
    }

    /// Change a principal's own password after re-verifying the current one. Existing
    /// sessions are left intact (a voluntary change, not a compromise reset). Returns
    /// `InvalidCredentials` if there is no password or the current one is wrong.
    pub async fn change_password(
        &self,
        principal: PrincipalId,
        current: &str,
        new: &str,
    ) -> Result<(), AuthError> {
        let credential = self
            .credentials
            .find_password(&principal)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;
        if !self
            .verify(current.to_owned(), credential.phc_hash().to_owned())
            .await?
        {
            return Err(AuthError::InvalidCredentials);
        }
        self.set_password(principal, new).await
    }

    /// Revoke every session for a principal — used when a user is deactivated.
    pub async fn revoke_all_sessions(&self, principal: &PrincipalId) -> Result<(), AuthError> {
        self.sessions.revoke_all_for_principal(principal).await
    }

    /// Run argon2 hashing off the async worker — it is deliberately CPU/memory-bound.
    async fn hash(&self, plaintext: String) -> Result<String, AuthError> {
        let _permit = self.hash_permit().await?;
        let hasher = self.hasher.clone();
        tokio::task::spawn_blocking(move || hasher.hash(&plaintext))
            .await
            .map_err(|error| AuthError::Backend(error.to_string()))
    }

    async fn verify(&self, plaintext: String, phc_hash: String) -> Result<bool, AuthError> {
        let _permit = self.hash_permit().await?;
        let hasher = self.hasher.clone();
        tokio::task::spawn_blocking(move || hasher.verify(&plaintext, &phc_hash))
            .await
            .map_err(|error| AuthError::Backend(error.to_string()))
    }

    /// Waits for a slot to do Argon2 work. Held for the duration of the hash, so at most
    /// [`MAX_CONCURRENT_PASSWORD_HASHES`] are in flight.
    async fn hash_permit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, AuthError> {
        self.hash_permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| AuthError::Backend(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[test]
    fn debug_never_prints_the_session_secrets() {
        let session = EstablishedSession {
            cookie_secret: "c00kie".to_owned(),
            csrf_token: "csrft0ken".to_owned(),
        };
        let rendered = format!("{session:?}");
        assert!(!rendered.contains("c00kie"), "Debug leaked: {rendered}");
        assert!(!rendered.contains("csrft0ken"), "Debug leaked: {rendered}");
    }

    #[derive(Default)]
    struct FakeDirectory {
        by_email: HashMap<String, String>,
    }
    impl UserDirectory for FakeDirectory {
        async fn find_by_email(&self, email: &str) -> Result<Option<PrincipalId>, AuthError> {
            Ok(self.by_email.get(email).map(PrincipalId::new))
        }
        async fn may_authenticate(&self, _principal: &PrincipalId) -> Result<bool, AuthError> {
            Ok(true)
        }
        async fn provision(&self, _email: &str, _name: &str) -> Result<PrincipalId, AuthError> {
            Err(AuthError::Backend(
                "provision not used in this test".to_owned(),
            ))
        }
    }

    #[derive(Default)]
    struct FakeCredentials {
        by_principal: Mutex<HashMap<String, PasswordCredential>>,
    }
    impl CredentialStore for FakeCredentials {
        async fn find_password(
            &self,
            principal: &PrincipalId,
        ) -> Result<Option<PasswordCredential>, AuthError> {
            Ok(self
                .by_principal
                .lock()
                .unwrap()
                .get(principal.as_str())
                .cloned())
        }
        async fn save_password(&self, credential: PasswordCredential) -> Result<(), AuthError> {
            self.by_principal
                .lock()
                .unwrap()
                .insert(credential.principal().as_str().to_owned(), credential);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeSessions {
        by_token_hash: Mutex<HashMap<String, Session>>,
    }
    impl SessionStore for FakeSessions {
        async fn put(&self, session: Session) -> Result<(), AuthError> {
            self.by_token_hash
                .lock()
                .unwrap()
                .insert(session.token_hash().to_owned(), session);
            Ok(())
        }
        async fn find_by_token_hash(&self, token_hash: &str) -> Result<Option<Session>, AuthError> {
            Ok(self.by_token_hash.lock().unwrap().get(token_hash).cloned())
        }
        async fn revoke_all_for_principal(&self, principal: &PrincipalId) -> Result<(), AuthError> {
            for session in self.by_token_hash.lock().unwrap().values_mut() {
                if session.principal() == principal {
                    session.revoke();
                }
            }
            Ok(())
        }
    }

    /// Reversible "hash" so the fake can verify deterministically.
    #[derive(Default)]
    struct FakeHasher {
        /// How many verifies were performed. The timing oracle is a *work* difference, so it
        /// is counted rather than timed — a wall-clock assertion would be flaky and would not
        /// actually pin the behaviour.
        verifies: AtomicU64,
        in_flight: AtomicU64,
        peak_concurrent: AtomicU64,
    }
    impl FakeHasher {
        /// Holds a slot for long enough that overlapping work is observable.
        fn enter(&self) {
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak_concurrent.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        fn leave(&self) {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
        }
    }
    impl PasswordHasher for FakeHasher {
        fn hash(&self, plaintext: &str) -> String {
            self.enter();
            let out = format!("phc({plaintext})");
            self.leave();
            out
        }
        fn verify(&self, plaintext: &str, phc_hash: &str) -> bool {
            self.verifies.fetch_add(1, Ordering::SeqCst);
            self.enter();
            let out = phc_hash == format!("phc({plaintext})");
            self.leave();
            out
        }
    }

    #[derive(Default)]
    struct FakeSecrets {
        counter: AtomicU64,
    }
    impl SecretGenerator for FakeSecrets {
        fn generate(&self) -> String {
            format!("secret-{}", self.counter.fetch_add(1, Ordering::SeqCst))
        }
    }

    struct FakeTokenHasher;
    impl TokenHasher for FakeTokenHasher {
        fn hash(&self, plaintext: &str) -> String {
            format!("h({plaintext})")
        }
    }

    /// Fixed clock whose future timestamps are lexically far ahead, so sessions stay active.
    struct FakeClock;
    impl Clock for FakeClock {
        fn now_rfc3339(&self) -> String {
            "2026-06-01T00:00:00Z".to_owned()
        }
        fn rfc3339_in(&self, _seconds: i64) -> String {
            "2999-01-01T00:00:00Z".to_owned()
        }
    }

    fn service() -> AuthenticationService<FakeSessions, FakeCredentials, FakeDirectory> {
        service_with(Arc::new(FakeHasher::default()))
    }

    fn service_with(
        hasher: Arc<FakeHasher>,
    ) -> AuthenticationService<FakeSessions, FakeCredentials, FakeDirectory> {
        let mut directory = FakeDirectory::default();
        directory
            .by_email
            .insert("ada@example.com".to_owned(), "U-1".to_owned());
        AuthenticationService::new(
            FakeSessions::default(),
            FakeCredentials::default(),
            directory,
            hasher,
            Arc::new(FakeSecrets::default()),
            Arc::new(FakeTokenHasher),
            Arc::new(FakeClock),
            SessionConfig {
                idle_seconds: 3600,
                absolute_seconds: 86_400,
            },
        )
    }

    /// Peak Argon2 memory has to be bounded by the semaphore, not by how fast a stranger can
    /// open connections — login is unauthenticated and each verify costs ~19 MiB.
    #[tokio::test]
    async fn concurrent_password_work_is_capped() {
        let hasher = Arc::new(FakeHasher::default());
        let service = Arc::new(service_with(hasher.clone()));
        service
            .set_password(PrincipalId::new("U-1"), "correct horse")
            .await
            .unwrap();

        let attempts = MAX_CONCURRENT_PASSWORD_HASHES * 4;
        let mut handles = Vec::new();
        for _ in 0..attempts {
            let service = service.clone();
            handles.push(tokio::spawn(async move {
                let _ = service
                    .login_with_password("ada@example.com", "guess")
                    .await;
            }));
        }
        for handle in handles {
            handle.await.expect("task completes");
        }

        // An upper bound, not an equality: how many actually overlap depends on the runner,
        // and asserting the exact peak would be flaky without testing anything more. The bound
        // is the property that matters — it is what caps peak memory.
        let peak = hasher.peak_concurrent.load(Ordering::SeqCst) as usize;
        assert!(
            peak <= MAX_CONCURRENT_PASSWORD_HASHES,
            "{peak} hashes ran at once, above the cap of {MAX_CONCURRENT_PASSWORD_HASHES}"
        );
    }

    /// The oracle this closes: an unknown address used to return before any Argon2 work, so
    /// "does this account exist" was answerable from response time alone even though every
    /// response body and status is identical.
    #[tokio::test]
    async fn a_failed_login_costs_the_same_whether_or_not_the_account_exists() {
        let unknown_email = {
            let hasher = Arc::new(FakeHasher::default());
            let service = service_with(hasher.clone());
            hasher.verifies.store(0, Ordering::SeqCst);
            let _ = service
                .login_with_password("nobody@example.com", "guess")
                .await;
            hasher.verifies.load(Ordering::SeqCst)
        };

        let known_email_wrong_password = {
            let hasher = Arc::new(FakeHasher::default());
            let service = service_with(hasher.clone());
            service
                .set_password(PrincipalId::new("U-1"), "correct horse")
                .await
                .unwrap();
            hasher.verifies.store(0, Ordering::SeqCst);
            let _ = service
                .login_with_password("ada@example.com", "guess")
                .await;
            hasher.verifies.load(Ordering::SeqCst)
        };

        let known_email_no_password = {
            let hasher = Arc::new(FakeHasher::default());
            let service = service_with(hasher.clone());
            hasher.verifies.store(0, Ordering::SeqCst);
            let _ = service
                .login_with_password("ada@example.com", "guess")
                .await;
            hasher.verifies.load(Ordering::SeqCst)
        };

        assert_eq!(known_email_wrong_password, 1, "the baseline is one verify");
        assert_eq!(
            unknown_email, known_email_wrong_password,
            "an unknown address must cost the same as a wrong password"
        );
        assert_eq!(
            known_email_no_password, known_email_wrong_password,
            "an account with no password must cost the same as a wrong password"
        );
    }

    #[tokio::test]
    async fn login_then_resolve_then_logout() {
        let service = service();
        service
            .set_password(PrincipalId::new("U-1"), "correct horse")
            .await
            .unwrap();

        let established = service
            .login_with_password("ada@example.com", "correct horse")
            .await
            .unwrap();

        let resolved = service
            .resolve_session(&established.cookie_secret)
            .await
            .unwrap()
            .expect("session resolves");
        assert_eq!(resolved.principal.as_str(), "U-1");
        // The CSRF token the SPA holds hashes to the stored csrf hash.
        assert_eq!(resolved.csrf_hash, format!("h({})", established.csrf_token));

        service.logout(&established.cookie_secret).await.unwrap();
        assert!(
            service
                .resolve_session(&established.cookie_secret)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn csrf_matches_only_the_session_token() {
        let service = service();
        service
            .set_password(PrincipalId::new("U-1"), "correct horse")
            .await
            .unwrap();
        let established = service
            .login_with_password("ada@example.com", "correct horse")
            .await
            .unwrap();
        let resolved = service
            .resolve_session(&established.cookie_secret)
            .await
            .unwrap()
            .expect("session resolves");

        assert!(service.csrf_matches(&resolved, &established.csrf_token));
        assert!(!service.csrf_matches(&resolved, "not-the-token"));
        assert!(!service.csrf_matches(&resolved, ""));
    }

    #[tokio::test]
    async fn wrong_password_is_rejected() {
        let service = service();
        service
            .set_password(PrincipalId::new("U-1"), "correct horse")
            .await
            .unwrap();
        assert_eq!(
            service
                .login_with_password("ada@example.com", "wrong")
                .await
                .unwrap_err(),
            AuthError::InvalidCredentials
        );
    }

    #[tokio::test]
    async fn unknown_email_is_rejected() {
        let service = service();
        assert_eq!(
            service
                .login_with_password("nobody@example.com", "whatever")
                .await
                .unwrap_err(),
            AuthError::InvalidCredentials
        );
    }
}
