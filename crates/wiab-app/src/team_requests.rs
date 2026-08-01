use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub description: String,
    /// The VM type (template name) the team's sandbox runs. Required: a team with no
    /// template could never start.
    pub vm_type: String,
}
