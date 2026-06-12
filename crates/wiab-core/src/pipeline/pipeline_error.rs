use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PipelineError {
    #[error("pipeline name must be a non-empty trimmed string")]
    EmptyName,
    #[error("'{0}' is not a valid pipeline id")]
    InvalidPipelineId(String),
}
