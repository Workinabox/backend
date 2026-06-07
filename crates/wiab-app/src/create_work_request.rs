use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkRequest {
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddDoneRequest {
    pub criterion: String,
}
