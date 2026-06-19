use authbox_core::auth::{AuthError, PrincipalId};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, Request, State, ws::WebSocketUpgrade},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{AppendHeaders, IntoResponse, Response},
    routing::{delete, get, post, put},
};
use base64::Engine;
use wiab_app::{
    AddDoneRequest, AddSshKeyRequest, CommitChangesRequest, CreateAgentRequest, CreateBoardRequest,
    CreateMeetingRequest, CreateOrganizationRequest, CreatePipelineRequest, CreateProjectRequest,
    CreateRepoRequest, CreateUserRequest, CreateWorkRequest, GrantRoleRequest, IssueTokenRequest,
    IssuedTokenSnapshot, SetVisibilityRequest, UpdateAgentRequest, UpdateBoardRequest,
    UpdateOrganizationRequest, UpdatePipelineRequest, UpdateProjectRequest, UpdateRepoRequest,
    UpdateWorkRequest,
};
use wiab_core::access::{Operation, Role, RoleAssignmentSnapshot, Scope};
use wiab_core::agent::{AgentId, AgentSnapshot};
use wiab_core::board::BoardSnapshot;
use wiab_core::meeting::MeetingSnapshot;
use wiab_core::organization::OrganizationId;
use wiab_core::organization::OrganizationSnapshot;
use wiab_core::pipeline::PipelineSnapshot;
use wiab_core::project::ProjectSnapshot;
use wiab_core::repo::{BranchSnapshot, CommitSnapshot, FileEntrySnapshot, RepoId, RepoSnapshot};
use wiab_core::user::{TokenScope, UserId, UserSnapshot};
use wiab_core::work::WorkSnapshot;

use crate::{AppState, handle_signal_socket};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/meetings", get(list_meetings).post(create_meeting))
        .route(
            "/organizations",
            get(list_organizations).post(create_organization),
        )
        .route(
            "/organizations/{organization_id}",
            put(update_organization).get(get_organization),
        )
        .route(
            "/organizations/{organization_id}/projects",
            get(list_projects).post(create_project),
        )
        .route(
            "/organizations/{organization_id}/agents",
            get(list_agents).post(create_agent),
        )
        .route(
            "/projects/{project_id}",
            put(update_project).get(get_project),
        )
        .route(
            "/projects/{project_id}/works",
            get(list_project_works).post(create_work),
        )
        .route(
            "/projects/{project_id}/boards",
            get(list_boards).post(create_board),
        )
        .route(
            "/projects/{project_id}/repos",
            get(list_repos).post(create_repo),
        )
        .route(
            "/projects/{project_id}/pipelines",
            get(list_pipelines).post(create_pipeline),
        )
        .route("/agents/{agent_id}", put(update_agent).get(get_agent))
        .route("/boards/{board_id}", put(update_board).get(get_board))
        .route("/repos/{repo_id}", put(update_repo).get(get_repo))
        .route("/repos/{repo_id}/branches", get(list_branches))
        .route(
            "/repos/{repo_id}/branches/{branch}/files",
            get(list_repo_files),
        )
        .route(
            "/repos/{repo_id}/branches/{branch}/files/raw",
            get(read_repo_file),
        )
        .route(
            "/repos/{repo_id}/branches/{branch}/commits",
            get(list_repo_commits),
        )
        .route("/repos/{repo_id}/commits", post(create_commit))
        .route("/repos/{repo_id}/visibility", put(set_repo_visibility))
        // Identity & access management.
        .route("/users", get(list_users).post(create_user))
        .route("/users/{user_id}", get(get_user))
        .route("/users/{user_id}/ssh-keys", post(add_ssh_key))
        .route("/users/{user_id}/ssh-keys/{key_id}", delete(remove_ssh_key))
        .route("/users/{user_id}/tokens", post(issue_token))
        .route("/users/{user_id}/tokens/{token_id}", delete(revoke_token))
        .route("/users/invite", post(invite_user))
        .route("/users/{user_id}/deactivate", post(deactivate_user))
        .route("/users/{user_id}/activate", post(activate_user))
        .route(
            "/role-assignments",
            get(list_role_assignments).post(grant_role),
        )
        .route("/role-assignments/{assignment_id}", delete(revoke_role))
        // Interactive authentication: local password login, current-user, logout, and the
        // login-method config the SPA reads to decide which buttons to show.
        .route("/auth/session", post(login).get(whoami).delete(logout))
        .route("/auth/password", put(change_password))
        .route("/auth/password/reset/request", post(password_reset_request))
        .route("/auth/password/reset/confirm", post(password_reset_confirm))
        .route("/auth/config", get(auth_config))
        // Inbound OIDC federation (Google / enterprise SSO): start redirects to the IdP, the
        // callback validates and establishes a session. Enabled per deployment via flags.
        .route("/auth/oidc/{connection}/start", get(oidc_start))
        .route("/auth/oidc/{connection}/callback", get(oidc_callback))
        .route("/auth/signup", post(signup))
        .route("/auth/invite/accept", post(accept_invite))
        .route("/auth/verify-email", post(verify_email))
        // Git Smart-HTTP transport (real `git clone`/`fetch`/`push`). The `{repo_id}`
        // segment arrives as `R-<n>.git`; the handlers strip the `.git` suffix.
        .route(
            "/repos/{repo_id}/info/refs",
            get(crate::git_http::info_refs),
        )
        .route(
            "/repos/{repo_id}/git-upload-pack",
            post(crate::git_http::upload_pack),
        )
        .route(
            "/repos/{repo_id}/git-receive-pack",
            post(crate::git_http::receive_pack),
        )
        .route(
            "/pipelines/{pipeline_id}",
            put(update_pipeline).get(get_pipeline),
        )
        .route("/works/{work_id}", get(get_work).put(update_work))
        .route("/works/{work_id}/dones", post(add_done))
        .route(
            "/works/{work_id}/dones/{done_id}/fulfill",
            post(fulfill_done),
        )
        .route(
            "/works/{work_id}/dones/{done_id}/unfulfill",
            post(unfulfill_done),
        )
        .route("/signal", get(signal))
        .layer(middleware::from_fn_with_state(state.clone(), csrf_guard))
        .with_state(state)
}

