//! `authbox-core`: product-neutral identity & access primitives ("identity in a box").
//!
//! This crate holds the generic domain that any product can reuse: a role/operation
//! ladder and a hierarchy-agnostic RBAC policy. Products (e.g. WIAB) layer their own
//! resource model on top by implementing [`rbac::ResourceHierarchy`] and mapping their
//! scopes onto [`rbac::ResourceRef`]. Nothing here depends on any product's types.

pub mod auth;
pub mod credential;
pub mod rbac;

pub use auth::{
    AuthError, AuthFlow, AuthFlowStore, AuthRequest, Clock, CredentialStore, EmailSender,
    FederatedIdentity, FederatedIdentityStore, FederationConnection, OidcPort, PasswordCredential,
    PasswordHasher, PasswordState, PrincipalId, SecretGenerator, Session, SessionId, SessionStore,
    UserDirectory, VerificationPurpose, VerificationToken, VerificationTokenStore, VerifiedClaims,
};
pub use credential::{GeneratedToken, KeyFingerprinter, TokenFactory, TokenHasher};
pub use rbac::{Grant, Operation, ResourceHierarchy, ResourceRef, Role, RoleError, effective_role};
