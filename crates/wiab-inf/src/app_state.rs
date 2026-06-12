use std::sync::Arc;

use wiab_app::{
    AgentApplicationService, BoardApplicationService, MeetingApplicationService,
    OrganizationApplicationService, PipelineApplicationService, ProjectApplicationService,
    RepoApplicationService, WorkApplicationService,
};

use crate::{
    InMemoryAgentRepository, InMemoryBoardRepository, InMemoryMeetingRepository,
    InMemoryOrganizationRepository, InMemoryPipelineRepository, InMemoryProjectRepository,
    InMemoryRepoRepository, InMemoryWorkRepository, Sfu,
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
    pub pipeline_service:
        Arc<PipelineApplicationService<InMemoryPipelineRepository, InMemoryProjectRepository>>,
    pub work_service:
        Arc<WorkApplicationService<InMemoryWorkRepository, InMemoryProjectRepository>>,
    pub sfu: Arc<Sfu>,
    /// Version of the running backend, reported by `/health`.
    pub version: &'static str,
}