#[derive(serde::Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    Json(Health {
        status: "ok",
        version: state.version,
    })
}

async fn list_meetings(
    State(state): State<AppState>,
) -> Result<Json<Vec<MeetingSnapshot>>, (StatusCode, String)> {
    Ok(Json(
        state
            .meeting_service
            .list_meetings()
            .await
            .map_err(bad_request)?,
    ))
}

async fn create_meeting(
    State(state): State<AppState>,
    Json(request): Json<CreateMeetingRequest>,
) -> Result<Json<MeetingSnapshot>, (axum::http::StatusCode, String)> {
    state
        .meeting_service
        .create_meeting(request)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn list_organizations(
    State(state): State<AppState>,
) -> Result<Json<Vec<OrganizationSnapshot>>, (StatusCode, String)> {
    Ok(Json(
        state
            .organization_service
            .list_organizations()
            .await
            .map_err(bad_request)?,
    ))
}

async fn create_organization(
    State(state): State<AppState>,
    Json(request): Json<CreateOrganizationRequest>,
) -> Result<Json<OrganizationSnapshot>, (StatusCode, String)> {
    state
        .organization_service
        .create_organization(request)
        .await
        .map(Json)
        .map_err(bad_request)
}

async fn get_organization(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
) -> Result<Json<OrganizationSnapshot>, (StatusCode, String)> {
    match state
        .organization_service
        .organization_snapshot(&organization_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("organization", &organization_id)),
    }
}

async fn update_organization(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
    Json(request): Json<UpdateOrganizationRequest>,
) -> Result<Json<OrganizationSnapshot>, (StatusCode, String)> {
    match state
        .organization_service
        .update_organization(&organization_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("organization", &organization_id)),
    }
}

async fn list_projects(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
) -> Result<Json<Vec<ProjectSnapshot>>, (StatusCode, String)> {
    match state
        .project_service
        .list_projects(&organization_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshots) => Ok(Json(snapshots)),
        None => Err(not_found("organization", &organization_id)),
    }
}

async fn create_project(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<Json<ProjectSnapshot>, (StatusCode, String)> {
    match state
        .project_service
        .create_project(&organization_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("organization", &organization_id)),
    }
}

async fn get_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<ProjectSnapshot>, (StatusCode, String)> {
    match state
        .project_service
        .project_snapshot(&project_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn update_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(request): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectSnapshot>, (StatusCode, String)> {
    match state
        .project_service
        .update_project(&project_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn list_agents(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
) -> Result<Json<Vec<AgentSnapshot>>, (StatusCode, String)> {
    match state
        .agent_service
        .list_agents(&organization_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshots) => Ok(Json(snapshots)),
        None => Err(not_found("organization", &organization_id)),
    }
}

async fn create_agent(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
    Json(request): Json<CreateAgentRequest>,
) -> Result<Json<AgentSnapshot>, (StatusCode, String)> {
    let snapshot = match state
        .agent_service
        .create_agent(&organization_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => snapshot,
        None => return Err(not_found("organization", &organization_id)),
    };

    // Provision the agent's own user identity, granted Write on its org so it can push to
    // the org's repos once it has a key/token.
    let agent_id: AgentId = snapshot.id.parse().map_err(internal)?;
    let user = state
        .user_service
        .provision_agent_user(snapshot.name.clone(), agent_id)
        .await
        .map_err(bad_request)?;
    let user_id: UserId = user.id.parse().map_err(internal)?;
    let org: OrganizationId = snapshot.organization_id.parse().map_err(internal)?;
    state
        .access_service
        .grant_direct(user_id, Scope::Org(org), Role::Write)
        .await
        .map_err(bad_request)?;

    Ok(Json(snapshot))
}

async fn get_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentSnapshot>, (StatusCode, String)> {
    match state
        .agent_service
        .agent_snapshot(&agent_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("agent", &agent_id)),
    }
}

async fn update_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Json(request): Json<UpdateAgentRequest>,
) -> Result<Json<AgentSnapshot>, (StatusCode, String)> {
    match state
        .agent_service
        .update_agent(&agent_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("agent", &agent_id)),
    }
}

