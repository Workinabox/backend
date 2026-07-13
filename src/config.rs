//! Centralized application configuration.
//!
//! Every environment variable the backend reads is resolved here, once, at startup — instead of
//! scattered `std::env::var` calls across bootstrap and the infrastructure crate. `main` parses
//! the CLI, builds an [`AppConfig`], and threads it (and its sub-configs) into `build_app_state`
//! and the runtime components, which no longer touch the environment themselves.
//!
//! Grouped sub-configs keep each concern together. Structs consumed inside `wiab-inf`
//! (Firecracker, Docker, Llama, Whisper, media) live in that crate next to their components; the
//! groups consumed here in the binary (serve, auth, email, dev-seeding) live in this module.

use crate::Cli;

/// The whole backend configuration, resolved once at startup.
pub struct AppConfig {
    pub serve: ServeConfig,
}

/// Process/serving config: persistence selection, the addresses we bind, and TLS material.
pub struct ServeConfig {
    /// Persistence backend: `postgres` or `memory` (from `--persistence` / `WIAB_PERSISTENCE`).
    pub persistence: String,
    /// Postgres connection URL (from `--database-url` / `DATABASE_URL`).
    pub database_url: String,
    /// Address the git SSH transport binds (`WIAB_GIT_SSH_ADDR`).
    pub git_ssh_addr: String,
    /// Optional git SSH host key path (`WIAB_GIT_SSH_HOST_KEY`).
    pub git_ssh_host_key: Option<String>,
    /// TLS cert/key PEM paths (`WIAB_TLS_CERT`/`WIAB_TLS_KEY`); both unset → self-signed.
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    /// Tracing filter (`RUST_LOG`).
    pub rust_log: String,
}

impl AppConfig {
    /// Resolve the full configuration from the parsed CLI plus the environment. Reads only —
    /// heavy work (DB connect, model loading) stays in `build_app_state`.
    pub fn load(cli: &Cli) -> anyhow::Result<Self> {
        Ok(Self {
            serve: ServeConfig {
                persistence: cli.persistence.clone(),
                database_url: cli.database_url.clone(),
                git_ssh_addr: env_or("WIAB_GIT_SSH_ADDR", "0.0.0.0:2222"),
                git_ssh_host_key: std::env::var("WIAB_GIT_SSH_HOST_KEY").ok(),
                tls_cert: std::env::var("WIAB_TLS_CERT").ok(),
                tls_key: std::env::var("WIAB_TLS_KEY").ok(),
                rust_log: env_or("RUST_LOG", "wiab=info,tower_http=info"),
            },
        })
    }
}

/// Read an env var, falling back to a default when unset.
fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}
