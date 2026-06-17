use std::path::PathBuf;
use std::sync::Arc;

use authbox_app::{
    AuthenticationService, FederationService, InvitationService, PasswordResetService,
};
use authbox_inf::{
    AuthFlowStoreImpl, CredentialStoreImpl, FederatedIdentityStoreImpl, OidcRelyingParty,
    SessionStoreImpl, VerificationTokenStoreImpl,
};
use wiab_app::{
    AccessApplicationService, AgentApplicationService, AuthorizationService,
    BoardApplicationService, MeetingApplicationService, OrganizationApplicationService,
    PipelineApplicationService, ProjectApplicationService, RepoApplicationService,
    UserApplicationService, WorkApplicationService,
};

use crate::{
    AgentRepo, BoardRepo, InMemoryMeetingRepository, OrganizationRepo, PipelineRepo, ProjectRepo,
    RepoRepo, RoleAssignmentRepo, Sfu, UserRepo, WiabUserDirectory, WorkRepo,
};

/// The fully-resolved auth service type once WIAB picks its stores and user directory.
pub type WiabAuthService =
    AuthenticationService<SessionStoreImpl, CredentialStoreImpl, WiabUserDirectory>;

/// The fully-resolved inbound-OIDC federation service type (Google + enterprise).
pub type WiabFederationService = FederationService<
    FederatedIdentityStoreImpl,
    AuthFlowStoreImpl,
    WiabUserDirectory,
    OidcRelyingParty,
>;

/// The fully-resolved forgotten-password reset service type.
pub type WiabPasswordResetService = PasswordResetService<
    WiabUserDirectory,
    VerificationTokenStoreImpl,
    CredentialStoreImpl,
    SessionStoreImpl,
>;

/// The fully-resolved invite / signup-email-verification service type.
pub type WiabInvitationService = InvitationService<VerificationTokenStoreImpl, CredentialStoreImpl>;

/// HTTP-facing auth configuration, derived from env at boot.
#[derive(Clone)]
pub struct AuthSettings {
    /// Public origin (e.g. `https://wiab.example.com`), used to build redirect/email URLs.
    pub base_url: String,
    /// Whether the session cookie carries `Secure` (true when `base_url` is https).
    pub cookie_secure: bool,
    /// Whether open self-service signup is enabled (off by default for a single-company box).
    pub signup_enabled: bool,
    /// Whether "Continue with Google" is offered.
    pub google_enabled: bool,
    /// Whether enterprise SSO (inbound OIDC) is offered.
    pub oidc_enabled: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub meeting_service: Arc<MeetingApplicationService<InMemoryMeetingRepository>>,
    pub organization_service: Arc<OrganizationApplicationService<OrganizationRepo>>,
    pub project_service: Arc<ProjectApplicationService<ProjectRepo, OrganizationRepo>>,
    pub agent_service: Arc<AgentApplicationService<AgentRepo, OrganizationRepo>>,
    pub board_service: Arc<BoardApplicationService<BoardRepo, ProjectRepo>>,
    pub repo_service: Arc<RepoApplicationService<RepoRepo, ProjectRepo>>,
    pub user_service: Arc<UserApplicationService<UserRepo>>,
    /// Reusable authentication (password login + browser sessions), keyed on the user id.
    pub auth_service: Arc<WiabAuthService>,
    /// Inbound OIDC federation (Google / enterprise SSO). `None` when no provider is enabled.
    pub federation_service: Option<Arc<WiabFederationService>>,
    /// Forgotten-password reset (email a single-use link).
    pub password_reset_service: Arc<WiabPasswordResetService>,
    /// Admin invites and signup email-verification (email a single-use activation link).
    pub invitation_service: Arc<WiabInvitationService>,
    pub access_service: Arc<AccessApplicationService<RoleAssignmentRepo, UserRepo>>,
    pub authorization_service: Arc<AuthorizationService<RoleAssignmentRepo, RepoRepo, ProjectRepo>>,
    pub pipeline_service: Arc<PipelineApplicationService<PipelineRepo, ProjectRepo>>,
    pub work_service: Arc<WorkApplicationService<WorkRepo, ProjectRepo>>,
    pub sfu: Arc<Sfu>,
    /// HTTP auth configuration (cookie flags, enabled login methods).
    pub auth_settings: AuthSettings,
    /// Filesystem root under which hosted bare git repos live (`<root>/R-<n>.git`).
    /// Used by the Smart-HTTP handlers to locate the repo to serve.
    pub git_root: PathBuf,
    /// Version of the running backend, reported by `/health`.
    pub version: &'static str,
}
