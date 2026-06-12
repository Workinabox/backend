use serde::{Deserialize, Serialize};

/// Serializable read view of an `Organization`. HTTP responses use this rather than the
/// domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationSnapshot {
    pub id: String,
    pub name: String,
    pub description: String,
}
