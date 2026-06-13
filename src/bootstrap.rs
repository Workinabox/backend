use std::sync::Arc;

use anyhow::Context;
use tokio::sync::mpsc;
use tracing::info;
use wiab_app::{
    AccessApplicationService, AgentApplicationService, AuthorizationService,
    BoardApplicationService, CreateOrganizationRequest, CreateProjectRequest, CreateUserRequest,
    IssueTokenRequest, MeetingApplicationService, OrganizationApplicationService,
    PipelineApplicationService, ProjectApplicationService, RepoApplicationService,
    UserApplicationService, WorkApplicationService,
};
use wiab_core::{
    access::{Role, RoleAssignmentRepository, Scope},
    agent::AgentRepository,
    board::BoardRepository,
    meeting_traits::{Clock, MeetingIntelligence},
    organization::{OrganizationId, OrganizationRepository},
    pipeline::PipelineRepository,
    project::ProjectRepository,
    repo::{GitBackend, RepoRepository},
    transcript::FinalizedTranscript,
    user::{UserId, UserRepository},
    work::WorkRepository,
};
use wiab_inf::{
    AgentRepo, AppState, BoardRepo, DefaultSpeechSynthesizer, Git2Backend,
    HeuristicMeetingIntelligence, InMemoryAgentNumbering, InMemoryAgentRepository,
    InMemoryBoardNumbering, InMemoryBoardRepository, InMemoryMeetingRepository,
    InMemoryOrganizationNumbering, InMemoryOrganizationRepository, InMemoryPipelineNumbering,
    InMemoryPipelineRepository, InMemoryProjectNumbering, InMemoryProjectRepository,
    InMemoryRepoNumbering, InMemoryRepoRepository, InMemoryRoleAssignmentNumbering,
    InMemoryRoleAssignmentRepository, InMemoryUserNumbering, InMemoryUserRepository,
    InMemoryWorkNumbering, InMemoryWorkRepository, LlamaMeetingIntelligence, OrganizationRepo,
    PipelineRepo, PostgresAgentRepository, PostgresBoardRepository, PostgresOrganizationRepository,
    PostgresPipelineRepository, PostgresProjectRepository, PostgresRepoRepository,
    PostgresRoleAssignmentRepository, PostgresUserRepository, PostgresWorkRepository, ProjectRepo,
    RandomTokenFactory, RepoRepo, RoleAssignmentRepo, Sfu, Sha256KeyFingerprinter,
    Sha256TokenHasher, SystemClock, UserRepo, WorkRepo, pg_pool,
};

