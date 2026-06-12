use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: String,
    pub description: String,
}