async fn list_boards(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<BoardSnapshot>>, (StatusCode, String)> {
    match state
        .board_service
        .list_boards(&project_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshots) => Ok(Json(snapshots)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn create_board(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(request): Json<CreateBoardRequest>,
) -> Result<Json<BoardSnapshot>, (StatusCode, String)> {
    match state
        .board_service
        .create_board(&project_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn get_board(
    State(state): State<AppState>,
    Path(board_id): Path<String>,
) -> Result<Json<BoardSnapshot>, (StatusCode, String)> {
    match state
        .board_service
        .board_snapshot(&board_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("board", &board_id)),
    }
}

async fn update_board(
    State(state): State<AppState>,
    Path(board_id): Path<String>,
    Json(request): Json<UpdateBoardRequest>,
) -> Result<Json<BoardSnapshot>, (StatusCode, String)> {
    match state
        .board_service
        .update_board(&board_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("board", &board_id)),
    }
}

async fn list_repos(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<RepoSnapshot>>, (StatusCode, String)> {
    match state
        .repo_service
        .list_repos(&project_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshots) => Ok(Json(snapshots)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn create_repo(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateRepoRequest>,
) -> Result<Json<RepoSnapshot>, (StatusCode, String)> {
    let Some(project) = state
        .project_service
        .project_snapshot(&project_id)
        .await
        .map_err(bad_request)?
    else {
        return Err(not_found("project", &project_id));
    };
    require_org_role(&state, &project.organization_id, Operation::Write, &headers).await?;
    match state
        .repo_service
        .create_repo(&project_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn get_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<RepoSnapshot>, (StatusCode, String)> {
    match state
        .repo_service
        .repo_snapshot(&repo_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("repo", &repo_id)),
    }
}

async fn update_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Json(request): Json<UpdateRepoRequest>,
) -> Result<Json<RepoSnapshot>, (StatusCode, String)> {
    match state
        .repo_service
        .update_repo(&repo_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("repo", &repo_id)),
    }
}

#[derive(serde::Deserialize)]
struct FilesQuery {
    path: Option<String>,
}

#[derive(serde::Deserialize)]
struct RawFileQuery {
    path: String,
}

#[derive(serde::Deserialize)]
struct CommitsQuery {
    limit: Option<usize>,
}

async fn list_branches(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
) -> Result<Json<Vec<BranchSnapshot>>, (StatusCode, String)> {
    let service = state.repo_service.clone();
    let id = repo_id.clone();
    match service.list_branches(&id).await.map_err(bad_request)? {
        Some(branches) => Ok(Json(branches)),
        None => Err(not_found("repo", &repo_id)),
    }
}

async fn list_repo_files(
    State(state): State<AppState>,
    Path((repo_id, branch)): Path<(String, String)>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<Vec<FileEntrySnapshot>>, (StatusCode, String)> {
    let service = state.repo_service.clone();
    let dir = query.path.unwrap_or_default();
    let (id, branch_param) = (repo_id.clone(), branch);
    match service
        .list_files(&id, &branch_param, &dir)
        .await
        .map_err(bad_request)?
    {
        Some(entries) => Ok(Json(entries)),
        None => Err(not_found("repo", &repo_id)),
    }
}

async fn read_repo_file(
    State(state): State<AppState>,
    Path((repo_id, branch)): Path<(String, String)>,
    Query(query): Query<RawFileQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let service = state.repo_service.clone();
    let (id, branch_param, path) = (repo_id.clone(), branch, query.path);
    match service
        .read_file(&id, &branch_param, &path)
        .await
        .map_err(bad_request)?
    {
        Some(bytes) => Ok((
            [(header::CONTENT_TYPE, "application/octet-stream")],
            Bytes::from(bytes),
        )),
        None => Err(not_found("repo", &repo_id)),
    }
}

async fn list_repo_commits(
    State(state): State<AppState>,
    Path((repo_id, branch)): Path<(String, String)>,
    Query(query): Query<CommitsQuery>,
) -> Result<Json<Vec<CommitSnapshot>>, (StatusCode, String)> {
    let service = state.repo_service.clone();
    let limit = query.limit.unwrap_or(20).clamp(1, 1000);
    let (id, branch_param) = (repo_id.clone(), branch);
    match service
        .recent_commits(&id, &branch_param, limit)
        .await
        .map_err(bad_request)?
    {
        Some(commits) => Ok(Json(commits)),
        None => Err(not_found("repo", &repo_id)),
    }
}

async fn create_commit(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CommitChangesRequest>,
) -> Result<Json<CommitSnapshot>, (StatusCode, String)> {
    require_repo_role(&state, &repo_id, Operation::Write, &headers).await?;
    let service = state.repo_service.clone();
    let id = repo_id.clone();
    match service
        .commit_changes(&id, request)
        .await
        .map_err(bad_request)?
    {
        Some(commit) => Ok(Json(commit)),
        None => Err(not_found("repo", &repo_id)),
    }
}

/// Extracts the password field of an `Authorization: Basic` header — git sends the
/// access token there.
pub(crate) fn basic_auth_password(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    decoded.split_once(':').map(|(_, pass)| pass.to_owned())
}

/// The request's access token, from `Authorization: Bearer <token>` (console) or the
/// password field of HTTP Basic (git).
fn request_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        && let Some(token) = value.strip_prefix("Bearer ")
    {
        return Some(token.to_owned());
    }
    basic_auth_password(headers)
}

const SESSION_COOKIE: &str = "wiab_session";
/// 7 days — matches the absolute session cap in the auth service.
const SESSION_MAX_AGE_SECONDS: i64 = 604_800;

/// The session cookie secret from the request's `Cookie` header, if present.
fn session_cookie_value(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_owned())
}

/// `Set-Cookie` value installing the session cookie: `HttpOnly` (no JS access),
/// `SameSite=Lax` (blocks cross-site state-changing requests — the current CSRF defense),
/// `Path=/`, and `Secure` when served over https.
fn set_session_cookie(secret: &str, secure: bool) -> String {
    let mut cookie = format!(
        "{SESSION_COOKIE}={secret}; HttpOnly; SameSite=Lax; Path=/; Max-Age={SESSION_MAX_AGE_SECONDS}"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn clear_session_cookie(secure: bool) -> String {
    let mut cookie = format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

const CSRF_COOKIE: &str = "wiab_csrf";
const CSRF_HEADER: &str = "x-csrf-token";

/// `Set-Cookie` for the CSRF token. Deliberately **not** `HttpOnly` — the SPA reads it and
/// echoes it in the `X-CSRF-Token` header (double-submit). It is not a session secret, and
/// it persists across reloads so the SPA still has it after a refresh.
fn set_csrf_cookie(token: &str, secure: bool) -> String {
    let mut cookie =
        format!("{CSRF_COOKIE}={token}; SameSite=Lax; Path=/; Max-Age={SESSION_MAX_AGE_SECONDS}");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn clear_csrf_cookie(secure: bool) -> String {
    let mut cookie = format!("{CSRF_COOKIE}=; SameSite=Lax; Path=/; Max-Age=0");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Identity-establishing endpoints can't carry a prior session's CSRF token (the caller has
/// no session yet, or is recovering access via a single-use token / credentials), so they are
/// exempt. Every other cookie-authenticated, state-changing request must present a valid token.
fn csrf_exempt(method: &Method, path: &str) -> bool {
    *method == Method::POST
        && matches!(
            path,
            "/auth/session"
                | "/auth/signup"
                | "/auth/password/reset/request"
                | "/auth/password/reset/confirm"
                | "/auth/invite/accept"
                | "/auth/verify-email"
        )
}

/// Enforces double-submit CSRF on cookie-authenticated, state-changing requests. Bearer/Basic
/// callers (PATs, git, agents) are exempt — they send no cookie and aren't CSRF-prone. Safe
/// methods and the identity-establishing endpoints are exempt. The SPA echoes the readable
/// `wiab_csrf` cookie in `X-CSRF-Token`; it must hash to the session's stored CSRF hash.
async fn csrf_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let is_unsafe = !matches!(
        request.method().as_str(),
        "GET" | "HEAD" | "OPTIONS" | "TRACE"
    );
    if is_unsafe
        && request_token(request.headers()).is_none()
        && !csrf_exempt(request.method(), request.uri().path())
        && let Some(secret) = session_cookie_value(request.headers())
        && let Some(resolved) = state
            .auth_service
            .resolve_session(&secret)
            .await
            .map_err(internal)?
    {
        let presented = request
            .headers()
            .get(CSRF_HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !state.auth_service.csrf_matches(&resolved, presented) {
            return Err((
                StatusCode::FORBIDDEN,
                "missing or invalid CSRF token".to_owned(),
            ));
        }
    }
    Ok(next.run(request).await)
}

/// Resolves the request to a user and its scope, or 401.
///
/// Precedence: a Bearer/Basic token first (console PATs, git, agents) — an explicit,
/// scoped, CSRF-immune credential that machine clients always send. Otherwise a browser
/// session cookie, which carries the user's full authority (unrestricted scope, like SSH
/// key auth). Git and machine paths never reach the cookie branch, so they are unchanged.
async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(UserId, TokenScope), (StatusCode, String)> {
    if let Some(token) = request_token(headers) {
        return match state
            .user_service
            .resolve_token(&token)
            .await
            .map_err(internal)?
        {
            Some(resolved) => Ok(resolved),
            None => Err(unauthorized()),
        };
    }
    if let Some(secret) = session_cookie_value(headers)
        && let Some(resolved) = state
            .auth_service
            .resolve_session(&secret)
            .await
            .map_err(internal)?
    {
        let user_id: UserId = resolved.principal.as_str().parse().map_err(internal)?;
        return Ok((user_id, TokenScope::unrestricted()));
    }
    Err(unauthorized())
}

/// Requires the caller be an Owner — the bar for managing users and grants.
async fn require_owner(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<UserId, (StatusCode, String)> {
    let (user, _scope) = authenticate(state, headers).await?;
    let access = state.access_service.clone();
    let is_owner = access.is_owner(user).await.map_err(internal)?;
    if is_owner { Ok(user) } else { Err(forbidden()) }
}

/// Requires the caller hold a sufficient org-level role for the operation (e.g. creating
/// a repo).
async fn require_org_role(
    state: &AppState,
    org_id: &str,
    operation: Operation,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
    let (user, _scope) = authenticate(state, headers).await?;
    let org: OrganizationId = org_id
        .parse()
        .map_err(|_| not_found("organization", org_id))?;
    let authz = state.authorization_service.clone();
    let allowed = authz
        .authorize_org(user, org, operation)
        .await
        .map_err(internal)?;
    if allowed { Ok(()) } else { Err(forbidden()) }
}

/// Requires the caller hold a sufficient role on the repo for the operation (token scope
/// applied).
async fn require_repo_role(
    state: &AppState,
    repo_id: &str,
    operation: Operation,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
    let (user, scope) = authenticate(state, headers).await?;
    let repo: RepoId = repo_id.parse().map_err(|_| not_found("repo", repo_id))?;
    let authz = state.authorization_service.clone();
    let allowed = authz
        .authorize(user, repo, operation, Some(&scope))
        .await
        .map_err(internal)?;
    if allowed { Ok(()) } else { Err(forbidden()) }
}

fn unauthorized() -> (StatusCode, String) {
    (
        StatusCode::UNAUTHORIZED,
        "authentication required".to_owned(),
    )
}

fn forbidden() -> (StatusCode, String) {
    (StatusCode::FORBIDDEN, "insufficient permissions".to_owned())
}

fn internal(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

/// Allows the caller if they are the target user or an Owner — for managing a user's own
/// keys and tokens.
async fn require_self_or_owner(
    state: &AppState,
    headers: &HeaderMap,
    target_user: &str,
) -> Result<(), (StatusCode, String)> {
    let (user, _scope) = authenticate(state, headers).await?;
    if user.to_string() == target_user {
        return Ok(());
    }
    let access = state.access_service.clone();
    let is_owner = access.is_owner(user).await.map_err(internal)?;
    if is_owner { Ok(()) } else { Err(forbidden()) }
}

#[derive(serde::Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(serde::Serialize)]
struct CurrentUser {
    id: String,
    name: String,
    email: Option<String>,
    is_owner: bool,
}

#[derive(serde::Serialize)]
struct LoginResponse {
    user: CurrentUser,
    /// Double-submit CSRF token the SPA echoes in `X-CSRF-Token` on unsafe requests; also
    /// set as the readable `wiab_csrf` cookie. `csrf_guard` enforces it on cookie-authed writes.
    csrf_token: String,
}

#[derive(serde::Serialize)]
struct AuthConfigResponse {
    local_password: bool,
    signup: bool,
    google: bool,
    oidc: bool,
}

async fn current_user(
    state: &AppState,
    user_id: UserId,
) -> Result<CurrentUser, (StatusCode, String)> {
    let snapshot = state
        .user_service
        .user_snapshot(&user_id.to_string())
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("user", &user_id.to_string()))?;
    let is_owner = state
        .access_service
        .is_owner(user_id)
        .await
        .map_err(internal)?;
    Ok(CurrentUser {
        id: snapshot.id,
        name: snapshot.name,
        email: snapshot.email,
        is_owner,
    })
}

/// Local email/password login: verifies credentials, establishes a session, sets the
/// cookie, and returns the current user plus a CSRF token.
async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let established = state
        .auth_service
        .login_with_password(&request.email, &request.password)
        .await
        .map_err(|error| match error {
            AuthError::InvalidCredentials => unauthorized(),
            other => internal(other),
        })?;
    let user_id = state
        .user_service
        .find_by_email(&request.email)
        .await
        .map_err(internal)?
        .ok_or_else(unauthorized)?;
    let user = current_user(&state, user_id).await?;
    let csrf_cookie = set_csrf_cookie(&established.csrf_token, state.auth_settings.cookie_secure);
    let cookie = set_session_cookie(
        &established.cookie_secret,
        state.auth_settings.cookie_secure,
    );
    Ok((
        AppendHeaders([
            (header::SET_COOKIE, cookie),
            (header::SET_COOKIE, csrf_cookie),
        ]),
        Json(LoginResponse {
            user,
            csrf_token: established.csrf_token,
        }),
    ))
}

/// The currently-authenticated user (resolved from session cookie or bearer token).
async fn whoami(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CurrentUser>, (StatusCode, String)> {
    let (user_id, _scope) = authenticate(&state, &headers).await?;
    Ok(Json(current_user(&state, user_id).await?))
}

/// Revoke the current session and clear the cookie. Idempotent.
async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if let Some(secret) = session_cookie_value(&headers) {
        state.auth_service.logout(&secret).await.map_err(internal)?;
    }
    let cookie = clear_session_cookie(state.auth_settings.cookie_secure);
    let csrf_cookie = clear_csrf_cookie(state.auth_settings.cookie_secure);
    Ok((
        AppendHeaders([
            (header::SET_COOKIE, cookie),
            (header::SET_COOKIE, csrf_cookie),
        ]),
        StatusCode::NO_CONTENT,
    ))
}

#[derive(serde::Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

/// Change the current user's password (re-verifies the current one). Self-service.
async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let (user_id, _scope) = authenticate(&state, &headers).await?;
    if request.new_password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "password must be at least 8 characters".to_owned(),
        ));
    }
    state
        .auth_service
        .change_password(
            PrincipalId::new(user_id.to_string()),
            &request.current_password,
            &request.new_password,
        )
        .await
        .map_err(|error| match error {
            AuthError::InvalidCredentials => (
                StatusCode::BAD_REQUEST,
                "current password is incorrect".to_owned(),
            ),
            other => internal(other),
        })?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
struct PasswordResetRequestBody {
    email: String,
}

/// Request a password-reset link. Always returns 202 (no account-existence disclosure).
async fn password_reset_request(
    State(state): State<AppState>,
    Json(request): Json<PasswordResetRequestBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .password_reset_service
        .request(&request.email)
        .await
        .map_err(internal)?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(serde::Deserialize)]
struct PasswordResetConfirmBody {
    token: String,
    new_password: String,
}

/// Set a new password using a reset token from the emailed link.
async fn password_reset_confirm(
    State(state): State<AppState>,
    Json(request): Json<PasswordResetConfirmBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    if request.new_password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "password must be at least 8 characters".to_owned(),
        ));
    }
    state
        .password_reset_service
        .confirm(&request.token, &request.new_password)
        .await
        .map_err(|error| match error {
            AuthError::InvalidCredentials => (
                StatusCode::BAD_REQUEST,
                "this reset link is invalid or has expired".to_owned(),
            ),
            other => internal(other),
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// Which login methods the SPA should offer.
async fn auth_config(State(state): State<AppState>) -> Json<AuthConfigResponse> {
    Json(AuthConfigResponse {
        local_password: true,
        signup: state.auth_settings.signup_enabled,
        google: state.auth_settings.google_enabled,
        oidc: state.auth_settings.oidc_enabled,
    })
}

/// Only same-origin relative paths are accepted as a post-login destination (no open
/// redirect); anything else falls back to the console home.
fn sanitize_return_to(next: Option<&str>) -> String {
    match next {
        Some(value) if value.starts_with('/') && !value.starts_with("//") => value.to_owned(),
        _ => "/works".to_owned(),
    }
}

#[derive(serde::Deserialize)]
struct OidcStartQuery {
    next: Option<String>,
}

/// Begin an OIDC login: redirect the browser to the IdP's authorization endpoint.
async fn oidc_start(
    State(state): State<AppState>,
    Path(connection): Path<String>,
    Query(query): Query<OidcStartQuery>,
) -> Result<axum::response::Redirect, (StatusCode, String)> {
    let Some(federation) = state.federation_service.clone() else {
        return Err((
            StatusCode::NOT_FOUND,
            "federation is not enabled".to_owned(),
        ));
    };
    let return_to = sanitize_return_to(query.next.as_deref());
    let url = federation
        .begin_login(&connection, &return_to)
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
    Ok(axum::response::Redirect::to(&url))
}

#[derive(serde::Deserialize)]
struct OidcCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Complete an OIDC login from the IdP callback: validate, resolve/provision the user,
/// establish a session, and redirect to where the user was headed.
async fn oidc_callback(
    State(state): State<AppState>,
    Path(connection): Path<String>,
    Query(query): Query<OidcCallbackQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let Some(federation) = state.federation_service.clone() else {
        return Err((
            StatusCode::NOT_FOUND,
            "federation is not enabled".to_owned(),
        ));
    };
    if let Some(error) = query.error {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("identity provider error: {error}"),
        ));
    }
    let (Some(code), Some(state_param)) = (query.code, query.state) else {
        return Err((StatusCode::BAD_REQUEST, "missing code or state".to_owned()));
    };
    let (principal, return_to) = federation
        .complete_login(&connection, &state_param, &code)
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error.to_string()))?;
    let established = state
        .auth_service
        .establish_session(principal)
        .await
        .map_err(internal)?;
    let csrf_cookie = set_csrf_cookie(&established.csrf_token, state.auth_settings.cookie_secure);
    let cookie = set_session_cookie(
        &established.cookie_secret,
        state.auth_settings.cookie_secure,
    );
    Ok((
        AppendHeaders([
            (header::SET_COOKIE, cookie),
            (header::SET_COOKIE, csrf_cookie),
        ]),
        axum::response::Redirect::to(&return_to),
    ))
}

#[derive(serde::Deserialize)]
struct InviteUserRequest {
    email: String,
    name: String,
}

/// Owner-only: create a pending user and email them an invite link to set a password.
async fn invite_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<InviteUserRequest>,
) -> Result<Json<UserSnapshot>, (StatusCode, String)> {
    require_owner(&state, &headers).await?;
    let snapshot = state
        .user_service
        .create_pending_user(request.name, request.email.clone())
        .await
        .map_err(bad_request)?;
    state
        .invitation_service
        .invite(&request.email, PrincipalId::new(snapshot.id.clone()))
        .await
        .map_err(internal)?;
    Ok(Json(snapshot))
}

/// Owner-only: deactivate a user (bars login) and drop their sessions immediately.
async fn deactivate_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<UserSnapshot>, (StatusCode, String)> {
    require_owner(&state, &headers).await?;
    let snapshot = state
        .user_service
        .deactivate_user(&user_id)
        .await
        .map_err(bad_request)?
        .ok_or_else(|| not_found("user", &user_id))?;
    state
        .auth_service
        .revoke_all_sessions(&PrincipalId::new(user_id))
        .await
        .map_err(internal)?;
    Ok(Json(snapshot))
}

/// Owner-only: re-activate a deactivated user.
async fn activate_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<UserSnapshot>, (StatusCode, String)> {
    require_owner(&state, &headers).await?;
    let snapshot = state
        .user_service
        .activate_user(&user_id)
        .await
        .map_err(bad_request)?
        .ok_or_else(|| not_found("user", &user_id))?;
    Ok(Json(snapshot))
}

#[derive(serde::Deserialize)]
struct SignupRequest {
    email: String,
    password: String,
    name: String,
}

/// Self-service signup (off unless `WIAB_AUTH_LOCAL_SIGNUP`): create a pending user with a
/// password and email a verification link. Always returns 202 (no account-existence leak).
async fn signup(
    State(state): State<AppState>,
    Json(request): Json<SignupRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !state.auth_settings.signup_enabled {
        return Err((
            StatusCode::FORBIDDEN,
            "self-service signup is disabled".to_owned(),
        ));
    }
    if request.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "password must be at least 8 characters".to_owned(),
        ));
    }
    // A taken email is not reported — same 202 either way.
    if let Ok(snapshot) = state
        .user_service
        .create_pending_user(request.name, request.email.clone())
        .await
    {
        let principal = PrincipalId::new(snapshot.id);
        state
            .auth_service
            .set_password(principal.clone(), &request.password)
            .await
            .map_err(internal)?;
        state
            .invitation_service
            .send_email_verification(&request.email, principal)
            .await
            .map_err(internal)?;
    }
    Ok(StatusCode::ACCEPTED)
}

