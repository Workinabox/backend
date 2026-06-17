//! `authbox-app`: application services for the reusable identity component.
//!
//! Orchestrates authentication and account lifecycle use cases over the ports defined in
//! [`authbox_core`].

mod authentication_service;
mod federation_service;
mod invitation_service;
mod password_reset_service;

pub use authentication_service::{
    AuthenticationService, EstablishedSession, ResolvedSession, SessionConfig,
};
pub use federation_service::FederationService;
pub use invitation_service::InvitationService;
pub use password_reset_service::PasswordResetService;
