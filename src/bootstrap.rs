use std::sync::Arc;

use anyhow::Context;
use authbox_app::{
    AuthenticationService, FederationService, InvitationService, PasswordResetService,
    SessionConfig,
};
use authbox_core::auth::{EmailSender, FederationConnection, PrincipalId};
use authbox_inf::{
    Argon2idPasswordHasher, AuthFlowStoreImpl, CredentialStoreImpl, FederatedIdentityStoreImpl,
    InMemoryAuthFlowStore, InMemoryCredentialStore, InMemoryFederatedIdentityStore,
    InMemorySessionStore, InMemoryVerificationTokenStore, LoggingEmailSender, OidcRelyingParty,
    PostgresAuthFlowStore, PostgresCredentialStore, PostgresFederatedIdentityStore,
    PostgresSessionStore, PostgresVerificationTokenStore, RandomSecretGenerator, ResendEmailSender,
    SessionStoreImpl, SmtpEmailSender, VerificationTokenStoreImpl,
};
use tokio::sync::mpsc;
use tracing::info;
use wiab_app::{
    AccessApplicationService, AgentApplicationService, AuthorizationService,
    BoardApplicationService, CreateOrganizationRequest, CreateProjectRequest, CreateUserRequest,
    IssueTokenRequest, MeetingApplicationService, OrganizationApplicationService,
    PipelineApplicationService, ProjectApplicationService, PullRequestApplicationService,
    RepoApplicationService, TaskApplicationService, TeamApplicationService, UserApplicationService,
    VmApplicationService, WorkApplicationService, run_publisher,
};
use wiab_core::{
    access::{Role, RoleAssignmentRepository, Scope},
    agent::AgentRepository,
    board::BoardRepository,
    meeting_traits::{Clock, MeetingIntelligence},
    organization::{OrganizationId, OrganizationRepository},
    pipeline::PipelineRepository,
    project::ProjectRepository,
    pull_request::PullRequestRepository,
    repo::{GitBackend, RepoRepository},
    task::TaskRepository,
    team::TeamRepository,
    transcript::FinalizedTranscript,
    user::{UserId, UserRepository},
    vm::VmRepository,
    work::WorkRepository,
};
use wiab_inf::{
    AgentRepo, AppState, AuthSettings, BoardRepo, DefaultSpeechSynthesizer, DockerRuntime,
    FirecrackerRuntime, Git2Backend, InMemoryAgentNumbering, InMemoryAgentRepository,
    InMemoryBoardNumbering, InMemoryBoardRepository, InMemoryMeetingRepository,
    InMemoryOrganizationNumbering, InMemoryOrganizationRepository, InMemoryPipelineNumbering,
    InMemoryPipelineRepository, InMemoryProjectNumbering, InMemoryProjectRepository,
    InMemoryPullRequestNumbering, InMemoryPullRequestRepository, InMemoryRepoNumbering,
    InMemoryRepoRepository, InMemoryRoleAssignmentNumbering, InMemoryRoleAssignmentRepository,
    InMemoryTaskNumbering, InMemoryTaskRepository, InMemoryTeamNumbering, InMemoryTeamRepository,
    InMemoryUserNumbering, InMemoryUserRepository, InMemoryVmNumbering, InMemoryVmRepository,
    InMemoryWorkNumbering, InMemoryWorkRepository, LlamaMeetingIntelligence, MessagingDispatch,
    NatsMessaging, OrganizationRepo, PipelineRepo, PostgresAgentRepository,
    PostgresBoardRepository, PostgresOrganizationRepository, PostgresOutbox,
    PostgresPipelineRepository, PostgresProjectRepository, PostgresPullRequestRepository,
    PostgresRepoRepository, PostgresRoleAssignmentRepository, PostgresTaskRepository,
    PostgresTeamRepository, PostgresUserRepository, PostgresVmRepository, PostgresWorkRepository,
    ProjectRepo, PullRequestRepo, RandomTokenFactory, RepoRepo, RoleAssignmentRepo, Sfu,
    Sha256KeyFingerprinter, Sha256TokenHasher, SystemClock, TaskRepo, TeamRepo, UserRepo, VmRepo,
    VmRuntimeDispatch, WiabAuthService, WiabTeamIdentity, WiabUserDirectory, WorkRepo, pg_pool,
};

