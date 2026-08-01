//! NATS implementation of the [`Messaging`] port.
//!
//! Connection is attempted once at startup and the client is shared. `async_nats::Client`
//! is cheap to clone and reconnects on its own, so a broker restart does not need the
//! backend restarted with it.

use async_nats::Client;
use wiab_app::{Messaging, MessagingError};

/// Broker connection, read from env with a default matching the local compose service.
#[derive(Clone, Debug)]
pub struct NatsConfig {
    pub url: String,
}

impl NatsConfig {
    pub fn from_env() -> Self {
        Self {
            url: std::env::var("WIAB_NATS_URL")
                .unwrap_or_else(|_| "nats://127.0.0.1:4222".to_owned()),
        }
    }
}

/// Clone is cheap: `async_nats::Client` is a handle onto one shared connection, so a
/// clone shares the connection rather than opening another.
#[derive(Clone)]
pub struct NatsMessaging {
    client: Client,
}

impl NatsMessaging {
    /// Connect to the broker. Unlike the Docker runtime — which warns and carries on,
    /// because a launch can be retried later — a failure here is returned to the caller,
    /// so bootstrap decides whether an unreachable broker is fatal.
    pub async fn connect(config: &NatsConfig) -> Result<Self, MessagingError> {
        let client = async_nats::connect(&config.url)
            .await
            .map_err(|e| MessagingError::Backend(format!("connect to {}: {e}", config.url)))?;
        tracing::info!("nats: connected to {}", config.url);
        Ok(Self { client })
    }
}

impl Messaging for NatsMessaging {
    async fn publish(&self, subject: &str, payload: Vec<u8>) -> Result<(), MessagingError> {
        self.client
            .publish(subject.to_owned(), payload.into())
            .await
            .map_err(|e| MessagingError::Backend(format!("publish to {subject}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the real broker. Ignored by default so the suite stays runnable without
    /// one; CI and `docker compose up nats` can run it with `--ignored`.
    #[tokio::test]
    #[ignore = "requires a NATS broker on WIAB_NATS_URL"]
    async fn publishes_to_a_real_broker() {
        use futures_util::StreamExt;

        let messaging = NatsMessaging::connect(&NatsConfig::from_env())
            .await
            .expect("connect");
        let mut sub = messaging
            .client
            .subscribe("wiab.test.publish")
            .await
            .expect("subscribe");

        messaging
            .publish("wiab.test.publish", b"hello".to_vec())
            .await
            .expect("publish");

        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), sub.next())
            .await
            .expect("no message within 5s")
            .expect("subscription closed");
        assert_eq!(&msg.payload[..], b"hello");
    }

    #[tokio::test]
    async fn config_defaults_to_the_local_broker() {
        // Only when unset — an operator's WIAB_NATS_URL must win.
        unsafe { std::env::remove_var("WIAB_NATS_URL") };
        assert_eq!(NatsConfig::from_env().url, "nats://127.0.0.1:4222");
    }
}
