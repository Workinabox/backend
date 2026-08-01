use serde::{Deserialize, Serialize};

/// Serializable read view of a `Task`. HTTP responses use this rather than the domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub id: String,
    pub board_id: String,
    pub work_id: String,
    pub state: String,
    /// The team holding the task; `None` while it waits on the board.
    pub assignee: Option<String>,
    /// Why the task is blocked, escalated or failed. `None` otherwise.
    pub reason: Option<String>,
}
