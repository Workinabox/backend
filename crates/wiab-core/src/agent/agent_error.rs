use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentError {
    #[error("agent name must be a non-empty trimmed string")]
    EmptyName,
    #[error("'{0}' is not a valid agent id")]
    InvalidAgentId(String),
    #[error("the agent has no VM type assigned")]
    NoVmTemplate,
    #[error("the agent is already active")]
    AlreadyActive,
    #[error("the agent is not active")]
    NotActive,
    #[error("the VM type cannot be changed while the agent is active")]
    ActiveTemplateChange,
}
