//! PostgreSQL connection pool and migration runner shared by the `Postgres*Repository`
//! implementations.
//!
//! The pool is built from a libpq-style connection string (the `DATABASE_URL` env var).
//! Migrations are embedded into the binary at compile time and applied on boot, so a
//! freshly deployed binary brings its own schema up to date with no separate step.

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use tokio_postgres::NoTls;

mod embedded {
    refinery::embed_migrations!("migrations");
}

/// Build a connection pool from a libpq connection string / URL
/// (e.g. `postgres://user:pass@localhost:5432/wiab`), failing fast if unreachable.
pub async fn build_pool(database_url: &str) -> anyhow::Result<Pool> {
    let pg_config: tokio_postgres::Config = database_url.parse()?;
    let manager = Manager::from_config(
        pg_config,
        NoTls,
        ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        },
    );
    let pool = Pool::builder(manager).max_size(16).build()?;
    // Fail fast with a clear error if the database is unreachable.
    let _ = pool.get().await?;
    Ok(pool)
}

/// Apply all pending embedded migrations. Safe to call on every boot; refinery records
/// applied migrations in its own table and only runs new ones.
pub async fn run_migrations(pool: &Pool) -> anyhow::Result<()> {
    let mut client = pool.get().await?;
    embedded::migrations::runner()
        .run_async(&mut **client)
        .await?;
    Ok(())
}
