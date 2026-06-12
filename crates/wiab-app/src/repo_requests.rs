use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRepoRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRepoRequest {
    pub name: String,
    pub description: String,
}