pub async fn build_app_state(persistence: &str, database_url: &str) -> anyhow::Result<AppState> {
    let seed_clock = SystemClock;
    let meeting_repository = InMemoryMeetingRepository::with_seed_data(|| seed_clock.now_rfc3339());
    let intelligence = load_meeting_intelligence()?;
    let meeting_service = Arc::new(MeetingApplicationService::new(
        meeting_repository.clone(),
        intelligence,
        Arc::new(DefaultSpeechSynthesizer::from_env()),
        Arc::new(SystemClock),
    ));

    log_loaded_meetings(meeting_service.as_ref()).await;

    // Choose the persistence backend from config. Meeting state is always in-memory
    // (ephemeral live sessions); every other aggregate is backed by the selected store.
    let pool = match persistence.trim().to_ascii_lowercase().as_str() {
        "memory" => {
            info!("persistence backend: in-memory (data is lost on restart)");
            None
        }
        "postgres" => {
            let pool = pg_pool::build_pool(database_url)
                .await
                .context("failed to connect to Postgres")?;
            pg_pool::run_migrations(&pool)
                .await
                .context("failed to apply database migrations")?;
            info!("persistence backend: postgres");
            Some(pool)
        }
        other => anyhow::bail!(
            "unsupported persistence value '{other}' (expected 'memory' or 'postgres')"
        ),
    };

    let organization_repo = match &pool {
        Some(pool) => OrganizationRepo::Postgres(PostgresOrganizationRepository::new(pool.clone())),
        None => OrganizationRepo::InMemory(InMemoryOrganizationRepository::new()),
    };
    let organization_numbering = InMemoryOrganizationNumbering::starting_at(next_after(
        &organization_repo.list().await?,
        |organization| organization.id().number(),
    ));
    let organization_service = Arc::new(OrganizationApplicationService::new(
        organization_repo.clone(),
        Arc::new(organization_numbering),
    ));

    let project_repo = match &pool {
        Some(pool) => ProjectRepo::Postgres(PostgresProjectRepository::new(pool.clone())),
        None => ProjectRepo::InMemory(InMemoryProjectRepository::new()),
    };
    let project_numbering =
        InMemoryProjectNumbering::starting_at(next_after(&project_repo.list().await?, |project| {
            project.id().number()
        }));
    let project_service = Arc::new(ProjectApplicationService::new(
        project_repo.clone(),
        organization_repo.clone(),
        Arc::new(project_numbering),
    ));

    let agent_repo = match &pool {
        Some(pool) => AgentRepo::Postgres(PostgresAgentRepository::new(pool.clone())),
        None => AgentRepo::InMemory(InMemoryAgentRepository::new()),
    };
    let agent_numbering =
        InMemoryAgentNumbering::starting_at(next_after(&agent_repo.list().await?, |agent| {
            agent.id().number()
        }));
    let agent_service = Arc::new(AgentApplicationService::new(
        agent_repo,
        organization_repo.clone(),
        Arc::new(agent_numbering),
    ));

    let board_repo = match &pool {
        Some(pool) => BoardRepo::Postgres(PostgresBoardRepository::new(pool.clone())),
        None => BoardRepo::InMemory(InMemoryBoardRepository::new()),
    };
    let board_numbering =
        InMemoryBoardNumbering::starting_at(next_after(&board_repo.list().await?, |board| {
            board.id().number()
        }));
    let board_service = Arc::new(BoardApplicationService::new(
        board_repo,
        project_repo.clone(),
        Arc::new(board_numbering),
    ));

    let git_root = std::env::var("WIAB_GIT_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("wiab-git"));
    std::fs::create_dir_all(&git_root)
        .with_context(|| format!("failed to create git root {}", git_root.display()))?;
    info!("hosting git repos under {}", git_root.display());
    let git_backend: Arc<dyn GitBackend> = Arc::new(Git2Backend::new(git_root.clone()));

    let repo_repo = match &pool {
        Some(pool) => RepoRepo::Postgres(PostgresRepoRepository::new(pool.clone())),
        None => RepoRepo::InMemory(InMemoryRepoRepository::new()),
    };
    let repo_numbering =
        InMemoryRepoNumbering::starting_at(next_after(&repo_repo.list().await?, |repo| {
            repo.id().number()
        }));
    let repo_service = Arc::new(RepoApplicationService::new(
        repo_repo.clone(),
        project_repo.clone(),
        Arc::new(repo_numbering),
        git_backend,
    ));

    // Identity, credentials, and access control.
    let user_repo = match &pool {
        Some(pool) => UserRepo::Postgres(PostgresUserRepository::new(pool.clone())),
        None => UserRepo::InMemory(InMemoryUserRepository::new()),
    };
    let user_numbering =
        InMemoryUserNumbering::starting_at(next_after(&user_repo.list().await?, |user| {
            user.id().number()
        }));
    let user_service = Arc::new(UserApplicationService::new(
        user_repo.clone(),
        Arc::new(user_numbering),
        Arc::new(RandomTokenFactory),
        Arc::new(Sha256TokenHasher),
        Arc::new(Sha256KeyFingerprinter),
        Arc::new(SystemClock),
    ));

    let assignment_repo = match &pool {
        Some(pool) => {
            RoleAssignmentRepo::Postgres(PostgresRoleAssignmentRepository::new(pool.clone()))
        }
        None => RoleAssignmentRepo::InMemory(InMemoryRoleAssignmentRepository::new()),
    };
    let assignment_numbering = InMemoryRoleAssignmentNumbering::starting_at(next_after(
        &assignment_repo.list().await?,
        |assignment| assignment.id().number(),
    ));
    let access_service = Arc::new(AccessApplicationService::new(
        assignment_repo.clone(),
        user_repo.clone(),
        Arc::new(assignment_numbering),
    ));
    let authorization_service = Arc::new(AuthorizationService::new(
        assignment_repo.clone(),
        repo_repo.clone(),
        project_repo.clone(),
    ));

    // Seed the default org + owner only when the store is empty, so a Postgres-backed
    // restart does not try to re-create them (which would fail the unique-id insert).
    if organization_service.list_organizations().await?.is_empty() {
        seed_default_organization(organization_service.as_ref(), project_service.as_ref()).await?;
        seed_owner(user_service.as_ref(), access_service.as_ref()).await;
    }

    let pipeline_repo = match &pool {
        Some(pool) => PipelineRepo::Postgres(PostgresPipelineRepository::new(pool.clone())),
        None => PipelineRepo::InMemory(InMemoryPipelineRepository::new()),
    };
    let pipeline_numbering = InMemoryPipelineNumbering::starting_at(next_after(
        &pipeline_repo.list().await?,
        |pipeline| pipeline.id().number(),
    ));
    let pipeline_service = Arc::new(PipelineApplicationService::new(
        pipeline_repo,
        project_repo.clone(),
        Arc::new(pipeline_numbering),
    ));

    let work_repo = match &pool {
        Some(pool) => WorkRepo::Postgres(PostgresWorkRepository::new(pool.clone())),
        None => WorkRepo::InMemory(InMemoryWorkRepository::new()),
    };
    let work_numbering =
        InMemoryWorkNumbering::starting_at(next_after(&work_repo.list().await?, |work| {
            work.id().number()
        }));
    let work_service = Arc::new(WorkApplicationService::new(
        work_repo,
        project_repo.clone(),
        Arc::new(work_numbering),
    ));

    let (transcript_tx, transcript_rx) = mpsc::unbounded_channel::<FinalizedTranscript>();
    let sfu = Arc::new(
        Sfu::new(meeting_service.clone(), transcript_tx)
            .await
            .context("failed to initialize SFU")?,
    );
    spawn_transcript_runtime(sfu.clone(), transcript_rx);

    Ok(AppState {
        meeting_service,
        organization_service,
        project_service,
        agent_service,
        board_service,
        repo_service,
        user_service,
        access_service,
        authorization_service,
        pipeline_service,
        work_service,
        sfu,
        git_root,
        // Release builds inject WIAB_VERSION (the git tag) so the reported
        // version matches the release; local builds fall back to Cargo.toml.
        version: option_env!("WIAB_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
    })
}

/// Highest id number already present, so sequential numbering resumes at `n + 1` after a
/// restart instead of colliding with persisted ids. Returns 0 for an empty store.
fn next_after<T>(items: &[T], number: impl Fn(&T) -> u64) -> u64 {
    items.iter().map(number).max().unwrap_or(0)
}

async fn seed_default_organization(
    organization_service: &OrganizationApplicationService<OrganizationRepo>,
    project_service: &ProjectApplicationService<ProjectRepo, OrganizationRepo>,
) -> anyhow::Result<()> {
    let organization = organization_service
        .create_organization(CreateOrganizationRequest {
            name: "Gos & co".to_owned(),
            description: String::new(),
        })
        .await
        .context("failed to seed default organization")?;
    let project = project_service
        .create_project(
            &organization.id,
            CreateProjectRequest {
                name: "Workinabox".to_owned(),
                description: String::new(),
            },
        )
        .await
        .context("failed to seed default project")?
        .expect("seed organization exists");
    info!(
        "seeded organization '{}' with project '{}'",
        organization.id, project.id
    );
    Ok(())
}

/// Seeds an initial Owner user for the default org and logs a one-time access token, so
/// there is a way to authenticate before the real identity provider exists. Re-seeded each
/// boot because metadata is in-memory.
async fn seed_owner(
    user_service: &UserApplicationService<UserRepo>,
    access_service: &AccessApplicationService<RoleAssignmentRepo, UserRepo>,
) {
    let owner = user_service
        .create_user(CreateUserRequest {
            kind: "human".to_owned(),
            name: "Owner".to_owned(),
            email: Some("owner@workinabox.local".to_owned()),
        })
        .await
        .expect("failed to seed owner user");
    let user_id: UserId = owner.id.parse().expect("seeded owner id is valid");
    access_service
        .grant_direct(
            user_id,
            Scope::Org(OrganizationId::from_number(1)),
            Role::Owner,
        )
        .await
        .expect("failed to grant owner role");
    let issued = user_service
        .issue_token(
            &owner.id,
            IssueTokenRequest {
                label: "bootstrap".to_owned(),
                read_only: false,
                repos: None,
                orgs: None,
                expires_at: None,
            },
        )
        .await
        .expect("failed to issue bootstrap token")
        .expect("seeded owner exists");
    info!(
        "seeded owner '{}' (Owner of O-1) — bootstrap access token: {}",
        owner.id, issued.plaintext
    );
}

async fn log_loaded_meetings(
    meeting_service: &MeetingApplicationService<InMemoryMeetingRepository>,
) {
    let meetings = meeting_service
        .list_meetings()
        .await
        .expect("failed to list seeded meetings");
    info!("loaded {} meetings from startup data", meetings.len());
    for meeting in &meetings {
        info!("meeting '{}' state {:?}", meeting.title, meeting.state);
    }
}

fn spawn_transcript_runtime(
    sfu: Arc<Sfu>,
    mut transcript_rx: mpsc::UnboundedReceiver<FinalizedTranscript>,
) {
    tokio::spawn(async move {
        while let Some(transcript) = transcript_rx.recv().await {
            sfu.handle_finalized_transcript(transcript).await;
        }
    });
}

fn load_meeting_intelligence() -> anyhow::Result<Arc<dyn MeetingIntelligence>> {
    match std::env::var("WIAB_MEETING_INTELLIGENCE")
        .unwrap_or_else(|_| "heuristic".to_owned())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "heuristic" => {
            info!("meeting intelligence adapter: heuristic");
            Ok(Arc::new(HeuristicMeetingIntelligence))
        }
        "llama" => {
            let intelligence = LlamaMeetingIntelligence::from_env()
                .context("failed to initialize llama meeting intelligence")?;
            info!("meeting intelligence adapter: llama");
            Ok(Arc::new(intelligence))
        }
        other => Err(anyhow::anyhow!(
            "unsupported WIAB_MEETING_INTELLIGENCE value '{other}'"
        )),
    }
}
