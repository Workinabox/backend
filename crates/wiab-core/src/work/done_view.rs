use serde::{Deserialize, Serialize};

/// Serializable read view of a `Done`. HTTP responses use this rather than the domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoneView {
    pub id: String,
    pub criterion: String,
    pub fulfilled: bool,
}
