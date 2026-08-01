use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTaskRequest {
    /// The work item to queue on the board.
    pub work_id: String,
}

/// Body for the transitions that must say why: block, escalate and fail.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskReasonRequest {
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaimTaskRequest {
    /// The team taking the task.
    pub team_id: String,
}
