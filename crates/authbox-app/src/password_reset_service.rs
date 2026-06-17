use std::sync::Arc;

use authbox_core::auth::{
    AuthError, Clock, CredentialStore, EmailSender, PasswordCredential, PasswordHasher,
    SecretGenerator, SessionStore, UserDirectory, VerificationPurpose, VerificationToken,
    VerificationTokenStore,
};
use authbox_core::credential::TokenHasher;

/// Forgotten-password reset: `request` emails a single-use link; `confirm` consumes the
/// token, sets the new password, and revokes the user's sessions. Anti-enumeration: a
/// request for an unknown email succeeds silently and sends nothing.
pub struct PasswordResetService<D, V, C, S>
where
    D: UserDirectory,
    V: VerificationTokenStore,
    C: CredentialStore,
    S: SessionStore,
{
    directory: D,
    verifications: V,
    credentials: C,
    sessions: S,
    hasher: Arc<dyn PasswordHasher>,
    secrets: Arc<dyn SecretGenerator>,
    token_hasher: Arc<dyn TokenHasher>,
    clock: Arc<dyn Clock>,
    email_sender: Arc<dyn EmailSender>,
    base_url: String,
    token_ttl_seconds: i64,
}

impl<D, V, C, S> PasswordResetService<D, V, C, S>
where
    D: UserDirectory,
    V: VerificationTokenStore,
    C: CredentialStore,
    S: SessionStore,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        directory: D,
        verifications: V,
        credentials: C,
        sessions: S,
        hasher: Arc<dyn PasswordHasher>,
        secrets: Arc<dyn SecretGenerator>,
        token_hasher: Arc<dyn TokenHasher>,
        clock: Arc<dyn Clock>,
        email_sender: Arc<dyn EmailSender>,
        base_url: String,
        token_ttl_seconds: i64,
    ) -> Self {
        Self {
            directory,
            verifications,
            credentials,
            sessions,
            hasher,
            secrets,
            token_hasher,
            clock,
            email_sender,
            base_url,
            token_ttl_seconds,
        }
    }

    /// Request a reset. Always returns `Ok` regardless of whether the email is known, so the
    /// response does not reveal account existence.
    pub async fn request(&self, email: &str) -> Result<(), AuthError> {
        let Some(principal) = self.directory.find_by_email(email).await? else {
            return Ok(());
        };
        let plaintext = self.secrets.generate();
        let token_hash = self.token_hasher.hash(&plaintext);
        let expires_at = self.clock.rfc3339_in(self.token_ttl_seconds);
        self.verifications
            .put(VerificationToken::new(
                VerificationPurpose::PasswordReset,
                token_hash,
                principal,
                expires_at,
            ))
            .await?;
        let link = format!(
            "{}/reset-password?token={}",
            self.base_url.trim_end_matches('/'),
            plaintext
        );
        let body = format!(
            "Someone requested a password reset for your account.\n\n\
             Reset it here (the link expires soon and can be used once):\n{link}\n\n\
             If this wasn't you, you can ignore this email."
        );
        let sender = self.email_sender.clone();
        let to = email.to_owned();
        // Send off the async worker; a flaky mailer must not turn the request into an
        // account-existence oracle, so failures are swallowed (the sender logs them).
        let _ = tokio::task::spawn_blocking(move || sender.send(&to, "Reset your password", &body))
            .await;
        Ok(())
    }

    /// Confirm a reset: consume the token, set the new password, and revoke every session for
    /// the user (a reset implies the account may be compromised).
    pub async fn confirm(&self, token: &str, new_password: &str) -> Result<(), AuthError> {
        let token_hash = self.token_hasher.hash(token);
        let Some(record) = self.verifications.consume(&token_hash).await? else {
            return Err(AuthError::InvalidCredentials);
        };
        if record.purpose() != VerificationPurpose::PasswordReset
            || record.is_expired(&self.clock.now_rfc3339())
        {
            return Err(AuthError::InvalidCredentials);
        }
        let principal = record.principal().clone();
        let phc = self.hash(new_password.to_owned()).await?;
        self.credentials
            .save_password(PasswordCredential::new(
                principal.clone(),
                phc,
                self.clock.now_rfc3339(),
            ))
            .await?;
        self.sessions.revoke_all_for_principal(&principal).await?;
        Ok(())
    }

    async fn hash(&self, plaintext: String) -> Result<String, AuthError> {
        let hasher = self.hasher.clone();
        tokio::task::spawn_blocking(move || hasher.hash(&plaintext))
            .await
            .map_err(|error| AuthError::Backend(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use authbox_core::auth::PrincipalId;

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
            Err(AuthError::Backend("unused".to_owned()))
        }
    }

    #[derive(Default)]
    struct FakeVerifications {
        by_hash: Mutex<HashMap<String, VerificationToken>>,
    }
    impl VerificationTokenStore for FakeVerifications {
        async fn put(&self, token: VerificationToken) -> Result<(), AuthError> {
            self.by_hash
                .lock()
                .unwrap()
                .insert(token.token_hash().to_owned(), token);
            Ok(())
        }
        async fn consume(&self, token_hash: &str) -> Result<Option<VerificationToken>, AuthError> {
            Ok(self.by_hash.lock().unwrap().remove(token_hash))
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
        revoked: Mutex<Vec<String>>,
    }
    impl SessionStore for FakeSessions {
        async fn put(&self, _session: authbox_core::auth::Session) -> Result<(), AuthError> {
            Ok(())
        }
        async fn find_by_token_hash(
            &self,
            _token_hash: &str,
        ) -> Result<Option<authbox_core::auth::Session>, AuthError> {
            Ok(None)
        }
        async fn revoke_all_for_principal(&self, principal: &PrincipalId) -> Result<(), AuthError> {
            self.revoked
                .lock()
                .unwrap()
                .push(principal.as_str().to_owned());
            Ok(())
        }
    }

    struct FakeHasher;
    impl PasswordHasher for FakeHasher {
        fn hash(&self, plaintext: &str) -> String {
            format!("phc({plaintext})")
        }
        fn verify(&self, plaintext: &str, phc_hash: &str) -> bool {
            phc_hash == format!("phc({plaintext})")
        }
    }

    struct FakeSecrets;
    impl SecretGenerator for FakeSecrets {
        fn generate(&self) -> String {
            "RESET-SECRET".to_owned()
        }
    }

    struct FakeTokenHasher;
    impl TokenHasher for FakeTokenHasher {
        fn hash(&self, plaintext: &str) -> String {
            format!("h({plaintext})")
        }
    }

    struct FakeClock;
    impl Clock for FakeClock {
        fn now_rfc3339(&self) -> String {
            "2026-06-01T00:00:00Z".to_owned()
        }
        fn rfc3339_in(&self, _seconds: i64) -> String {
            "2999-01-01T00:00:00Z".to_owned()
        }
    }

    #[derive(Default, Clone)]
    struct CapturingEmail {
        last: Arc<Mutex<Option<(String, String)>>>,
    }
    impl EmailSender for CapturingEmail {
        fn send(&self, to: &str, _subject: &str, body: &str) -> Result<(), AuthError> {
            *self.last.lock().unwrap() = Some((to.to_owned(), body.to_owned()));
            Ok(())
        }
    }

    fn service(
        email: CapturingEmail,
    ) -> PasswordResetService<FakeDirectory, FakeVerifications, FakeCredentials, FakeSessions> {
        let mut directory = FakeDirectory::default();
        directory
            .by_email
            .insert("ada@example.com".to_owned(), "U-1".to_owned());
        PasswordResetService::new(
            directory,
            FakeVerifications::default(),
            FakeCredentials::default(),
            FakeSessions::default(),
            Arc::new(FakeHasher),
            Arc::new(FakeSecrets),
            Arc::new(FakeTokenHasher),
            Arc::new(FakeClock),
            Arc::new(email),
            "https://app.example".to_owned(),
            3600,
        )
    }

    fn token_from_link(body: &str) -> String {
        body.split("token=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned()
    }

    #[tokio::test]
    async fn request_then_confirm_resets_the_password() {
        let email = CapturingEmail::default();
        let svc = service(email.clone());
        svc.request("ada@example.com").await.unwrap();

        let (to, body) = email.last.lock().unwrap().clone().expect("email sent");
        assert_eq!(to, "ada@example.com");
        let token = token_from_link(&body);

        svc.confirm(&token, "new-password").await.unwrap();
        // The new password is stored hashed under the user, and the token is single-use.
        let stored = svc
            .credentials
            .find_password(&PrincipalId::new("U-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.phc_hash(), "phc(new-password)");
        assert!(svc.confirm(&token, "again").await.is_err());
        assert_eq!(svc.sessions.revoked.lock().unwrap().as_slice(), ["U-1"]);
    }

    #[tokio::test]
    async fn request_for_unknown_email_sends_nothing() {
        let email = CapturingEmail::default();
        let svc = service(email.clone());
        svc.request("nobody@example.com").await.unwrap();
        assert!(email.last.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn confirm_with_a_bad_token_is_rejected() {
        let svc = service(CapturingEmail::default());
        assert_eq!(
            svc.confirm("not-a-real-token", "whatever")
                .await
                .unwrap_err(),
            AuthError::InvalidCredentials
        );
    }
}
