use axum::{
    Json, Router,
    extract::{Path, State, ws::WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use wiab_app::{AddDoneRequest, CreateMeetingRequest, CreateWorkRequest};
use wiab_core::meeting::MeetingSnapshot;
use wiab_core::work::WorkSnapshot;

use crate::{AppState, handle_signal_socket};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/meetings", get(list_meetings).post(create_meeting))
        .route("/works", get(list_works).post(create_work))
        .route("/works/{work_id}", get(get_work))
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

async fn list_works(State(state): State<AppState>) -> Json<Vec<WorkSnapshot>> {
    Json(state.work_service.list_works())
}

async fn create_work(
    State(state): State<AppState>,
    Json(request): Json<CreateWorkRequest>,
) -> Result<Json<WorkSnapshot>, (StatusCode, String)> {
    state
        .work_service
        .create_work(request)
        .map(Json)
        .map_err(bad_request)
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
        None => Err((StatusCode::NOT_FOUND, format!("work '{work_id}' not found"))),
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

async fn signal(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_signal_socket(state.sfu, socket))
}
