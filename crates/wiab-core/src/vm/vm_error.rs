use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VmError {
    #[error("vm template must be a non-empty trimmed string")]
    EmptyTemplate,
    #[error(
        "'{0}' is not a valid vm template name (lowercase letters, digits, '.', '_' and '-', \
         starting with a letter or digit)"
    )]
    InvalidTemplate(String),
    #[error("'{0}' is not a valid vm id")]
    InvalidVmId(String),
    #[error("'{0}' is not an agent or team id, so no vm can be booted for it")]
    InvalidVmOwner(String),
    #[error("'{0}' is not a valid vm state")]
    InvalidVmState(String),
    #[error("a vm can only start running from the Creating state")]
    NotCreating,
    #[error("a vm can only be stopped from the Running state")]
    NotRunning,
}
