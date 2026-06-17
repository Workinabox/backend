//! Reusable authentication: sessions and password credentials, plus the ports that let a
//! host plug in its user store (`UserDirectory`), persistence (`SessionStore`,
//! `CredentialStore`), and crypto/time (`PasswordHasher`, `Clock`).
//!
//! Everything is keyed on an opaque [`PrincipalId`] — the auth layer never interprets the
//! host's user id, so the same code serves any product.

mod auth_error;
mod auth_flow;
mod auth_flow_store;
mod clock;
mod credential_store;
mod email_sender;
mod federated_identity;
mod federated_identity_store;
mod federation_connection;
mod oidc_port;
mod password_credential;
mod password_hasher;
mod principal_id;
mod secret_generator;
mod session;
mod session_id;
mod session_store;
mod user_directory;
mod verification_token;
mod verification_token_store;
mod verified_claims;

pub use auth_error::AuthError;
pub use auth_flow::AuthFlow;
pub use auth_flow_store::AuthFlowStore;
pub use clock::Clock;
pub use credential_store::CredentialStore;
pub use email_sender::EmailSender;
pub use federated_identity::FederatedIdentity;
pub use federated_identity_store::FederatedIdentityStore;
pub use federation_connection::FederationConnection;
pub use oidc_port::{AuthRequest, OidcPort};
pub use password_credential::{PasswordCredential, PasswordState};
pub use password_hasher::PasswordHasher;
pub use principal_id::PrincipalId;
pub use secret_generator::SecretGenerator;
pub use session::Session;
pub use session_id::SessionId;
pub use session_store::SessionStore;
pub use user_directory::UserDirectory;
pub use verification_token::{VerificationPurpose, VerificationToken};
pub use verification_token_store::VerificationTokenStore;
pub use verified_claims::VerifiedClaims;
