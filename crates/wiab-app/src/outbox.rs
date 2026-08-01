use wiab_core::event::DomainEvent;
use wiab_core::repository::RepoError;

/// One event as it sits in the outbox, waiting to be published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEvent {
    pub id: i64,
    pub event: DomainEvent,
}

/// Reading and retiring events that aggregates have already committed.
///
/// There is no `append` here: writing an event is not a separate decision a caller gets to
/// make, it happens inside the same transaction as the save that produced it. Exposing an
/// append would invite exactly the split that the outbox pattern exists to prevent.
#[allow(async_fn_in_trait)]
pub trait Outbox: Send + Sync + 'static {
    /// The oldest `limit` events still waiting, in the order they happened.
    async fn pending(&self, limit: i64) -> Result<Vec<PendingEvent>, RepoError>;

    /// Forget events that have been published. Taking a batch keeps it to one round trip.
    async fn mark_published(&self, ids: &[i64]) -> Result<(), RepoError>;
}
