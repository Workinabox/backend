use axum::{
    Json, Router,
    extract::{State, ws::WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use wiab_app::CreateMeetingRequest;
use wiab_core::meeting::MeetingSnapshot;

use crate::{AppState, handle_signal_socket};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/meetings", get(list_meetings).post(create_meeting))
        .route("/signal", get(signal))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
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

async fn signal(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_signal_socket(state.sfu, socket))
}
