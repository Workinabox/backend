//! authbox's database migrations, applied on boot.
//!
//! They run in their **own** refinery history table (`authbox_migrations`), independent of
//! the host's `refinery_schema_history`, so the two migration series never interleave and
//! the auth schema extracts cleanly with the crate later.

use deadpool_postgres::Pool;

mod embedded {
    refinery::embed_migrations!("migrations");
}

/// Apply all pending authbox migrations. Safe to call on every boot. Call this alongside
/// (before or after) the host's own migration runner — they use separate history tables.
pub async fn run_migrations(pool: &Pool) -> anyhow::Result<()> {
    let mut client = pool.get().await?;
    let mut runner = embedded::migrations::runner();
    runner.set_migration_table_name("authbox_migrations");
    runner.run_async(&mut **client).await?;
    Ok(())
}
