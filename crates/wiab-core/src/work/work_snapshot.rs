use serde::{Deserialize, Serialize};

use crate::work::DoneView;

/// Serializable read view of a `Work` and its subtree. HTTP responses use this rather than
/// the domain type. `is_done` is computed per node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkSnapshot {
    pub id: String,
    pub title: String,
    pub description: String,
    pub dones: Vec<DoneView>,
    pub children: Vec<WorkSnapshot>,
    pub is_done: bool,
}
