use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateOrganizationRequest {
    pub name: String,
    pub description: String,
}
