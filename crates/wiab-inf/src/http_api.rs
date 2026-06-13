use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
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
        .route(
            "/role-assignments",
            get(list_role_assignments).post(grant_role),
        )
        .route("/role-assignments/{assignment_id}", delete(revoke_role))
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

/// Resolves the request's token to a user and its scope, or 401.
async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(UserId, TokenScope), (StatusCode, String)> {
    let Some(token) = request_token(headers) else {
        return Err(unauthorized());
    };
    let user_service = state.user_service.clone();
    match user_service.resolve_token(&token).await.map_err(internal)? {
        Some(resolved) => Ok(resolved),
        None => Err(unauthorized()),
    }
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
