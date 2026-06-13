use std::path::PathBuf;
use std::sync::Arc;

use wiab_app::{
    AccessApplicationService, AgentApplicationService, AuthorizationService,
    BoardApplicationService, MeetingApplicationService, OrganizationApplicationService,
    PipelineApplicationService, ProjectApplicationService, RepoApplicationService,
    UserApplicationService, WorkApplicationService,
};

use crate::{
    InMemoryAgentRepository, InMemoryBoardRepository, InMemoryMeetingRepository,
    InMemoryOrganizationRepository, InMemoryPipelineRepository, InMemoryProjectRepository,
    InMemoryRepoRepository, InMemoryRoleAssignmentRepository, InMemoryUserRepository,
    InMemoryWorkRepository, Sfu,
};

#[derive(Clone)]
pub struct AppState {
    pub meeting_service: Arc<MeetingApplicationService<InMemoryMeetingRepository>>,
    pub organization_service: Arc<OrganizationApplicationService<InMemoryOrganizationRepository>>,
    pub project_service:
        Arc<ProjectApplicationService<InMemoryProjectRepository, InMemoryOrganizationRepository>>,
    pub agent_service:
        Arc<AgentApplicationService<InMemoryAgentRepository, InMemoryOrganizationRepository>>,
    pub board_service:
        Arc<BoardApplicationService<InMemoryBoardRepository, InMemoryProjectRepository>>,
    pub repo_service:
        Arc<RepoApplicationService<InMemoryRepoRepository, InMemoryProjectRepository>>,
    pub user_service: Arc<UserApplicationService<InMemoryUserRepository>>,
    pub access_service:
        Arc<AccessApplicationService<InMemoryRoleAssignmentRepository, InMemoryUserRepository>>,
    pub authorization_service: Arc<
        AuthorizationService<
            InMemoryRoleAssignmentRepository,
            InMemoryRepoRepository,
            InMemoryProjectRepository,
        >,
    >,
    pub pipeline_service:
        Arc<PipelineApplicationService<InMemoryPipelineRepository, InMemoryProjectRepository>>,
    pub work_service:
        Arc<WorkApplicationService<InMemoryWorkRepository, InMemoryProjectRepository>>,
    pub sfu: Arc<Sfu>,
    /// Filesystem root under which hosted bare git repos live (`<root>/R-<n>.git`).
    /// Used by the Smart-HTTP handlers to locate the repo to serve.
    pub git_root: PathBuf,
    /// Version of the running backend, reported by `/health`.
    pub version: &'static str,
}
