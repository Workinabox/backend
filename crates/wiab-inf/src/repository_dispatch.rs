//! Enum-dispatch repository wrappers.
//!
//! Each aggregate (except `Meeting`, which is always in-memory) gets an enum with an
//! `InMemory` and a `Postgres` variant that implements the aggregate's repository trait by
//! delegating through a `match`. This lets one binary choose its persistence backend at
//! startup from config — `AppState` holds the concrete enum type, so there is no `dyn`
//! indirection and no generic ripple into the HTTP layer. Both variants are `Clone`
//! (the in-memory store shares an `Arc`; the Postgres repo shares its connection `Pool`),
//! so a single repo can be shared across the services that need it.

use wiab_core::access::{RoleAssignment, RoleAssignmentId, RoleAssignmentRepository};
use wiab_core::agent::{Agent, AgentId, AgentRepository};
use wiab_core::board::{Board, BoardId, BoardRepository};
use wiab_core::organization::{Organization, OrganizationId, OrganizationRepository};
use wiab_core::pipeline::{Pipeline, PipelineId, PipelineRepository};
use wiab_core::project::{Project, ProjectId, ProjectRepository};
use wiab_core::pull_request::{PullRequest, PullRequestId, PullRequestRepository};
use wiab_core::repo::{Repo, RepoId, RepoRepository};
use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::task::{Task, TaskId, TaskRepository};
use wiab_core::team::{Team, TeamId, TeamRepository};
use wiab_core::user::{User, UserId, UserRepository};
use wiab_core::vm::{Vm, VmId, VmRepository};
use wiab_core::work::{Work, WorkId, WorkRepository};

use crate::{
    InMemoryAgentRepository, InMemoryBoardRepository, InMemoryOrganizationRepository,
    InMemoryPipelineRepository, InMemoryProjectRepository, InMemoryPullRequestRepository,
    InMemoryRepoRepository, InMemoryRoleAssignmentRepository, InMemoryTaskRepository,
    InMemoryTeamRepository, InMemoryUserRepository, InMemoryVmRepository, InMemoryWorkRepository,
    PostgresAgentRepository, PostgresBoardRepository, PostgresOrganizationRepository,
    PostgresPipelineRepository, PostgresProjectRepository, PostgresPullRequestRepository,
    PostgresRepoRepository, PostgresRoleAssignmentRepository, PostgresTaskRepository,
    PostgresTeamRepository, PostgresUserRepository, PostgresVmRepository, PostgresWorkRepository,
};

#[derive(Clone)]
pub enum OrganizationRepo {
    InMemory(InMemoryOrganizationRepository),
    Postgres(PostgresOrganizationRepository),
}

impl OrganizationRepository for OrganizationRepo {
    async fn save(
        &self,
        organization: Organization,
        expected: Version,
    ) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(repo) => repo.save(organization, expected).await,
            Self::Postgres(repo) => repo.save(organization, expected).await,
        }
    }

    async fn get(&self, id: &OrganizationId) -> Result<Option<(Organization, Version)>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.get(id).await,
            Self::Postgres(repo) => repo.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<Organization>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.list().await,
            Self::Postgres(repo) => repo.list().await,
        }
    }
}

#[derive(Clone)]
pub enum ProjectRepo {
    InMemory(InMemoryProjectRepository),
    Postgres(PostgresProjectRepository),
}

impl ProjectRepository for ProjectRepo {
    async fn save(&self, project: Project, expected: Version) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(repo) => repo.save(project, expected).await,
            Self::Postgres(repo) => repo.save(project, expected).await,
        }
    }

    async fn get(&self, id: &ProjectId) -> Result<Option<(Project, Version)>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.get(id).await,
            Self::Postgres(repo) => repo.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<Project>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.list().await,
            Self::Postgres(repo) => repo.list().await,
        }
    }
}

#[derive(Clone)]
pub enum AgentRepo {
    InMemory(InMemoryAgentRepository),
    Postgres(PostgresAgentRepository),
}

impl AgentRepository for AgentRepo {
    async fn save(&self, agent: Agent, expected: Version) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(repo) => repo.save(agent, expected).await,
            Self::Postgres(repo) => repo.save(agent, expected).await,
        }
    }

    async fn get(&self, id: &AgentId) -> Result<Option<(Agent, Version)>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.get(id).await,
            Self::Postgres(repo) => repo.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<Agent>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.list().await,
            Self::Postgres(repo) => repo.list().await,
        }
    }
}