/// `certificate_pem` is the server's own TLS certificate. Team containers are handed it so
/// they can verify this backend; see `TeamApplicationService`.
/// How often the publisher checks the outbox. Events are for observing a team, not for
/// driving it, so seconds of lag cost nothing and a tighter loop would just poll harder.
const OUTBOX_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Login email of the bootstrap Owner seeded into an empty store.
const SEED_OWNER_EMAIL: &str = "owner@workinabox.local";

pub async fn build_app_state(
    config: &crate::config::AppConfig,
    certificate_pem: Option<String>,
) -> anyhow::Result<AppState> {
    let persistence = &config.serve.persistence;
    let database_url = &config.serve.database_url;
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
            authbox_inf::run_migrations(&pool)
                .await
                .context("failed to apply authbox migrations")?;
            info!("persistence backend: postgres");
            Some(pool)
        }
        other => anyhow::bail!(
            "unsupported persistence value '{other}' (expected 'memory' or 'postgres')"
        ),
    };

    // Opt-in: an existing deployment without a broker keeps working, and `Disabled`
    // makes a publish fail loudly rather than look like it succeeded.
    let messaging = match &config.nats {
        Some(nats) => MessagingDispatch::Nats(NatsMessaging::connect(nats).await?),
        None => {
            info!("messaging: disabled (set WIAB_NATS_ENABLED to publish)");
            MessagingDispatch::Disabled
        }
    };

    // Drain the outbox to the broker. Only with both a database to read from and a broker
    // to publish to — either alone gives events nowhere to go, and a task looping over
    // nothing is just noise in the logs.
    match (&pool, &messaging) {
        (Some(pool), MessagingDispatch::Nats(nats)) => {
            let outbox = PostgresOutbox::new(pool.clone());
            let nats: NatsMessaging = (*nats).clone();
            tokio::spawn(async move {
                run_publisher(outbox, nats, OUTBOX_INTERVAL).await;
            });
            info!(
                "outbox publisher started ({}s interval)",
                OUTBOX_INTERVAL.as_secs()
            );
        }
        _ => info!("outbox publisher disabled (needs both postgres and nats)"),
    }

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

    // VM service. The runtime is Firecracker only when enabled AND /dev/kvm is present (the demo
    // host); otherwise Docker, so the backend runs agents in containers on a Mac / CI / non-KVM
    // host (and in prod for operators who accept the weaker shared-kernel boundary).
    let vm_repo = match &pool {
        Some(pool) => VmRepo::Postgres(PostgresVmRepository::new(pool.clone())),
        None => VmRepo::InMemory(InMemoryVmRepository::new()),
    };
    let vm_numbering =
        InMemoryVmNumbering::starting_at(next_after(&vm_repo.list().await?, |vm| vm.id().number()));
    let vm_runtime = if config.vm.firecracker_enabled && std::path::Path::new("/dev/kvm").exists() {
        info!("vm runtime: firecracker (/dev/kvm present)");
        VmRuntimeDispatch::Firecracker(Box::new(FirecrackerRuntime::new(
            config.vm.firecracker.clone(),
        )))
    } else {
        info!("vm runtime: docker (firecracker disabled or /dev/kvm absent)");
        VmRuntimeDispatch::Docker(DockerRuntime::new(config.vm.docker.clone()).await?)
    };
    let vm_service = Arc::new(VmApplicationService::new(
        vm_repo,
        organization_repo.clone(),
        vm_runtime,
        Arc::new(vm_numbering),
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
        vm_service.clone(),
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
        board_repo.clone(),
        project_repo.clone(),
        Arc::new(board_numbering),
    ));

    let git_root = config.serve.git_root.clone();
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
        git_backend.clone(),
    ));

    let pull_request_repo = match &pool {
        Some(pool) => PullRequestRepo::Postgres(PostgresPullRequestRepository::new(pool.clone())),
        None => PullRequestRepo::InMemory(InMemoryPullRequestRepository::new()),
    };
    let pull_request_numbering = InMemoryPullRequestNumbering::starting_at(next_after(
        &pull_request_repo.list().await?,
        |pull_request| pull_request.id().number(),
    ));
    let pull_request_service = Arc::new(PullRequestApplicationService::new(
        pull_request_repo.clone(),
        repo_repo.clone(),
        Arc::new(pull_request_numbering),
        git_backend,
        Arc::new(SystemClock),
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
    let team_repo = match &pool {
        Some(pool) => TeamRepo::Postgres(PostgresTeamRepository::new(pool.clone())),
        None => TeamRepo::InMemory(InMemoryTeamRepository::new()),
    };
    let team_numbering =
        InMemoryTeamNumbering::starting_at(next_after(&team_repo.list().await?, |team| {
            team.id().number()
        }));
    let team_service = Arc::new(TeamApplicationService::new(
        team_repo,
        organization_repo.clone(),
        board_repo.clone(),
        repo_repo.clone(),
        vm_service,
        WiabTeamIdentity::new(user_service.clone(), access_service.clone()),
        Arc::new(team_numbering),
        config.auth.base_url.clone(),
        certificate_pem,
    ));

    let authorization_service = Arc::new(AuthorizationService::new(
        assignment_repo.clone(),
        repo_repo.clone(),
        project_repo.clone(),
    ));

    // Reusable authentication (password login + browser sessions). Keyed on the user id, it
    // resolves logins through the user service via `WiabUserDirectory` and persists its own
    // sessions/credentials in the selected backend.
    let session_store = match &pool {
        Some(pool) => SessionStoreImpl::Postgres(PostgresSessionStore::new(pool.clone())),
        None => SessionStoreImpl::InMemory(InMemorySessionStore::new()),
    };
    let credential_store = match &pool {
        Some(pool) => CredentialStoreImpl::Postgres(PostgresCredentialStore::new(pool.clone())),
        None => CredentialStoreImpl::InMemory(InMemoryCredentialStore::new()),
    };
    let auth_service = Arc::new(AuthenticationService::new(
        session_store.clone(),
        credential_store.clone(),
        WiabUserDirectory::new(user_service.clone()),
        Arc::new(Argon2idPasswordHasher),
        Arc::new(RandomSecretGenerator),
        Arc::new(Sha256TokenHasher),
        Arc::new(SystemClock),
        // 8-hour idle window, 7-day absolute cap.
        SessionConfig {
            idle_seconds: 28_800,
            absolute_seconds: 604_800,
        },
    ));

    // Seed the default org + owner only when the store is empty, so a Postgres-backed
    // restart does not try to re-create them (which would fail the unique-id insert).
    if organization_service.list_organizations().await?.is_empty() {
        seed_default_organization(organization_service.as_ref(), project_service.as_ref()).await?;
        seed_owner(
            user_service.as_ref(),
            access_service.as_ref(),
            auth_service.as_ref(),
            &config.dev,
        )
        .await;
    }

    // Pre-provision a dev SSO user (env-gated, idempotent) so a real Entra/OIDC login lands
    // on an existing Owner account rather than a role-less JIT one.
    seed_sso_owner(
        user_service.as_ref(),
        access_service.as_ref(),
        auth_service.as_ref(),
        &config.dev,
    )
    .await?;

    // Meetings are in-memory (ephemeral live sessions), but a seeded meeting needs its owner to
    // be a real user — a human seat is bound to the user entitled to occupy it. So the meeting
    // store is built here, after the owner exists, rather than at the top of bootstrap.
    let seed_clock = SystemClock;
    let meeting_repository = match user_service.find_by_email(SEED_OWNER_EMAIL).await? {
        Some(owner) => {
            InMemoryMeetingRepository::with_seed_data(owner, || seed_clock.now_rfc3339())
        }
        None => {
            info!(
                "no '{SEED_OWNER_EMAIL}' user; starting with no seeded meeting (nothing to own it)"
            );
            InMemoryMeetingRepository::new()
        }
    };
    let intelligence = load_meeting_intelligence(&config.meeting)?;
    let meeting_service = Arc::new(MeetingApplicationService::new(
        meeting_repository.clone(),
        intelligence,
        Arc::new(DefaultSpeechSynthesizer::from_env()),
        Arc::new(SystemClock),
    ));

    log_loaded_meetings(meeting_service.as_ref()).await;

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
        work_repo.clone(),
        project_repo.clone(),
        Arc::new(work_numbering),
    ));

    let task_repo = match &pool {
        Some(pool) => TaskRepo::Postgres(PostgresTaskRepository::new(pool.clone())),
        None => TaskRepo::InMemory(InMemoryTaskRepository::new()),
    };
    let task_numbering =
        InMemoryTaskNumbering::starting_at(next_after(&task_repo.list().await?, |task| {
            task.id().number()
        }));
    let task_service = Arc::new(TaskApplicationService::new(
        task_repo,
        board_repo,
        work_repo,
        Arc::new(task_numbering),
    ));

    // HTTP auth configuration. Cookie `Secure` follows the base-url scheme so the dev
    // HTTP origin still gets its cookie. Signup/Google/OIDC are opt-in (off by default).
    let base_url = config.auth.base_url.clone();
    let auth_settings = AuthSettings {
        cookie_secure: base_url.starts_with("https"),
        signup_enabled: config.auth.local_signup,
        google_enabled: config.auth.google_enabled,
        oidc_enabled: config.auth.oidc_enabled,
        base_url,
    };

    // Inbound OIDC federation connections, built from env (off by default). Google and the
    // enterprise IdP are the same relying-party path with different config.
    let mut connections: Vec<FederationConnection> = Vec::new();
    if auth_settings.google_enabled {
        connections.push(FederationConnection {
            slug: "google".to_owned(),
            issuer: "https://accounts.google.com".to_owned(),
            client_id: config.auth.google_client_id.clone(),
            client_secret: config.auth.google_client_secret.clone(),
            scopes: vec![
                "openid".to_owned(),
                "email".to_owned(),
                "profile".to_owned(),
            ],
            redirect_uri: format!("{}/api/auth/oidc/google/callback", auth_settings.base_url),
            auto_link_verified_email: false,
            // Google reliably stamps email_verified; require it.
            require_email_verified: true,
        });
    }
    if auth_settings.oidc_enabled {
        connections.push(FederationConnection {
            slug: "enterprise".to_owned(),
            issuer: config.auth.oidc_issuer.clone(),
            client_id: config.auth.oidc_client_id.clone(),
            client_secret: config.auth.oidc_client_secret.clone(),
            scopes: vec![
                "openid".to_owned(),
                "email".to_owned(),
                "profile".to_owned(),
            ],
            redirect_uri: format!(
                "{}/api/auth/oidc/enterprise/callback",
                auth_settings.base_url
            ),
            auto_link_verified_email: true,
            // An enterprise IdP is authoritative for its users and may omit
            // email_verified (e.g. Microsoft Entra) — don't require the claim.
            require_email_verified: false,
        });
    }
    let federation_service = if connections.is_empty() {
        None
    } else {
        let federated_store = match &pool {
            Some(pool) => FederatedIdentityStoreImpl::Postgres(
                PostgresFederatedIdentityStore::new(pool.clone()),
            ),
            None => FederatedIdentityStoreImpl::InMemory(InMemoryFederatedIdentityStore::new()),
        };
        let flow_store = match &pool {
            Some(pool) => AuthFlowStoreImpl::Postgres(PostgresAuthFlowStore::new(pool.clone())),
            None => AuthFlowStoreImpl::InMemory(InMemoryAuthFlowStore::new()),
        };
        Some(Arc::new(FederationService::new(
            federated_store,
            flow_store,
            WiabUserDirectory::new(user_service.clone()),
            OidcRelyingParty::new(connections.clone()),
            Arc::new(SystemClock),
            connections,
            // 10-minute login-state TTL.
            600,
        )))
    };

    // Transactional email (reset/invite/verify). Provider is selectable via
    // WIAB_EMAIL_PROVIDER (default "resend"); "smtp" uses lettre. Missing credentials fall
    // back to logging so dev still surfaces the link in the server log.
    let from = config.email.from.clone();
    let email_sender: Arc<dyn EmailSender> =
        match config.email.provider.trim().to_ascii_lowercase().as_str() {
            "resend" => match &config.email.resend_api_key {
                Some(key) => {
                    info!("email delivery: Resend (from {from})");
                    Arc::new(ResendEmailSender::new(key.clone(), from))
                }
                None => {
                    info!("email delivery: logging only (set RESEND_API_KEY to send via Resend)");
                    Arc::new(LoggingEmailSender)
                }
            },
            "smtp" => match &config.email.smtp_host {
                Some(host) => {
                    let port = config.email.smtp_port;
                    match SmtpEmailSender::new(
                        host,
                        port,
                        config.email.smtp_user.clone(),
                        config.email.smtp_password.clone(),
                        &from,
                        config.email.smtp_tls,
                    ) {
                        Ok(sender) => {
                            info!("email delivery: SMTP {host}:{port}");
                            Arc::new(sender)
                        }
                        Err(error) => {
                            info!("email delivery: SMTP config invalid ({error}); logging instead");
                            Arc::new(LoggingEmailSender)
                        }
                    }
                }
                None => {
                    info!("email delivery: logging only (set WIAB_SMTP_HOST to send via SMTP)");
                    Arc::new(LoggingEmailSender)
                }
            },
            other => {
                info!("email delivery: unknown WIAB_EMAIL_PROVIDER '{other}', logging only");
                Arc::new(LoggingEmailSender)
            }
        };
    let verification_store = match &pool {
        Some(pool) => {
            VerificationTokenStoreImpl::Postgres(PostgresVerificationTokenStore::new(pool.clone()))
        }
        None => VerificationTokenStoreImpl::InMemory(InMemoryVerificationTokenStore::new()),
    };
    let password_reset_service = Arc::new(PasswordResetService::new(
        WiabUserDirectory::new(user_service.clone()),
        verification_store.clone(),
        credential_store.clone(),
        session_store,
        Arc::new(Argon2idPasswordHasher),
        Arc::new(RandomSecretGenerator),
        Arc::new(Sha256TokenHasher),
        Arc::new(SystemClock),
        email_sender.clone(),
        auth_settings.base_url.clone(),
        // 1-hour reset link.
        3600,
    ));
    let invitation_service = Arc::new(InvitationService::new(
        verification_store,
        credential_store,
        Arc::new(Argon2idPasswordHasher),
        Arc::new(RandomSecretGenerator),
        Arc::new(Sha256TokenHasher),
        Arc::new(SystemClock),
        email_sender,
        auth_settings.base_url.clone(),
        // 24-hour invite / email-verification link.
        86_400,
    ));

    let (transcript_tx, transcript_rx) = mpsc::unbounded_channel::<FinalizedTranscript>();
    let sfu = Arc::new(
        Sfu::new(meeting_service.clone(), &config.media, transcript_tx)
            .await
            .context("failed to initialize SFU")?,
    );
    spawn_transcript_runtime(sfu.clone(), transcript_rx);

    Ok(AppState {
        meeting_service,
        organization_service,
        project_service,
        agent_service,
        task_service,
        team_service,
        board_service,
        messaging: Arc::new(messaging),
        pull_request_service,
        repo_service,
        user_service,
        auth_service,
        federation_service,
        password_reset_service,
        invitation_service,
        access_service,
        authorization_service,
        pipeline_service,
        work_service,
        sfu,
        auth_settings,
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
    auth_service: &WiabAuthService,
    dev: &crate::config::DevConfig,
) {
    let owner = user_service
        .create_user(CreateUserRequest {
            kind: "human".to_owned(),
            name: "Owner".to_owned(),
            email: Some(SEED_OWNER_EMAIL.to_owned()),
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
    // Seed a password so a human can log in interactively (the bootstrap token above stays
    // for machine/agent access). Dev-only default; override with WIAB_DEV_OWNER_PASSWORD.
    let dev_password = &dev.owner_password;
    auth_service
        .set_password(PrincipalId::new(owner.id.clone()), dev_password)
        .await
        .expect("failed to seed owner password");
    info!(
        "seeded owner '{}' (Owner of O-1) — login: owner@workinabox.local / {} — bootstrap access token: {}",
        owner.id, dev_password, issued.plaintext
    );
}

/// Pre-provisions a dev SSO user (Active + Owner of O-1) from env, so a real Entra/OIDC
/// login matches an existing account instead of landing role-less. Idempotent; a no-op
/// unless `WIAB_DEV_SSO_OWNER_EMAIL` is set. Dev-only convenience.
async fn seed_sso_owner(
    user_service: &UserApplicationService<UserRepo>,
    access_service: &AccessApplicationService<RoleAssignmentRepo, UserRepo>,
    auth_service: &WiabAuthService,
    dev: &crate::config::DevConfig,
) -> anyhow::Result<()> {
    let email = &dev.sso_owner_email;
    if email.is_empty() {
        return Ok(());
    }
    if user_service.find_by_email(email).await?.is_some() {
        info!("dev SSO owner '{email}' already present — leaving as-is");
        return Ok(());
    }
    let name = dev.sso_owner_name.clone();
    let user = user_service
        .create_user(CreateUserRequest {
            kind: "human".to_owned(),
            name,
            email: Some(email.clone()),
        })
        .await
        .context("failed to seed dev SSO owner")?;
    let user_id: UserId = user.id.parse().expect("seeded SSO owner id is valid");
    access_service
        .grant_direct(
            user_id,
            Scope::Org(OrganizationId::from_number(1)),
            Role::Owner,
        )
        .await
        .context("failed to grant dev SSO owner role")?;
    // Optional local-password fallback for first run if SSO misbehaves.
    if let Some(password) = &dev.sso_owner_password {
        auth_service
            .set_password(PrincipalId::new(user.id.clone()), password)
            .await
            .context("failed to set dev SSO owner password")?;
    }
    info!(
        "seeded dev SSO owner '{}' ({email}) as Owner of O-1",
        user.id
    );
    Ok(())
}

async fn log_loaded_meetings(
    meeting_service: &MeetingApplicationService<InMemoryMeetingRepository>,
) {
    // Seeded meetings live in the bootstrap org O-1 (see `InMemoryMeetingRepository` seed data).
    let meetings = meeting_service
        .list_meetings("O-1")
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

fn load_meeting_intelligence(
    meeting: &crate::config::MeetingConfig,
) -> anyhow::Result<Option<Arc<dyn MeetingIntelligence>>> {
    let Some(llama) = &meeting.llama else {
        info!("meeting intelligence disabled (WIAB_LLAMA_ENABLED off)");
        return Ok(None);
    };

    let intelligence = LlamaMeetingIntelligence::new(llama)
        .context("failed to initialize llama meeting intelligence")?;
    info!("meeting intelligence adapter: llama (eager-loaded)");
    Ok(Some(Arc::new(intelligence)))
}
