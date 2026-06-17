//! `authbox-inf`: infrastructure adapters for the reusable identity component.
//!
//! Concrete impls of the [`authbox_core`] ports — argon2id password hashing, CSPRNG secret
//! generation, OIDC relying-party, email sending (logging + SMTP), and persistence
//! (in-memory + Postgres) for sessions, password credentials, federated identities, OIDC
//! login state, and verification tokens — plus the auth schema migrations. The HTTP layer
//! lives in the host app, which mounts these behind its routes.

mod argon2_password_hasher;
mod in_memory_auth_flow_store;
mod in_memory_credential_store;
mod in_memory_federated_identity_store;
mod in_memory_session_store;
mod in_memory_verification_token_store;
mod logging_email_sender;
mod migrations;
mod oidc_relying_party;
mod postgres_auth_flow_store;
mod postgres_credential_store;
mod postgres_federated_identity_store;
mod postgres_session_store;
mod postgres_verification_token_store;
mod random_secret_generator;
mod smtp_email_sender;
mod store_dispatch;

pub use argon2_password_hasher::Argon2idPasswordHasher;
pub use in_memory_auth_flow_store::InMemoryAuthFlowStore;
pub use in_memory_credential_store::InMemoryCredentialStore;
pub use in_memory_federated_identity_store::InMemoryFederatedIdentityStore;
pub use in_memory_session_store::InMemorySessionStore;
pub use in_memory_verification_token_store::InMemoryVerificationTokenStore;
pub use logging_email_sender::LoggingEmailSender;
pub use migrations::run_migrations;
pub use oidc_relying_party::OidcRelyingParty;
pub use postgres_auth_flow_store::PostgresAuthFlowStore;
pub use postgres_credential_store::PostgresCredentialStore;
pub use postgres_federated_identity_store::PostgresFederatedIdentityStore;
pub use postgres_session_store::PostgresSessionStore;
pub use postgres_verification_token_store::PostgresVerificationTokenStore;
pub use random_secret_generator::RandomSecretGenerator;
pub use smtp_email_sender::SmtpEmailSender;
pub use store_dispatch::{
    AuthFlowStoreImpl, CredentialStoreImpl, FederatedIdentityStoreImpl, SessionStoreImpl,
    VerificationTokenStoreImpl,
};
