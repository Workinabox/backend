use authbox_core::auth::{AuthError, EmailSender};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

/// SMTP `EmailSender` over lettre's blocking transport. TLS (rustls) when `use_tls`,
/// plaintext otherwise — e.g. a local Mailpit on `:1025`.
pub struct SmtpEmailSender {
    transport: SmtpTransport,
    from: Mailbox,
}

impl SmtpEmailSender {
    pub fn new(
        host: &str,
        port: u16,
        username: Option<String>,
        password: Option<String>,
        from: &str,
        use_tls: bool,
    ) -> anyhow::Result<Self> {
        let mut builder = if use_tls {
            SmtpTransport::relay(host)?
        } else {
            SmtpTransport::builder_dangerous(host)
        }
        .port(port);
        if let (Some(user), Some(pass)) = (username, password) {
            builder = builder.credentials(Credentials::new(user, pass));
        }
        Ok(Self {
            transport: builder.build(),
            from: from.parse()?,
        })
    }
}

impl EmailSender for SmtpEmailSender {
    fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), AuthError> {
        let message = Message::builder()
            .from(self.from.clone())
            .to(to
                .parse()
                .map_err(|error| AuthError::Backend(format!("invalid recipient: {error}")))?)
            .subject(subject)
            .body(body.to_owned())
            .map_err(|error| AuthError::Backend(error.to_string()))?;
        self.transport
            .send(&message)
            .map_err(|error| AuthError::Backend(error.to_string()))?;
        Ok(())
    }
}
