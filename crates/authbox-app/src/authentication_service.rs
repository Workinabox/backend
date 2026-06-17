use std::sync::Arc;

use authbox_core::auth::{
    AuthError, Clock, CredentialStore, PasswordCredential, PasswordHasher, PrincipalId,
    SecretGenerator, Session, SessionId, SessionStore, UserDirectory,
};
use authbox_core::credential::TokenHasher;

/// Idle and absolute session lifetimes, in seconds.
#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    pub idle_seconds: i64,
    pub absolute_seconds: i64,
}

/// Returned once when a session is established: the plaintext cookie secret to set on the
/// browser and the CSRF token to hand the SPA. Only their hashes are persisted.
#[derive(Debug, Clone)]
pub struct EstablishedSession {
    pub cookie_secret: String,
    pub csrf_token: String,
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
        Self {
            sessions,
            credentials,
            directory,
            hasher,
            secrets,
            token_hasher,
            clock,
            config,
        }
    }

    /// Verify an email/password pair and, on success, establish a session.
    ///
    /// Returns `InvalidCredentials` for an unknown email, a user with no password, or a bad
    /// password — the same error in every case so the response does not distinguish them.
    /// (A login-timing oracle still exists for now: verification only runs when a credential
    /// is present. Flattening it with a dummy hash is a follow-up once the argon2 impl can
    /// supply a constant PHC.)
    pub async fn login_with_password(
        &self,
        email: &str,
        password: &str,
    ) -> Result<EstablishedSession, AuthError> {
        let Some(principal) = self.directory.find_by_email(email).await? else {
            return Err(AuthError::InvalidCredentials);
        };
        let Some(credential) = self.credentials.find_password(&principal).await? else {
            return Err(AuthError::InvalidCredentials);
        };
        let ok = self
            .verify(password.to_owned(), credential.phc_hash().to_owned())
            .await?;
        if !ok {
            return Err(AuthError::InvalidCredentials);
        }
        self.establish_session(principal).await
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
        let hasher = self.hasher.clone();
        tokio::task::spawn_blocking(move || hasher.hash(&plaintext))
            .await
            .map_err(|error| AuthError::Backend(error.to_string()))
    }

    async fn verify(&self, plaintext: String, phc_hash: String) -> Result<bool, AuthError> {
        let hasher = self.hasher.clone();
        tokio::task::spawn_blocking(move || hasher.verify(&plaintext, &phc_hash))
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

    #[derive(Default)]
    struct FakeDirectory {
        by_email: HashMap<String, String>,
    }
    impl UserDirectory for FakeDirectory {
        async fn find_by_email(&self, email: &str) -> Result<Option<PrincipalId>, AuthError> {
            Ok(self.by_email.get(email).map(PrincipalId::new))
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
    struct FakeHasher;
    impl PasswordHasher for FakeHasher {
        fn hash(&self, plaintext: &str) -> String {
            format!("phc({plaintext})")
        }
        fn verify(&self, plaintext: &str, phc_hash: &str) -> bool {
            phc_hash == format!("phc({plaintext})")
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
        let mut directory = FakeDirectory::default();
        directory
            .by_email
            .insert("ada@example.com".to_owned(), "U-1".to_owned());
        AuthenticationService::new(
            FakeSessions::default(),
            FakeCredentials::default(),
            directory,
            Arc::new(FakeHasher),
            Arc::new(FakeSecrets::default()),
            Arc::new(FakeTokenHasher),
            Arc::new(FakeClock),
            SessionConfig {
                idle_seconds: 3600,
                absolute_seconds: 86_400,
            },
        )
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
