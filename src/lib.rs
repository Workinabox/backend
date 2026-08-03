//! The backend as a library.
//!
//! `main.rs` is a thin wrapper over this: it parses the CLI, builds the `AppConfig`, and serves
//! the router. Exposing the same modules as a library is what lets the integration tests in
//! `tests/` build a real `AppState` and drive the actual router, rather than testing the
//! handlers' helper predicates in isolation.

pub mod bootstrap;
pub mod config;

use clap::Parser;

/// Backend configuration. Each value defaults to a baked-in dev value, can be overridden by
/// an environment variable, and can be overridden again by a command-line flag (which wins).
#[derive(Parser, Debug)]
#[command(name = "wiab")]
pub struct Cli {
    /// Persistence backend: "postgres" or "memory".
    #[arg(long, env = "WIAB_PERSISTENCE", default_value = "postgres")]
    pub persistence: String,

    /// Postgres connection URL (used when persistence is "postgres").
    #[arg(
        long,
        env = "DATABASE_URL",
        default_value = "postgres://wiab:wiab@localhost:5432/wiab"
    )]
    pub database_url: String,
}
