use std::path::PathBuf;
use std::sync::Arc;

use wiab_app::{
    AccessApplicationService, AgentApplicationService, AuthorizationService,
    BoardApplicationService, MeetingApplicationService, OrganizationApplicationService,
    PipelineApplicationService, ProjectApplicationService, RepoApplicationService,
    UserApplicationService, WorkApplicationService,
};

use crate::{
    AgentRepo, BoardRepo, InMemoryMeetingRepository, OrganizationRepo, PipelineRepo, ProjectRepo,
    RepoRepo, RoleAssignmentRepo, Sfu, UserRepo, WorkRepo,
};

#[derive(Clone)]
pub struct AppState {
    pub meeting_service: Arc<MeetingApplicationService<InMemoryMeetingRepository>>,
    pub organization_service: Arc<OrganizationApplicationService<OrganizationRepo>>,
    pub project_service: Arc<ProjectApplicationService<ProjectRepo, OrganizationRepo>>,
    pub agent_service: Arc<AgentApplicationService<AgentRepo, OrganizationRepo>>,
    pub board_service: Arc<BoardApplicationService<BoardRepo, ProjectRepo>>,
    pub repo_service: Arc<RepoApplicationService<RepoRepo, ProjectRepo>>,
    pub user_service: Arc<UserApplicationService<UserRepo>>,
    pub access_service: Arc<AccessApplicationService<RoleAssignmentRepo, UserRepo>>,
    pub authorization_service: Arc<AuthorizationService<RoleAssignmentRepo, RepoRepo, ProjectRepo>>,
    pub pipeline_service: Arc<PipelineApplicationService<PipelineRepo, ProjectRepo>>,
    pub work_service: Arc<WorkApplicationService<WorkRepo, ProjectRepo>>,
    pub sfu: Arc<Sfu>,
    /// Filesystem root under which hosted bare git repos live (`<root>/R-<n>.git`).
    /// Used by the Smart-HTTP handlers to locate the repo to serve.
    pub git_root: PathBuf,
    /// Version of the running backend, reported by `/health`.
    pub version: &'static str,
}
