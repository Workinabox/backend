use deadpool_postgres::Pool;
use wiab_app::{Outbox, PendingEvent};
use wiab_core::event::DomainEvent;
use wiab_core::repository::RepoError;

/// PostgreSQL-backed outbox reader. Writing is not here — see `outbox_writes`, which
/// appends inside the same transaction as the aggregate save.
#[derive(Clone)]
pub struct PostgresOutbox {
    pool: Pool,
}

impl PostgresOutbox {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn repo_error<E: std::fmt::Display>(error: E) -> RepoError {
    RepoError::Backend(error.to_string())
}

impl Outbox for PostgresOutbox {
    async fn pending(&self, limit: i64) -> Result<Vec<PendingEvent>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(
                "SELECT id, name, aggregate_id, payload FROM outbox ORDER BY id LIMIT $1",
                &[&limit],
            )
            .await
            .map_err(repo_error)?;
        Ok(rows
            .iter()
            .map(|row| PendingEvent {
                id: row.get(0),
                event: DomainEvent::new(
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                    row.get(3),
                ),
            })
            .collect())
    }

    async fn mark_published(&self, ids: &[i64]) -> Result<(), RepoError> {
        if ids.is_empty() {
            return Ok(());
        }
        let client = self.pool.get().await.map_err(repo_error)?;
        client
            .execute("DELETE FROM outbox WHERE id = ANY($1)", &[&ids])
            .await
            .map_err(repo_error)?;
        Ok(())
    }
}
