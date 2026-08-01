use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub description: String,
    /// The board this team pulls its work from.
    pub board_id: String,
    /// The repo this team works in.
    pub repo_id: String,
    /// The VM type (template name) the team's sandbox runs. Required: a team with no
    /// template could never start.
    pub vm_type: String,
}
