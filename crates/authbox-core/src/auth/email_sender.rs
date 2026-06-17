use crate::auth::AuthError;

/// Sends a transactional email (password reset, later: verification, invites).
///
/// Sync so it can be an injected `Arc<dyn EmailSender>`; callers run it inside
/// `spawn_blocking` because SMTP is blocking I/O. The dev impl just logs the message.
pub trait EmailSender: Send + Sync {
    fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), AuthError>;
}
