use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UserError {
    #[error("user name must be a non-empty trimmed string")]
    EmptyName,
    #[error("'{0}' is not a valid user id")]
    InvalidUserId(String),
    #[error("'{0}' is not a valid user kind")]
    InvalidUserKind(String),
    #[error("ssh key label must be a non-empty trimmed string")]
    EmptySshKeyLabel,
    #[error("ssh key must be a non-empty public key")]
    EmptySshKey,
    #[error("'{0}' is not a valid OpenSSH public key")]
    InvalidSshKey(String),
    #[error("'{0}' is not a valid ssh key id")]
    InvalidSshKeyId(String),
    #[error("no ssh key '{0}' on this user")]
    SshKeyNotFound(String),
    #[error("access token label must be a non-empty trimmed string")]
    EmptyTokenLabel,
    #[error("access token must carry a non-empty hash")]
    EmptyTokenHash,
    #[error("'{0}' is not a valid access token id")]
    InvalidTokenId(String),
    #[error("no access token '{0}' on this user")]
    TokenNotFound(String),
}
