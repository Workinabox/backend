use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VmError {
    #[error("vm template must be a non-empty trimmed string")]
    EmptyTemplate,
    #[error("'{0}' is not a valid vm id")]
    InvalidVmId(String),
    #[error("'{0}' is not a valid vm state")]
    InvalidVmState(String),
    #[error("a vm can only start running from the Creating state")]
    NotCreating,
    #[error("a vm can only be stopped from the Running state")]
    NotRunning,
}