#[derive(Clone)]
pub enum BoardRepo {
    InMemory(InMemoryBoardRepository),
    Postgres(PostgresBoardRepository),
}

impl BoardRepository for BoardRepo {
    async fn save(&self, board: Board, expected: Version) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(repo) => repo.save(board, expected).await,
            Self::Postgres(repo) => repo.save(board, expected).await,
        }
    }

    async fn get(&self, id: &BoardId) -> Result<Option<(Board, Version)>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.get(id).await,
            Self::Postgres(repo) => repo.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<Board>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.list().await,
            Self::Postgres(repo) => repo.list().await,
        }
    }
}

#[derive(Clone)]
pub enum RepoRepo {
    InMemory(InMemoryRepoRepository),
    Postgres(PostgresRepoRepository),
}

impl RepoRepository for RepoRepo {
    async fn save(&self, repo: Repo, expected: Version) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(inner) => inner.save(repo, expected).await,
            Self::Postgres(inner) => inner.save(repo, expected).await,
        }
    }

    async fn get(&self, id: &RepoId) -> Result<Option<(Repo, Version)>, RepoError> {
        match self {
            Self::InMemory(inner) => inner.get(id).await,
            Self::Postgres(inner) => inner.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<Repo>, RepoError> {
        match self {
            Self::InMemory(inner) => inner.list().await,
            Self::Postgres(inner) => inner.list().await,
        }
    }
}

#[derive(Clone)]
pub enum PullRequestRepo {
    InMemory(InMemoryPullRequestRepository),
    Postgres(PostgresPullRequestRepository),
}

impl PullRequestRepository for PullRequestRepo {
    async fn save(
        &self,
        pull_request: PullRequest,
        expected: Version,
    ) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(inner) => inner.save(pull_request, expected).await,
            Self::Postgres(inner) => inner.save(pull_request, expected).await,
        }
    }

    async fn get(&self, id: &PullRequestId) -> Result<Option<(PullRequest, Version)>, RepoError> {
        match self {
            Self::InMemory(inner) => inner.get(id).await,
            Self::Postgres(inner) => inner.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<PullRequest>, RepoError> {
        match self {
            Self::InMemory(inner) => inner.list().await,
            Self::Postgres(inner) => inner.list().await,
        }
    }
}

#[derive(Clone)]
pub enum PipelineRepo {
    InMemory(InMemoryPipelineRepository),
    Postgres(PostgresPipelineRepository),
}

impl PipelineRepository for PipelineRepo {
    async fn save(&self, pipeline: Pipeline, expected: Version) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(repo) => repo.save(pipeline, expected).await,
            Self::Postgres(repo) => repo.save(pipeline, expected).await,
        }
    }

    async fn get(&self, id: &PipelineId) -> Result<Option<(Pipeline, Version)>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.get(id).await,
            Self::Postgres(repo) => repo.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<Pipeline>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.list().await,
            Self::Postgres(repo) => repo.list().await,
        }
    }
}

#[derive(Clone)]
pub enum WorkRepo {
    InMemory(InMemoryWorkRepository),
    Postgres(PostgresWorkRepository),
}

impl WorkRepository for WorkRepo {
    async fn save(&self, work: Work, expected: Version) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(repo) => repo.save(work, expected).await,
            Self::Postgres(repo) => repo.save(work, expected).await,
        }
    }

    async fn get(&self, id: &WorkId) -> Result<Option<(Work, Version)>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.get(id).await,
            Self::Postgres(repo) => repo.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<Work>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.list().await,
            Self::Postgres(repo) => repo.list().await,
        }
    }
}

#[derive(Clone)]
pub enum UserRepo {
    InMemory(InMemoryUserRepository),
    Postgres(PostgresUserRepository),
}

