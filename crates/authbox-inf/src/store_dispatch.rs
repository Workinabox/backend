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
            Self::InMemory(store) => {
                wiab_telemetry::timed_db("session", "put", "memory", store.put(session)).await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db("session", "put", "postgres", store.put(session)).await
            }
        }
    }

    async fn find_by_token_hash(&self, token_hash: &str) -> Result<Option<Session>, AuthError> {
        match self {
            Self::InMemory(store) => {
                wiab_telemetry::timed_db(
                    "session",
                    "find_by_token_hash",
                    "memory",
                    store.find_by_token_hash(token_hash),
                )
                .await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db(
                    "session",
                    "find_by_token_hash",
                    "postgres",
                    store.find_by_token_hash(token_hash),
                )
                .await
            }
        }
    }

    async fn revoke_all_for_principal(&self, principal: &PrincipalId) -> Result<(), AuthError> {
        match self {
            Self::InMemory(store) => {
                wiab_telemetry::timed_db(
                    "session",
                    "revoke_all_for_principal",
                    "memory",
                    store.revoke_all_for_principal(principal),
                )
                .await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db(
                    "session",
                    "revoke_all_for_principal",
                    "postgres",
                    store.revoke_all_for_principal(principal),
                )
                .await
            }
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
            Self::InMemory(store) => {
                wiab_telemetry::timed_db(
                    "credential",
                    "find_password",
                    "memory",
                    store.find_password(principal),
                )
                .await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db(
                    "credential",
                    "find_password",
                    "postgres",
                    store.find_password(principal),
                )
                .await
            }
        }
    }

    async fn save_password(&self, credential: PasswordCredential) -> Result<(), AuthError> {
        match self {
            Self::InMemory(store) => {
                wiab_telemetry::timed_db(
                    "credential",
                    "save_password",
                    "memory",
                    store.save_password(credential),
                )
                .await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db(
                    "credential",
                    "save_password",
                    "postgres",
                    store.save_password(credential),
                )
                .await
            }
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
            Self::InMemory(store) => {
                wiab_telemetry::timed_db(
                    "federated_identity",
                    "find",
                    "memory",
                    store.find(issuer, subject),
                )
                .await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db(
                    "federated_identity",
                    "find",
                    "postgres",
                    store.find(issuer, subject),
                )
                .await
            }
        }
    }

    async fn link(&self, identity: FederatedIdentity) -> Result<(), AuthError> {
        match self {
            Self::InMemory(store) => {
                wiab_telemetry::timed_db(
                    "federated_identity",
                    "link",
                    "memory",
                    store.link(identity),
                )
                .await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db(
                    "federated_identity",
                    "link",
                    "postgres",
                    store.link(identity),
                )
                .await
            }
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
            Self::InMemory(store) => {
                wiab_telemetry::timed_db("auth_flow", "put", "memory", store.put(flow)).await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db("auth_flow", "put", "postgres", store.put(flow)).await
            }
        }
    }

    async fn take(&self, state: &str) -> Result<Option<AuthFlow>, AuthError> {
        match self {
            Self::InMemory(store) => {
                wiab_telemetry::timed_db("auth_flow", "take", "memory", store.take(state)).await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db("auth_flow", "take", "postgres", store.take(state)).await
            }
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
            Self::InMemory(store) => {
                wiab_telemetry::timed_db("verification_token", "put", "memory", store.put(token))
                    .await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db("verification_token", "put", "postgres", store.put(token))
                    .await
            }
        }
    }

    async fn consume(&self, token_hash: &str) -> Result<Option<VerificationToken>, AuthError> {
        match self {
            Self::InMemory(store) => {
                wiab_telemetry::timed_db(
                    "verification_token",
                    "consume",
                    "memory",
                    store.consume(token_hash),
                )
                .await
            }
            Self::Postgres(store) => {
                wiab_telemetry::timed_db(
                    "verification_token",
                    "consume",
                    "postgres",
                    store.consume(token_hash),
                )
                .await
            }
        }
    }
}
