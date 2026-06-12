use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBoardRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBoardRequest {
    pub name: String,
    pub description: String,
}