impl UserRepository for UserRepo {
    async fn save(&self, user: User, expected: Version) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(repo) => repo.save(user, expected).await,
            Self::Postgres(repo) => repo.save(user, expected).await,
        }
    }

    async fn get(&self, id: &UserId) -> Result<Option<(User, Version)>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.get(id).await,
            Self::Postgres(repo) => repo.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<User>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.list().await,
            Self::Postgres(repo) => repo.list().await,
        }
    }

    async fn find_id_by_token_hash(&self, hash: &str) -> Result<Option<UserId>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.find_id_by_token_hash(hash).await,
            Self::Postgres(repo) => repo.find_id_by_token_hash(hash).await,
        }
    }

    async fn find_id_by_ssh_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<Option<UserId>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.find_id_by_ssh_fingerprint(fingerprint).await,
            Self::Postgres(repo) => repo.find_id_by_ssh_fingerprint(fingerprint).await,
        }
    }
}

#[derive(Clone)]
pub enum VmRepo {
    InMemory(InMemoryVmRepository),
    Postgres(PostgresVmRepository),
}

impl VmRepository for VmRepo {
    async fn save(&self, vm: Vm, expected: Version) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(repo) => repo.save(vm, expected).await,
            Self::Postgres(repo) => repo.save(vm, expected).await,
        }
    }

    async fn get(&self, id: &VmId) -> Result<Option<(Vm, Version)>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.get(id).await,
            Self::Postgres(repo) => repo.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<Vm>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.list().await,
            Self::Postgres(repo) => repo.list().await,
        }
    }
}

#[derive(Clone)]
pub enum RoleAssignmentRepo {
    InMemory(InMemoryRoleAssignmentRepository),
    Postgres(PostgresRoleAssignmentRepository),
}

impl RoleAssignmentRepository for RoleAssignmentRepo {
    async fn save(
        &self,
        assignment: RoleAssignment,
        expected: Version,
    ) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(repo) => repo.save(assignment, expected).await,
            Self::Postgres(repo) => repo.save(assignment, expected).await,
        }
    }

    async fn get(
        &self,
        id: &RoleAssignmentId,
    ) -> Result<Option<(RoleAssignment, Version)>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.get(id).await,
            Self::Postgres(repo) => repo.get(id).await,
        }
    }

    async fn remove(&self, id: &RoleAssignmentId) -> Result<bool, RepoError> {
        match self {
            Self::InMemory(repo) => repo.remove(id).await,
            Self::Postgres(repo) => repo.remove(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<RoleAssignment>, RepoError> {
        match self {
            Self::InMemory(repo) => repo.list().await,
            Self::Postgres(repo) => repo.list().await,
        }
    }
}

#[derive(Clone)]
pub enum TeamRepo {
    InMemory(InMemoryTeamRepository),
    Postgres(PostgresTeamRepository),
}

impl TeamRepository for TeamRepo {
    async fn save(&self, team: Team, expected: Version) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(inner) => inner.save(team, expected).await,
            Self::Postgres(inner) => inner.save(team, expected).await,
        }
    }

    async fn get(&self, id: &TeamId) -> Result<Option<(Team, Version)>, RepoError> {
        match self {
            Self::InMemory(inner) => inner.get(id).await,
            Self::Postgres(inner) => inner.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<Team>, RepoError> {
        match self {
            Self::InMemory(inner) => inner.list().await,
            Self::Postgres(inner) => inner.list().await,
        }
    }
}

#[derive(Clone)]
pub enum TaskRepo {
    InMemory(InMemoryTaskRepository),
    Postgres(PostgresTaskRepository),
}

impl TaskRepository for TaskRepo {
    async fn save(&self, task: Task, expected: Version) -> Result<Version, SaveError> {
        match self {
            Self::InMemory(inner) => inner.save(task, expected).await,
            Self::Postgres(inner) => inner.save(task, expected).await,
        }
    }

    async fn get(&self, id: &TaskId) -> Result<Option<(Task, Version)>, RepoError> {
        match self {
            Self::InMemory(inner) => inner.get(id).await,
            Self::Postgres(inner) => inner.get(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<Task>, RepoError> {
        match self {
            Self::InMemory(inner) => inner.list().await,
            Self::Postgres(inner) => inner.list().await,
        }
    }

    async fn claim_next(
        &self,
        board_id: &BoardId,
        team_id: TeamId,
    ) -> Result<Option<(Task, Version)>, RepoError> {
        match self {
            Self::InMemory(inner) => inner.claim_next(board_id, team_id).await,
            Self::Postgres(inner) => inner.claim_next(board_id, team_id).await,
        }
    }
}
