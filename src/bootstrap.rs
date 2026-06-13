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
    access::{Role, Scope},
    meeting_traits::{Clock, MeetingIntelligence},
    organization::OrganizationId,
    repo::GitBackend,
    transcript::FinalizedTranscript,
    user::UserId,
};
use wiab_inf::{
    AppState, DefaultSpeechSynthesizer, Git2Backend, HeuristicMeetingIntelligence,
    InMemoryAgentNumbering, InMemoryAgentRepository, InMemoryBoardNumbering,
    InMemoryBoardRepository, InMemoryMeetingRepository, InMemoryOrganizationNumbering,
    InMemoryOrganizationRepository, InMemoryPipelineNumbering, InMemoryPipelineRepository,
    InMemoryProjectNumbering, InMemoryProjectRepository, InMemoryRepoNumbering,
    InMemoryRepoRepository, InMemoryRoleAssignmentNumbering, InMemoryRoleAssignmentRepository,
    InMemoryUserNumbering, InMemoryUserRepository, InMemoryWorkNumbering, InMemoryWorkRepository,
    LlamaMeetingIntelligence, RandomTokenFactory, Sfu, Sha256KeyFingerprinter, Sha256TokenHasher,
    SystemClock,
};

pub async fn build_app_state() -> anyhow::Result<AppState> {
    let seed_clock = SystemClock;
    let meeting_repository = InMemoryMeetingRepository::with_seed_data(|| seed_clock.now_rfc3339());
    let intelligence = load_meeting_intelligence()?;
    let meeting_service = Arc::new(MeetingApplicationService::new(
        meeting_repository.clone(),
        intelligence,
        Arc::new(DefaultSpeechSynthesizer::from_env()),
        Arc::new(SystemClock),
    ));

    log_loaded_meetings(meeting_service.as_ref());

    let organization_repository = InMemoryOrganizationRepository::new();
    let organization_service = Arc::new(OrganizationApplicationService::new(
        organization_repository.clone(),
        Arc::new(InMemoryOrganizationNumbering::new()),
    ));

    let project_repository = InMemoryProjectRepository::new();
    let project_service = Arc::new(ProjectApplicationService::new(
        project_repository.clone(),
        organization_repository.clone(),
        Arc::new(InMemoryProjectNumbering::new()),
    ));

    seed_default_organization(organization_service.as_ref(), project_service.as_ref())?;

    let agent_repository = InMemoryAgentRepository::new();
    let agent_service = Arc::new(AgentApplicationService::new(
        agent_repository,
        organization_repository.clone(),
        Arc::new(InMemoryAgentNumbering::new()),
    ));

    let board_repository = InMemoryBoardRepository::new();
    let board_service = Arc::new(BoardApplicationService::new(
        board_repository,
        project_repository.clone(),
        Arc::new(InMemoryBoardNumbering::new()),
    ));

    let git_root = std::env::var("WIAB_GIT_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("wiab-git"));
    std::fs::create_dir_all(&git_root)
        .with_context(|| format!("failed to create git root {}", git_root.display()))?;
    info!("hosting git repos under {}", git_root.display());
    let git_backend: Arc<dyn GitBackend> = Arc::new(Git2Backend::new(git_root.clone()));

    let repo_repository = InMemoryRepoRepository::new();
    let repo_service = Arc::new(RepoApplicationService::new(
        repo_repository.clone(),
        project_repository.clone(),
        Arc::new(InMemoryRepoNumbering::new()),
        git_backend,
    ));

    // Identity, credentials, and access control.
    let user_repository = InMemoryUserRepository::new();
    let user_service = Arc::new(UserApplicationService::new(
        user_repository.clone(),
        Arc::new(InMemoryUserNumbering::new()),
        Arc::new(RandomTokenFactory),
        Arc::new(Sha256TokenHasher),
        Arc::new(Sha256KeyFingerprinter),
        Arc::new(SystemClock),
    ));

    let assignment_repository = InMemoryRoleAssignmentRepository::new();
    let access_service = Arc::new(AccessApplicationService::new(
        assignment_repository.clone(),
        user_repository.clone(),
        Arc::new(InMemoryRoleAssignmentNumbering::new()),
    ));
    let authorization_service = Arc::new(AuthorizationService::new(
        assignment_repository.clone(),
        repo_repository.clone(),
        project_repository.clone(),
    ));

    seed_owner(user_service.as_ref(), access_service.as_ref());

    let pipeline_repository = InMemoryPipelineRepository::new();
    let pipeline_service = Arc::new(PipelineApplicationService::new(
        pipeline_repository,
        project_repository.clone(),
        Arc::new(InMemoryPipelineNumbering::new()),
    ));

    let work_service = Arc::new(WorkApplicationService::new(
        InMemoryWorkRepository::new(),
        project_repository.clone(),
        Arc::new(InMemoryWorkNumbering::new()),
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

fn seed_default_organization(
    organization_service: &OrganizationApplicationService<InMemoryOrganizationRepository>,
    project_service: &ProjectApplicationService<
        InMemoryProjectRepository,
        InMemoryOrganizationRepository,
    >,
) -> anyhow::Result<()> {
    let organization = organization_service
        .create_organization(CreateOrganizationRequest {
            name: "Gos & co".to_owned(),
            description: String::new(),
        })
        .context("failed to seed default organization")?;
    let project = project_service
        .create_project(
            &organization.id,
            CreateProjectRequest {
                name: "Workinabox".to_owned(),
                description: String::new(),
            },
        )
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
fn seed_owner(
    user_service: &UserApplicationService<InMemoryUserRepository>,
    access_service: &AccessApplicationService<
        InMemoryRoleAssignmentRepository,
        InMemoryUserRepository,
    >,
) {
    let owner = user_service
        .create_user(CreateUserRequest {
            kind: "human".to_owned(),
            name: "Owner".to_owned(),
            email: Some("owner@workinabox.local".to_owned()),
        })
        .expect("failed to seed owner user");
    let user_id: UserId = owner.id.parse().expect("seeded owner id is valid");
    access_service.grant_direct(
        user_id,
        Scope::Org(OrganizationId::from_number(1)),
        Role::Owner,
    );
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
        .expect("failed to issue bootstrap token")
        .expect("seeded owner exists");
    info!(
        "seeded owner '{}' (Owner of O-1) — bootstrap access token: {}",
        owner.id, issued.plaintext
    );
}

fn log_loaded_meetings(meeting_service: &MeetingApplicationService<InMemoryMeetingRepository>) {
    let meetings = meeting_service.list_meetings();
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
