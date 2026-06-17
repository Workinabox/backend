use std::sync::Arc;

use authbox_core::auth::{
    AuthError, AuthFlow, AuthFlowStore, Clock, FederatedIdentity, FederatedIdentityStore,
    FederationConnection, OidcPort, PrincipalId, UserDirectory,
};

/// Orchestrates inbound OIDC federation (Google and the enterprise IdP — same code path,
/// different connection config). `begin_login` starts an authorization-code + PKCE flow;
/// `complete_login` validates the callback and resolves the external identity to a host
/// principal (existing link → provisioned/invited user → just-in-time new user), returning
/// the principal so the caller can establish a session.
pub struct FederationService<F, A, D, O>
where
    F: FederatedIdentityStore,
    A: AuthFlowStore,
    D: UserDirectory,
    O: OidcPort,
{
    federated: F,
    flows: A,
    directory: D,
    oidc: O,
    clock: Arc<dyn Clock>,
    connections: Vec<FederationConnection>,
    flow_ttl_seconds: i64,
}

impl<F, A, D, O> FederationService<F, A, D, O>
where
    F: FederatedIdentityStore,
    A: AuthFlowStore,
    D: UserDirectory,
    O: OidcPort,
{
    pub fn new(
        federated: F,
        flows: A,
        directory: D,
        oidc: O,
        clock: Arc<dyn Clock>,
        connections: Vec<FederationConnection>,
        flow_ttl_seconds: i64,
    ) -> Self {
        Self {
            federated,
            flows,
            directory,
            oidc,
            clock,
            connections,
            flow_ttl_seconds,
        }
    }

    fn connection(&self, slug: &str) -> Result<&FederationConnection, AuthError> {
        self.connections
            .iter()
            .find(|connection| connection.slug == slug)
            .ok_or_else(|| AuthError::FederationFailed(format!("unknown connection '{slug}'")))
    }

    /// Start a login: build the IdP authorization URL and persist the per-attempt state
    /// (PKCE verifier, nonce, where to return) for the callback. Returns the URL to redirect
    /// the browser to.
    pub async fn begin_login(
        &self,
        connection: &str,
        return_to: &str,
    ) -> Result<String, AuthError> {
        self.connection(connection)?;
        let request = self.oidc.begin(connection).await?;
        let flow = AuthFlow::new(
            request.state,
            connection.to_owned(),
            request.nonce,
            request.pkce_verifier,
            return_to.to_owned(),
            self.clock.rfc3339_in(self.flow_ttl_seconds),
        );
        self.flows.put(flow).await?;
        Ok(request.authorize_url)
    }

    /// Complete a login from the IdP callback. Returns the resolved principal and the
    /// original `return_to`.
    pub async fn complete_login(
        &self,
        connection: &str,
        state: &str,
        code: &str,
    ) -> Result<(PrincipalId, String), AuthError> {
        // The state must match a stored, unconsumed, unexpired flow for this connection —
        // this single-use lookup is the CSRF/state check.
        let Some(flow) = self.flows.take(state).await? else {
            return Err(AuthError::FederationFailed(
                "unknown or already-used login state".to_owned(),
            ));
        };
        if flow.connection() != connection || flow.is_expired(&self.clock.now_rfc3339()) {
            return Err(AuthError::FederationFailed(
                "invalid or expired login state".to_owned(),
            ));
        }

        let claims = self
            .oidc
            .complete(connection, code, flow.pkce_verifier(), flow.nonce())
            .await?;

        // Existing link wins — `subject` is the durable key, never the email.
        if let Some(identity) = self.federated.find(&claims.issuer, &claims.subject).await? {
            return Ok((identity.principal().clone(), flow.return_to().to_owned()));
        }

        // Provisioning/linking needs a verified email.
        let email = match (&claims.email, claims.email_verified) {
            (Some(email), true) => email.clone(),
            _ => {
                return Err(AuthError::FederationFailed(
                    "the identity provider did not supply a verified email".to_owned(),
                ));
            }
        };

        let connection_config = self.connection(connection)?;
        let principal = match self.directory.find_by_email(&email).await? {
            Some(existing) => {
                if connection_config.auto_link_verified_email {
                    existing
                } else {
                    // A local account exists but this connection won't silently adopt it.
                    return Err(AuthError::AccountExists);
                }
            }
            None => {
                let name = claims.name.clone().unwrap_or_else(|| email.clone());
                self.directory.provision(&email, &name).await?
            }
        };

        let identity = FederatedIdentity::new(
            principal.clone(),
            claims.issuer,
            claims.subject,
            Some(email),
            self.clock.now_rfc3339(),
        );
        self.federated.link(identity).await?;
        Ok((principal, flow.return_to().to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use authbox_core::auth::{AuthRequest, VerifiedClaims};

    use super::*;

    struct FakeOidc {
        claims: VerifiedClaims,
    }
    impl OidcPort for FakeOidc {
        async fn begin(&self, _connection: &str) -> Result<AuthRequest, AuthError> {
            Ok(AuthRequest {
                authorize_url: "https://idp.example/authorize?state=st".to_owned(),
                state: "st".to_owned(),
                nonce: "no".to_owned(),
                pkce_verifier: "pk".to_owned(),
            })
        }
        async fn complete(
            &self,
            _connection: &str,
            _code: &str,
            _pkce_verifier: &str,
            _expected_nonce: &str,
        ) -> Result<VerifiedClaims, AuthError> {
            Ok(self.claims.clone())
        }
    }

    #[derive(Default)]
    struct FakeFederated {
        by_subject: Mutex<HashMap<(String, String), FederatedIdentity>>,
    }
    impl FederatedIdentityStore for FakeFederated {
        async fn find(
            &self,
            issuer: &str,
            subject: &str,
        ) -> Result<Option<FederatedIdentity>, AuthError> {
            Ok(self
                .by_subject
                .lock()
                .unwrap()
                .get(&(issuer.to_owned(), subject.to_owned()))
                .cloned())
        }
        async fn link(&self, identity: FederatedIdentity) -> Result<(), AuthError> {
            self.by_subject.lock().unwrap().insert(
                (identity.issuer().to_owned(), identity.subject().to_owned()),
                identity,
            );
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeFlows {
        by_state: Mutex<HashMap<String, AuthFlow>>,
    }
    impl AuthFlowStore for FakeFlows {
        async fn put(&self, flow: AuthFlow) -> Result<(), AuthError> {
            self.by_state
                .lock()
                .unwrap()
                .insert(flow.state().to_owned(), flow);
            Ok(())
        }
        async fn take(&self, state: &str) -> Result<Option<AuthFlow>, AuthError> {
            Ok(self.by_state.lock().unwrap().remove(state))
        }
    }

    #[derive(Default)]
    struct FakeDirectory {
        by_email: Mutex<HashMap<String, String>>,
        next_id: Mutex<u64>,
    }
    impl UserDirectory for FakeDirectory {
        async fn find_by_email(&self, email: &str) -> Result<Option<PrincipalId>, AuthError> {
            Ok(self
                .by_email
                .lock()
                .unwrap()
                .get(email)
                .map(PrincipalId::new))
        }
        async fn provision(&self, email: &str, _name: &str) -> Result<PrincipalId, AuthError> {
            let mut next = self.next_id.lock().unwrap();
            *next += 1;
            let id = format!("U-{next}");
            self.by_email
                .lock()
                .unwrap()
                .insert(email.to_owned(), id.clone());
            Ok(PrincipalId::new(id))
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

    fn connections() -> Vec<FederationConnection> {
        vec![
            FederationConnection {
                slug: "google".to_owned(),
                issuer: "https://accounts.google.com".to_owned(),
                client_id: "cid".to_owned(),
                client_secret: "secret".to_owned(),
                scopes: vec!["openid".to_owned(), "email".to_owned()],
                redirect_uri: "https://app/api/auth/oidc/google/callback".to_owned(),
                auto_link_verified_email: false,
            },
            FederationConnection {
                slug: "enterprise".to_owned(),
                issuer: "https://idp.corp".to_owned(),
                client_id: "cid".to_owned(),
                client_secret: "secret".to_owned(),
                scopes: vec!["openid".to_owned(), "email".to_owned()],
                redirect_uri: "https://app/api/auth/oidc/enterprise/callback".to_owned(),
                auto_link_verified_email: true,
            },
        ]
    }

    fn service(
        claims: VerifiedClaims,
        directory: FakeDirectory,
    ) -> FederationService<FakeFederated, FakeFlows, FakeDirectory, FakeOidc> {
        FederationService::new(
            FakeFederated::default(),
            FakeFlows::default(),
            directory,
            FakeOidc { claims },
            Arc::new(FakeClock),
            connections(),
            600,
        )
    }

    fn claims(email: &str, verified: bool) -> VerifiedClaims {
        VerifiedClaims {
            issuer: "https://idp.corp".to_owned(),
            subject: "sub-123".to_owned(),
            email: Some(email.to_owned()),
            email_verified: verified,
            name: Some("Ada".to_owned()),
        }
    }

    #[tokio::test]
    async fn jit_provisions_a_new_user_on_first_login() {
        let svc = service(claims("ada@corp.com", true), FakeDirectory::default());
        let url = svc.begin_login("enterprise", "/works").await.unwrap();
        assert!(url.contains("state=st"));
        let (principal, return_to) = svc
            .complete_login("enterprise", "st", "code")
            .await
            .unwrap();
        assert_eq!(principal.as_str(), "U-1");
        assert_eq!(return_to, "/works");
    }

    #[tokio::test]
    async fn enterprise_links_an_existing_provisioned_user() {
        let directory = FakeDirectory::default();
        directory
            .by_email
            .lock()
            .unwrap()
            .insert("ada@corp.com".to_owned(), "U-7".to_owned());
        let svc = service(claims("ada@corp.com", true), directory);
        svc.begin_login("enterprise", "/works").await.unwrap();
        let (principal, _) = svc
            .complete_login("enterprise", "st", "code")
            .await
            .unwrap();
        assert_eq!(principal.as_str(), "U-7");
    }

    #[tokio::test]
    async fn google_refuses_to_silently_link_an_existing_account() {
        let directory = FakeDirectory::default();
        directory
            .by_email
            .lock()
            .unwrap()
            .insert("ada@corp.com".to_owned(), "U-7".to_owned());
        let svc = service(claims("ada@corp.com", true), directory);
        svc.begin_login("google", "/works").await.unwrap();
        assert_eq!(
            svc.complete_login("google", "st", "code")
                .await
                .unwrap_err(),
            AuthError::AccountExists
        );
    }

    #[tokio::test]
    async fn unverified_email_is_rejected() {
        let svc = service(claims("ada@corp.com", false), FakeDirectory::default());
        svc.begin_login("enterprise", "/works").await.unwrap();
        assert!(matches!(
            svc.complete_login("enterprise", "st", "code")
                .await
                .unwrap_err(),
            AuthError::FederationFailed(_)
        ));
    }

    #[tokio::test]
    async fn unknown_state_is_rejected() {
        let svc = service(claims("ada@corp.com", true), FakeDirectory::default());
        assert!(matches!(
            svc.complete_login("enterprise", "nope", "code")
                .await
                .unwrap_err(),
            AuthError::FederationFailed(_)
        ));
    }
}
