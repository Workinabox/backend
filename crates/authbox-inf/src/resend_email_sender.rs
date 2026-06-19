use authbox_core::auth::{AuthError, EmailSender};
use tracing::{info, warn};

/// `EmailSender` over the [Resend](https://resend.com) HTTP API: one authenticated POST per
/// email, no SMTP and no Basic-auth credentials.
///
/// The `EmailSender` port is synchronous, but an HTTP send is async, so `send` dispatches the
/// request onto the Tokio runtime (`Handle::spawn`) and returns immediately. Delivery is
/// best-effort — callers already treat email as non-blocking — and failures are logged.
///
/// `from` must be an address on a domain verified in Resend (or `onboarding@resend.dev` for
/// testing, which only delivers to your own account address).
pub struct ResendEmailSender {
    client: reqwest::Client,
    handle: tokio::runtime::Handle,
    api_key: String,
    from: String,
}

impl ResendEmailSender {
    /// Must be constructed within a Tokio runtime (it captures the current `Handle`).
    pub fn new(api_key: String, from: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            handle: tokio::runtime::Handle::current(),
            api_key,
            from,
        }
    }
}

impl EmailSender for ResendEmailSender {
    fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), AuthError> {
        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let payload = serde_json::json!({
            "from": self.from,
            "to": [to],
            "subject": subject,
            "text": body,
        });
        let to = to.to_owned();
        self.handle.spawn(async move {
            match client
                .post("https://api.resend.com/emails")
                .bearer_auth(&api_key)
                .json(&payload)
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    let body = response.text().await.unwrap_or_default();
                    info!("resend accepted email to {to}: {body}");
                }
                Ok(response) => {
                    let status = response.status();
                    let detail = response.text().await.unwrap_or_default();
                    warn!("resend rejected email to {to}: {status} {detail}");
                }
                Err(error) => warn!("resend request failed for {to}: {error}"),
            }
        });
        Ok(())
    }
}
