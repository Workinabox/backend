use authbox_core::auth::{AuthError, EmailSender};
use tracing::info;

/// Dev `EmailSender` that records that a message would have been delivered, instead of
/// delivering it. Used when no mailer is configured.
///
/// The body is deliberately not logged: it carries the single-use reset/invite/verify link,
/// and this sender is the *fallback* — it is selected by a missing credential, so it can end
/// up chosen in a shared environment by accident. A token in the log is an account takeover
/// for anyone with log access, for as long as the token lives.
pub struct LoggingEmailSender;

impl EmailSender for LoggingEmailSender {
    fn send(&self, to: &str, subject: &str, _body: &str) -> Result<(), AuthError> {
        info!(target: "authbox::email", "would send to={to} subject={subject:?} (body withheld: it contains a single-use link)");
        Ok(())
    }
}
