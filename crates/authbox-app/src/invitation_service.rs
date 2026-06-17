use std::sync::Arc;

use authbox_core::auth::{
    AuthError, Clock, CredentialStore, EmailSender, PasswordCredential, PasswordHasher,
    PrincipalId, SecretGenerator, VerificationPurpose, VerificationToken, VerificationTokenStore,
};
use authbox_core::credential::TokenHasher;

/// Invitations and signup email-verification — both email a single-use link that activates
/// a `Pending` user. `accept_invite` also sets the invitee's first password; `verify_email`
/// just confirms (the password was set at signup). Activation of the host's user record is
/// the caller's job (this service owns the token + email + credential, not lifecycle).
pub struct InvitationService<V, C>
where
    V: VerificationTokenStore,
    C: CredentialStore,
{
    verifications: V,
    credentials: C,
    hasher: Arc<dyn PasswordHasher>,
    secrets: Arc<dyn SecretGenerator>,
    token_hasher: Arc<dyn TokenHasher>,
    clock: Arc<dyn Clock>,
    email_sender: Arc<dyn EmailSender>,
    base_url: String,
    token_ttl_seconds: i64,
}

impl<V, C> InvitationService<V, C>
where
    V: VerificationTokenStore,
    C: CredentialStore,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        verifications: V,
        credentials: C,
        hasher: Arc<dyn PasswordHasher>,
        secrets: Arc<dyn SecretGenerator>,
        token_hasher: Arc<dyn TokenHasher>,
        clock: Arc<dyn Clock>,
        email_sender: Arc<dyn EmailSender>,
        base_url: String,
        token_ttl_seconds: i64,
    ) -> Self {
        Self {
            verifications,
            credentials,
            hasher,
            secrets,
            token_hasher,
            clock,
            email_sender,
            base_url,
            token_ttl_seconds,
        }
    }

    /// Email an invite link; the recipient sets a password to activate their account.
    pub async fn invite(&self, email: &str, principal: PrincipalId) -> Result<(), AuthError> {
        let token = self.issue(principal, VerificationPurpose::Invite).await?;
        self.email_link(
            email,
            "You've been invited",
            "accept-invite",
            &token,
            "You've been invited. Set your password to finish setting up your account:",
        )
        .await;
        Ok(())
    }

    /// Email a confirmation link to a just-signed-up user (their password is already set).
    pub async fn send_email_verification(
        &self,
        email: &str,
        principal: PrincipalId,
    ) -> Result<(), AuthError> {
        let token = self
            .issue(principal, VerificationPurpose::EmailVerify)
            .await?;
        self.email_link(
            email,
            "Confirm your email",
            "verify-email",
            &token,
            "Confirm your email address to activate your account:",
        )
        .await;
        Ok(())
    }

    /// Accept an invite: consume the token, set the first password, and return the principal
    /// (the caller activates the user). `InvalidCredentials` for a bad/expired/wrong token.
    pub async fn accept_invite(
        &self,
        token: &str,
        password: &str,
    ) -> Result<PrincipalId, AuthError> {
        let principal = self.consume(token, VerificationPurpose::Invite).await?;
        let phc = self.hash(password.to_owned()).await?;
        self.credentials
            .save_password(PasswordCredential::new(
                principal.clone(),
                phc,
                self.clock.now_rfc3339(),
            ))
            .await?;
        Ok(principal)
    }

    /// Confirm a signup email: consume the token and return the principal (the caller
    /// activates the user). No password is set — it was provided at signup.
    pub async fn verify_email(&self, token: &str) -> Result<PrincipalId, AuthError> {
        self.consume(token, VerificationPurpose::EmailVerify).await
    }

    async fn issue(
        &self,
        principal: PrincipalId,
        purpose: VerificationPurpose,
    ) -> Result<String, AuthError> {
        let plaintext = self.secrets.generate();
        let token_hash = self.token_hasher.hash(&plaintext);
        let expires_at = self.clock.rfc3339_in(self.token_ttl_seconds);
        self.verifications
            .put(VerificationToken::new(
                purpose, token_hash, principal, expires_at,
            ))
            .await?;
        Ok(plaintext)
    }

    async fn consume(
        &self,
        token: &str,
        expected: VerificationPurpose,
    ) -> Result<PrincipalId, AuthError> {
        let token_hash = self.token_hasher.hash(token);
        let Some(record) = self.verifications.consume(&token_hash).await? else {
            return Err(AuthError::InvalidCredentials);
        };
        if record.purpose() != expected || record.is_expired(&self.clock.now_rfc3339()) {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(record.principal().clone())
    }

    async fn email_link(&self, to: &str, subject: &str, path: &str, token: &str, intro: &str) {
        let link = format!(
            "{}/{}?token={}",
            self.base_url.trim_end_matches('/'),
            path,
            token
        );
        let body = format!("{intro}\n\n{link}\n\nThis link expires soon and can be used once.");
        let sender = self.email_sender.clone();
        let to = to.to_owned();
        let subject = subject.to_owned();
        let _ = tokio::task::spawn_blocking(move || sender.send(&to, &subject, &body)).await;
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

    use super::*;

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
    struct CountingSecrets {
        count: std::sync::atomic::AtomicU64,
    }
    impl SecretGenerator for CountingSecrets {
        fn generate(&self) -> String {
            use std::sync::atomic::Ordering;
            format!("secret-{}", self.count.fetch_add(1, Ordering::SeqCst))
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
        last: Arc<Mutex<Option<String>>>,
    }
    impl EmailSender for CapturingEmail {
        fn send(&self, _to: &str, _subject: &str, body: &str) -> Result<(), AuthError> {
            *self.last.lock().unwrap() = Some(body.to_owned());
            Ok(())
        }
    }

    fn service(email: CapturingEmail) -> InvitationService<FakeVerifications, FakeCredentials> {
        InvitationService::new(
            FakeVerifications::default(),
            FakeCredentials::default(),
            Arc::new(FakeHasher),
            Arc::new(CountingSecrets::default()),
            Arc::new(FakeTokenHasher),
            Arc::new(FakeClock),
            Arc::new(email),
            "https://app.example".to_owned(),
            86_400,
        )
    }

    fn token_from(body: &str) -> String {
        body.split("token=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned()
    }

    #[tokio::test]
    async fn invite_then_accept_sets_password() {
        let email = CapturingEmail::default();
        let svc = service(email.clone());
        svc.invite("ada@example.com", PrincipalId::new("U-5"))
            .await
            .unwrap();
        let body = email.last.lock().unwrap().clone().unwrap();
        assert!(body.contains("accept-invite?token="));
        let token = token_from(&body);

        let principal = svc.accept_invite(&token, "chosen-password").await.unwrap();
        assert_eq!(principal.as_str(), "U-5");
        let stored = svc
            .credentials
            .find_password(&PrincipalId::new("U-5"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.phc_hash(), "phc(chosen-password)");
        // Single-use.
        assert!(svc.accept_invite(&token, "again").await.is_err());
    }

    #[tokio::test]
    async fn signup_verification_returns_principal_without_password() {
        let email = CapturingEmail::default();
        let svc = service(email.clone());
        svc.send_email_verification("ada@example.com", PrincipalId::new("U-9"))
            .await
            .unwrap();
        let body = email.last.lock().unwrap().clone().unwrap();
        assert!(body.contains("verify-email?token="));
        let token = token_from(&body);

        let principal = svc.verify_email(&token).await.unwrap();
        assert_eq!(principal.as_str(), "U-9");
        // No password was set, and an invite token can't be used to verify.
        assert!(svc.verify_email(&token).await.is_err());
    }

    #[tokio::test]
    async fn wrong_purpose_token_is_rejected() {
        let email = CapturingEmail::default();
        let svc = service(email.clone());
        svc.invite("ada@example.com", PrincipalId::new("U-1"))
            .await
            .unwrap();
        let token = token_from(&email.last.lock().unwrap().clone().unwrap());
        // An invite token must not satisfy email verification.
        assert!(svc.verify_email(&token).await.is_err());
    }
}
