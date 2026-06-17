use authbox_core::auth::{AuthError, EmailSender};
use tracing::info;

/// Dev `EmailSender` that logs the message instead of delivering it — the reset link shows
/// up in the server log. Used when no SMTP host is configured.
pub struct LoggingEmailSender;

impl EmailSender for LoggingEmailSender {
    fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), AuthError> {
        info!(target: "authbox::email", "to={to} subject={subject:?}\n{body}");
        Ok(())
    }
}
