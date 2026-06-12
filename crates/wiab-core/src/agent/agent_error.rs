use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentError {
    #[error("agent name must be a non-empty trimmed string")]
    EmptyName,
    #[error("'{0}' is not a valid agent id")]
    InvalidAgentId(String),
}
