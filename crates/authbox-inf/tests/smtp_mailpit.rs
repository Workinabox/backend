//! Integration test: `SmtpEmailSender` actually delivers to a real SMTP server (Mailpit).
//!
//! Ignored by default — it needs a Mailpit container. Locally:
//!   docker run -d --rm -p 1025:1025 -p 8025:8025 axllent/mailpit
//!   cargo test -p authbox-inf --test smtp_mailpit -- --ignored
//! In CI the host/port/API are overridden via `MAILPIT_SMTP_HOST`, `MAILPIT_SMTP_PORT`,
//! and `MAILPIT_API` (which point at the `mailpit` service container).

use authbox_core::auth::EmailSender;
use authbox_inf::SmtpEmailSender;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

#[tokio::test]
#[ignore = "requires a Mailpit container (set MAILPIT_* or run on localhost:1025/:8025)"]
async fn delivers_a_reset_email_to_mailpit() {
    let smtp_host = env_or("MAILPIT_SMTP_HOST", "localhost");
    let smtp_port: u16 = env_or("MAILPIT_SMTP_PORT", "1025")
        .parse()
        .expect("smtp port");
    let api = env_or("MAILPIT_API", "http://localhost:8025");
    let http = reqwest::Client::new();

    // Start from an empty mailbox.
    let _ = http.delete(format!("{api}/api/v1/messages")).send().await;

    let sender = SmtpEmailSender::new(
        &smtp_host,
        smtp_port,
        None,
        None,
        "no-reply@workinabox.local",
        false,
    )
    .expect("build smtp sender");

    sender
        .send(
            "ada@example.com",
            "Reset your password",
            "Reset it here:\nhttps://app.example/reset-password?token=ABC123\n",
        )
        .expect("send email");

    let mailbox = http
        .get(format!("{api}/api/v1/messages"))
        .send()
        .await
        .expect("query mailpit")
        .text()
        .await
        .expect("read mailpit response");

    assert!(
        mailbox.contains("ada@example.com"),
        "recipient should be in the Mailpit inbox, got: {mailbox}"
    );
    assert!(
        mailbox.contains("Reset your password"),
        "subject should be in the Mailpit inbox, got: {mailbox}"
    );
}
