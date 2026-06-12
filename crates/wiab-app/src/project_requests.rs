use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: String,
    pub description: String,
}