#[derive(serde::Deserialize)]
struct AcceptInviteRequest {
    token: String,
    password: String,
}

/// Accept an invite: set the password, activate the user, and sign them in.
async fn accept_invite(
    State(state): State<AppState>,
    Json(request): Json<AcceptInviteRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if request.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "password must be at least 8 characters".to_owned(),
        ));
    }
    let principal = state
        .invitation_service
        .accept_invite(&request.token, &request.password)
        .await
        .map_err(invite_error)?;
    finish_activation(&state, principal).await
}

#[derive(serde::Deserialize)]
struct VerifyEmailRequest {
    token: String,
}

/// Confirm a signup email: activate the user and sign them in (password already set).
async fn verify_email(
    State(state): State<AppState>,
    Json(request): Json<VerifyEmailRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let principal = state
        .invitation_service
        .verify_email(&request.token)
        .await
        .map_err(invite_error)?;
    finish_activation(&state, principal).await
}

fn invite_error(error: AuthError) -> (StatusCode, String) {
    match error {
        AuthError::InvalidCredentials => (
            StatusCode::BAD_REQUEST,
            "this link is invalid or has expired".to_owned(),
        ),
        other => internal(other),
    }
}

/// Activate the principal's user and establish a session (shared by invite-accept and
/// email-verify, which both finish by signing the user in).
async fn finish_activation(
    state: &AppState,
    principal: PrincipalId,
) -> Result<
    (
        AppendHeaders<[(axum::http::HeaderName, String); 2]>,
        Json<LoginResponse>,
    ),
    (StatusCode, String),
