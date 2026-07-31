//! Enum dispatch over the [`Messaging`] port, mirroring `vm_runtime_dispatch`.
//!
//! NATS is opt-in (`WIAB_NATS_ENABLED`), so a developer or an existing deployment can run
//! the backend without a broker. `Disabled` makes that explicit: publishing returns
//! [`MessagingError::Disabled`] rather than silently succeeding, so a caller that depends
//! on delivery finds out.

use wiab_app::{Messaging, MessagingError};

use crate::nats_messaging::NatsMessaging;

pub enum MessagingDispatch {
    Nats(NatsMessaging),
    Disabled,
}

impl Messaging for MessagingDispatch {
    async fn publish(&self, subject: &str, payload: Vec<u8>) -> Result<(), MessagingError> {
        match self {
            Self::Nats(inner) => inner.publish(subject, payload).await,
            Self::Disabled => Err(MessagingError::Disabled),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_reports_why_rather_than_pretending_to_publish() {
        let error = MessagingDispatch::Disabled
            .publish("team.status", b"{}".to_vec())
            .await
            .unwrap_err();
        assert!(matches!(error, MessagingError::Disabled));
        // The message names the flag to set — the fix is not otherwise discoverable.
        assert!(error.to_string().contains("WIAB_NATS_ENABLED"));
    }
}
