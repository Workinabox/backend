use axum::{
    Json, Router,
    extract::{Path, State, ws::WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
};
use wiab_app::{
    AddDoneRequest, CreateAgentRequest, CreateBoardRequest, CreateMeetingRequest,
    CreateOrganizationRequest, CreatePipelineRequest, CreateProjectRequest, CreateRepoRequest,
    CreateWorkRequest, UpdateAgentRequest, UpdateBoardRequest, UpdateOrganizationRequest,
    UpdatePipelineRequest, UpdateProjectRequest, UpdateRepoRequest, UpdateWorkRequest,
};
use wiab_core::agent::AgentSnapshot;
use wiab_core::board::BoardSnapshot;
use wiab_core::meeting::MeetingSnapshot;
use wiab_core::organization::OrganizationSnapshot;
use wiab_core::pipeline::PipelineSnapshot;
use wiab_core::project::ProjectSnapshot;
use wiab_core::repo::RepoSnapshot;
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
        .route(
            "/pipelines/{pipeline_id}",
            put(update_pipeline).get(get_pipeline),
        )
        .route("/works/{work_id}", get(get_work).put(update_work))
        .route("/works/{work_id}/children", post(add_child))
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

async fn list_meetings(State(state): State<AppState>) -> Json<Vec<MeetingSnapshot>> {
    Json(state.meeting_service.list_meetings())
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

async fn list_organizations(State(state): State<AppState>) -> Json<Vec<OrganizationSnapshot>> {
    Json(state.organization_service.list_organizations())
}

async fn create_organization(
    State(state): State<AppState>,
    Json(request): Json<CreateOrganizationRequest>,
) -> Result<Json<OrganizationSnapshot>, (StatusCode, String)> {
    state
        .organization_service
        .create_organization(request)
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
    match state
        .agent_service
        .create_agent(&organization_id, request)
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("organization", &organization_id)),
    }
}

async fn get_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentSnapshot>, (StatusCode, String)> {
    match state
        .agent_service
        .agent_snapshot(&agent_id)
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
        .map_err(bad_request)?
    {
        Some(snapshots) => Ok(Json(snapshots)),
        None => Err(not_found("project", &project_id)),
    }
}

async fn create_repo(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(request): Json<CreateRepoRequest>,
) -> Result<Json<RepoSnapshot>, (StatusCode, String)> {
    match state
        .repo_service
        .create_repo(&project_id, request)
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
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("repo", &repo_id)),
    }
}

async fn list_pipelines(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<PipelineSnapshot>>, (StatusCode, String)> {
    match state
        .pipeline_service
        .list_pipelines(&project_id)
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
        .map_err(bad_request)?
    {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(not_found("work", &work_id)),
    }
}

async fn add_child(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(request): Json<CreateWorkRequest>,
) -> Result<Json<WorkSnapshot>, (StatusCode, String)> {
    state
        .work_service
        .add_child(&work_id, request)
        .map(Json)
        .map_err(bad_request)
}

async fn add_done(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(request): Json<AddDoneRequest>,
) -> Result<Json<WorkSnapshot>, (StatusCode, String)> {
    state
        .work_service
        .add_done(&work_id, request)
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