> {
    state
        .user_service
        .activate_user(principal.as_str())
        .await
        .map_err(internal)?;
    let user_id: UserId = principal.as_str().parse().map_err(internal)?;
    let established = state
        .auth_service
        .establish_session(principal)
        .await
        .map_err(internal)?;
    let user = current_user(state, user_id).await?;
    let csrf_cookie = set_csrf_cookie(&established.csrf_token, state.auth_settings.cookie_secure);
    let cookie = set_session_cookie(
        &established.cookie_secret,
        state.auth_settings.cookie_secure,
    );
    Ok((
        AppendHeaders([
            (header::SET_COOKIE, cookie),
            (header::SET_COOKIE, csrf_cookie),
        ]),
        Json(LoginResponse {
            user,
            csrf_token: established.csrf_token,
        }),
    ))
}

async fn set_repo_visibility(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<SetVisibilityRequest>,
) -> Result<Json<RepoSnapshot>, (StatusCode, String)> {
    require_repo_role(&state, &repo_id, Operation::Administer, &headers).await?;
    let service = state.repo_service.clone();
    let id = repo_id.clone();
    match service
        .set_visibility(&id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("repo", &repo_id)),
    }
}

async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<UserSnapshot>>, (StatusCode, String)> {
    require_owner(&state, &headers).await?;
    let service = state.user_service.clone();
    let users = service.list_users().await.map_err(internal)?;
    Ok(Json(users))
}

