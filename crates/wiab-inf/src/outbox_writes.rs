use tokio_postgres::Transaction;
use wiab_core::event::DomainEvent;

/// Append events inside the caller's transaction.
///
/// Taking a `Transaction` rather than a pool is the whole point: the events land with the
/// row that produced them, so a committed change always has its events and a rolled-back
/// one never does.
pub(crate) async fn append(
    transaction: &Transaction<'_>,
    events: &[DomainEvent],
) -> Result<(), tokio_postgres::Error> {
    for event in events {
        transaction
            .execute(
                "INSERT INTO outbox (name, aggregate_id, payload) VALUES ($1, $2, $3)",
                &[&event.name, &event.aggregate_id, &event.payload],
            )
            .await?;
    }
    Ok(())
}
