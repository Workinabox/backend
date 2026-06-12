use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BoardError {
    #[error("board name must be a non-empty trimmed string")]
    EmptyName,
    #[error("'{0}' is not a valid board id")]
    InvalidBoardId(String),
}