async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateUserRequest>,
) -> Result<Json<UserSnapshot>, (StatusCode, String)> {
    require_owner(&state, &headers).await?;
    let service = state.user_service.clone();
    let snapshot = service.create_user(request).await.map_err(bad_request)?;
    Ok(Json(snapshot))
}

async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<UserSnapshot>, (StatusCode, String)> {
    require_self_or_owner(&state, &headers, &user_id).await?;
    let service = state.user_service.clone();
    let id = user_id.clone();
    match service.user_snapshot(&id).await.map_err(bad_request)? {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("user", &user_id)),
    }
}

async fn add_ssh_key(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<AddSshKeyRequest>,
) -> Result<Json<UserSnapshot>, (StatusCode, String)> {
    require_self_or_owner(&state, &headers, &user_id).await?;
    let service = state.user_service.clone();
    let id = user_id.clone();
    match service
        .add_ssh_key(&id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("user", &user_id)),
    }
}

async fn remove_ssh_key(
    State(state): State<AppState>,
    Path((user_id, key_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<UserSnapshot>, (StatusCode, String)> {
    require_self_or_owner(&state, &headers, &user_id).await?;
    let service = state.user_service.clone();
    let id = user_id.clone();
    match service
        .remove_ssh_key(&id, &key_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("user", &user_id)),
    }
}

async fn issue_token(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<IssueTokenRequest>,
) -> Result<Json<IssuedTokenSnapshot>, (StatusCode, String)> {
    require_self_or_owner(&state, &headers, &user_id).await?;
    let service = state.user_service.clone();
    let id = user_id.clone();
    match service
        .issue_token(&id, request)
        .await
        .map_err(bad_request)?
    {
        Some(issued) => Ok(Json(issued)),
        None => Err(not_found("user", &user_id)),
    }
}

async fn revoke_token(
    State(state): State<AppState>,
    Path((user_id, token_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<UserSnapshot>, (StatusCode, String)> {
    require_self_or_owner(&state, &headers, &user_id).await?;
    let service = state.user_service.clone();
    let id = user_id.clone();
    match service
        .revoke_token(&id, &token_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("user", &user_id)),
    }
}

async fn list_role_assignments(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RoleAssignmentSnapshot>>, (StatusCode, String)> {
    require_owner(&state, &headers).await?;
    let service = state.access_service.clone();
    let assignments = service.list_assignments().await.map_err(internal)?;
    Ok(Json(assignments))
}

async fn grant_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<GrantRoleRequest>,
) -> Result<Json<RoleAssignmentSnapshot>, (StatusCode, String)> {
    require_owner(&state, &headers).await?;
    let service = state.access_service.clone();
    match service.grant(request).await.map_err(bad_request)? {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("user", "grantee")),
    }
}

async fn revoke_role(
    State(state): State<AppState>,
    Path(assignment_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, String)> {
    require_owner(&state, &headers).await?;
    let service = state.access_service.clone();
    let id = assignment_id.clone();
    let removed = service.revoke(&id).await.map_err(bad_request)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found("role assignment", &assignment_id))
    }
}

async fn list_pipelines(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<PipelineSnapshot>>, (StatusCode, String)> {
    match state
        .pipeline_service
        .list_pipelines(&project_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshots) => Ok(Json(snapshots)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn create_pipeline(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(request): Json<CreatePipelineRequest>,
) -> Result<Json<PipelineSnapshot>, (StatusCode, String)> {
    match state
        .pipeline_service
        .create_pipeline(&project_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn get_pipeline(
    State(state): State<AppState>,
    Path(pipeline_id): Path<String>,
) -> Result<Json<PipelineSnapshot>, (StatusCode, String)> {
    match state
        .pipeline_service
        .pipeline_snapshot(&pipeline_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("pipeline", &pipeline_id)),
    }
}

async fn update_pipeline(
    State(state): State<AppState>,
    Path(pipeline_id): Path<String>,
    Json(request): Json<UpdatePipelineRequest>,
) -> Result<Json<PipelineSnapshot>, (StatusCode, String)> {
    match state
        .pipeline_service
        .update_pipeline(&pipeline_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("pipeline", &pipeline_id)),
    }
}

async fn list_project_works(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<WorkSnapshot>>, (StatusCode, String)> {
    match state
        .work_service
        .list_works_by_project(&project_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshots) => Ok(Json(snapshots)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn create_work(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(request): Json<CreateWorkRequest>,
) -> Result<Json<WorkSnapshot>, (StatusCode, String)> {
    match state
        .work_service
        .create_work(&project_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn get_work(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> Result<Json<WorkSnapshot>, (StatusCode, String)> {
    match state
        .work_service
        .work_snapshot(&work_id)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("work", &work_id)),
    }
}

async fn update_work(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(request): Json<UpdateWorkRequest>,
) -> Result<Json<WorkSnapshot>, (StatusCode, String)> {
    match state
        .work_service
        .update_work(&work_id, request)
        .await
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("work", &work_id)),
    }
}

async fn add_done(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(request): Json<AddDoneRequest>,
) -> Result<Json<WorkSnapshot>, (StatusCode, String)> {
    state
        .work_service
        .add_done(&work_id, request)
        .await
        .map(Json)
        .map_err(bad_request)
}

async fn fulfill_done(
    State(state): State<AppState>,
    Path((work_id, done_id)): Path<(String, String)>,
) -> Result<Json<WorkSnapshot>, (StatusCode, String)> {
    state
        .work_service
        .fulfill_done(&work_id, &done_id)
        .await
        .map(Json)
        .map_err(bad_request)
}

async fn unfulfill_done(
    State(state): State<AppState>,
    Path((work_id, done_id)): Path<(String, String)>,
) -> Result<Json<WorkSnapshot>, (StatusCode, String)> {
    state
        .work_service
        .unfulfill_done(&work_id, &done_id)
        .await
        .map(Json)
        .map_err(bad_request)
}

fn bad_request(err: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, err.to_string())
}

fn not_found(what: &str, id: &str) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, format!("{what} '{id}' not found"))
}

async fn signal(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_signal_socket(state.sfu, socket))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csrf_exemptions_cover_only_identity_establishing_posts() {
        // Exempt: endpoints that establish/recover identity without a prior session.
        for path in [
            "/auth/session",
            "/auth/signup",
            "/auth/password/reset/request",
            "/auth/password/reset/confirm",
            "/auth/invite/accept",
            "/auth/verify-email",
        ] {
            assert!(csrf_exempt(&Method::POST, path), "{path} should be exempt");
        }
        // Authenticated mutations must carry a CSRF token — never exempt.
        assert!(!csrf_exempt(&Method::PUT, "/auth/password"));
        assert!(!csrf_exempt(&Method::DELETE, "/auth/session"));
        assert!(!csrf_exempt(&Method::POST, "/users"));
        // The exemption is POST-only.
        assert!(!csrf_exempt(&Method::GET, "/auth/session"));
    }
}
