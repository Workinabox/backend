use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePipelineRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePipelineRequest {
    pub name: String,
    pub description: String,
}
