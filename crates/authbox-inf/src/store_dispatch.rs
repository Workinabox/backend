//! Enum-dispatch wrappers so the host can pick a backend at startup and hold a concrete
//! store type (no `dyn`), mirroring WIAB's `repository_dispatch`.

use authbox_core::auth::{
    AuthError, AuthFlow, AuthFlowStore, CredentialStore, FederatedIdentity, FederatedIdentityStore,
    PasswordCredential, PrincipalId, Session, SessionStore, VerificationToken,
    VerificationTokenStore,
};

use crate::{
    InMemoryAuthFlowStore, InMemoryCredentialStore, InMemoryFederatedIdentityStore,
    InMemorySessionStore, InMemoryVerificationTokenStore, PostgresAuthFlowStore,
    PostgresCredentialStore, PostgresFederatedIdentityStore, PostgresSessionStore,
    PostgresVerificationTokenStore,
};

#[derive(Clone)]
pub enum SessionStoreImpl {
    InMemory(InMemorySessionStore),
    Postgres(PostgresSessionStore),
}

impl SessionStore for SessionStoreImpl {
    async fn put(&self, session: Session) -> Result<(), AuthError> {
        match self {
            Self::InMemory(store) => store.put(session).await,
            Self::Postgres(store) => store.put(session).await,
        }
    }

    async fn find_by_token_hash(&self, token_hash: &str) -> Result<Option<Session>, AuthError> {
        match self {
            Self::InMemory(store) => store.find_by_token_hash(token_hash).await,
            Self::Postgres(store) => store.find_by_token_hash(token_hash).await,
        }
    }

    async fn revoke_all_for_principal(&self, principal: &PrincipalId) -> Result<(), AuthError> {
        match self {
            Self::InMemory(store) => store.revoke_all_for_principal(principal).await,
            Self::Postgres(store) => store.revoke_all_for_principal(principal).await,
        }
    }
}

#[derive(Clone)]
pub enum CredentialStoreImpl {
    InMemory(InMemoryCredentialStore),
    Postgres(PostgresCredentialStore),
}

impl CredentialStore for CredentialStoreImpl {
    async fn find_password(
        &self,
        principal: &PrincipalId,
    ) -> Result<Option<PasswordCredential>, AuthError> {
        match self {
            Self::InMemory(store) => store.find_password(principal).await,
            Self::Postgres(store) => store.find_password(principal).await,
        }
    }

    async fn save_password(&self, credential: PasswordCredential) -> Result<(), AuthError> {
        match self {
            Self::InMemory(store) => store.save_password(credential).await,
            Self::Postgres(store) => store.save_password(credential).await,
        }
    }
}

#[derive(Clone)]
pub enum FederatedIdentityStoreImpl {
    InMemory(InMemoryFederatedIdentityStore),
    Postgres(PostgresFederatedIdentityStore),
}

impl FederatedIdentityStore for FederatedIdentityStoreImpl {
    async fn find(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<FederatedIdentity>, AuthError> {
        match self {
            Self::InMemory(store) => store.find(issuer, subject).await,
            Self::Postgres(store) => store.find(issuer, subject).await,
        }
    }

    async fn link(&self, identity: FederatedIdentity) -> Result<(), AuthError> {
        match self {
            Self::InMemory(store) => store.link(identity).await,
            Self::Postgres(store) => store.link(identity).await,
        }
    }
}

#[derive(Clone)]
pub enum AuthFlowStoreImpl {
    InMemory(InMemoryAuthFlowStore),
    Postgres(PostgresAuthFlowStore),
}

impl AuthFlowStore for AuthFlowStoreImpl {
    async fn put(&self, flow: AuthFlow) -> Result<(), AuthError> {
        match self {
            Self::InMemory(store) => store.put(flow).await,
            Self::Postgres(store) => store.put(flow).await,
        }
    }

    async fn take(&self, state: &str) -> Result<Option<AuthFlow>, AuthError> {
        match self {
            Self::InMemory(store) => store.take(state).await,
            Self::Postgres(store) => store.take(state).await,
        }
    }
}

#[derive(Clone)]
pub enum VerificationTokenStoreImpl {
    InMemory(InMemoryVerificationTokenStore),
    Postgres(PostgresVerificationTokenStore),
}

impl VerificationTokenStore for VerificationTokenStoreImpl {
    async fn put(&self, token: VerificationToken) -> Result<(), AuthError> {
        match self {
            Self::InMemory(store) => store.put(token).await,
            Self::Postgres(store) => store.put(token).await,
        }
    }

    async fn consume(&self, token_hash: &str) -> Result<Option<VerificationToken>, AuthError> {
        match self {
            Self::InMemory(store) => store.consume(token_hash).await,
            Self::Postgres(store) => store.consume(token_hash).await,
        }
    }
}
