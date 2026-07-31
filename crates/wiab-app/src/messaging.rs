use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessagingError {
    #[error("messaging error: {0}")]
    Backend(String),
    #[error("messaging is disabled; set WIAB_NATS_ENABLED to publish")]
    Disabled,
}

/// Port for the message broker the backend and its agent teams talk over.
///
/// Defined here as an application dependency; infrastructure provides the NATS
/// implementation. Kept to publishing for now — nothing consumes yet, and a port with
/// methods no caller uses is harder to change than one that grows with its callers.
///
/// This is the only channel to a containerised team: the vsock broker in
/// `vm_comms_broker` is Firecracker-only, so a team launched under Docker has no other
/// way to be reached.
#[allow(async_fn_in_trait)]
pub trait Messaging: Send + Sync + 'static {
    /// Publish `payload` on `subject`. Delivery guarantees are the implementation's;
    /// callers that need durability must say so through the subject they choose.
    async fn publish(&self, subject: &str, payload: Vec<u8>) -> Result<(), MessagingError>;
}
