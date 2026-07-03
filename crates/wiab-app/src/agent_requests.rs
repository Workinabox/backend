use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: String,
    /// The VM type (template name) to assign, if any.
    #[serde(default)]
    pub vm_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: String,
    pub description: String,
    /// The VM type (template name); `None` leaves it unchanged is NOT assumed — see the
    /// service: an explicit `null` clears it. Only applied while the agent is inactive.
    #[serde(default)]
    pub vm_type: Option<String>,
}
