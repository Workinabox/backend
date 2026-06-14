mod bootstrap;

use std::net::SocketAddr;

use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use clap::{CommandFactory, FromArgMatches, Parser, parser::ValueSource};
use tracing::info;
use wiab_inf::{http_router, spawn_git_ssh_server};

/// Backend configuration. Each value defaults to a baked-in dev value, can be overridden by
/// an environment variable, and can be overridden again by a command-line flag (which wins).
#[derive(Parser, Debug)]
#[command(name = "wiab")]
struct Cli {
    /// Persistence backend: "postgres" or "memory".
    #[arg(long, env = "WIAB_PERSISTENCE", default_value = "postgres")]
    persistence: String,

    /// Postgres connection URL (used when persistence is "postgres").
    #[arg(
        long,
        env = "DATABASE_URL",
        default_value = "postgres://wiab:wiab@localhost:5432/wiab"
    )]
    database_url: String,
}

/// Describe where a parsed value came from, so the console shows whether a default was
/// overridden by an env var or a CLI flag.
fn source_label(matches: &clap::ArgMatches, id: &str, env_name: &str, flag_name: &str) -> String {
    match matches.value_source(id) {
        Some(ValueSource::CommandLine) => format!("from {flag_name}"),
        Some(ValueSource::EnvVariable) => format!("from env {env_name}"),
        _ => "default".to_owned(),
    }
}

/// Hide the password in a `postgres://user:pass@host/db` URL before logging it.
fn redact_password(url: &str) -> String {
    match (url.find("://"), url.find('@')) {
        (Some(scheme_end), Some(at)) if scheme_end + 3 < at => {
            let creds = &url[scheme_end + 3..at];
            match creds.split_once(':') {
                Some((user, _)) => format!("{}://{user}:****{}", &url[..scheme_end], &url[at..]),
                None => url.to_owned(),
            }
        }
        _ => url.to_owned(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    // Parse via ArgMatches so we can report where each value came from.
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches)?;
    info!(
        "config: persistence = {} ({})",
        cli.persistence,
        source_label(&matches, "persistence", "WIAB_PERSISTENCE", "--persistence"),
    );
    if cli.persistence.trim().eq_ignore_ascii_case("postgres") {
        info!(
            "config: database_url = {} ({})",
            redact_password(&cli.database_url),
            source_label(&matches, "database_url", "DATABASE_URL", "--database-url"),
        );
    }

    let state = bootstrap::build_app_state(&cli.persistence, &cli.database_url).await?;

    // Git SSH transport runs on its own port alongside the main HTTP server.
    let ssh_addr = std::env::var("WIAB_GIT_SSH_ADDR").unwrap_or_else(|_| "0.0.0.0:2222".to_owned());
    let ssh_host_key = std::env::var("WIAB_GIT_SSH_HOST_KEY").ok();
    {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = spawn_git_ssh_server(state, ssh_addr, ssh_host_key).await {
                tracing::error!("git SSH server stopped: {err}");
            }
        });
    }

    let app = http_router(state);

    let addr: SocketAddr = "0.0.0.0:8080"
        .parse()
        .context("invalid backend bind address")?;

    // Serve HTTPS only when a cert/key is explicitly configured. Otherwise serve plain
    // HTTP: in production TLS is terminated upstream by nginx (which proxies to
    // http://127.0.0.1:8080), and locally http://localhost:8080 is convenient.
    let result = match load_tls_config().await? {
        Some(tls) => {
            info!("wiab backend listening on https://{addr}");
            axum_server::bind_rustls(addr, tls)
                .serve(app.into_make_service())
                .await
        }
        None => {
            info!(
                "wiab backend listening on http://{addr} (TLS terminated upstream; \
                 set WIAB_TLS_CERT/WIAB_TLS_KEY to serve HTTPS directly)"
            );
            axum_server::bind(addr).serve(app.into_make_service()).await
        }
    };
    result.context("backend server terminated unexpectedly")?;

    Ok(())
}

/// Loads the TLS cert/key from `WIAB_TLS_CERT`/`WIAB_TLS_KEY` (PEM) when both are set, else
/// `None` — serve plain HTTP and let a reverse proxy terminate TLS.
async fn load_tls_config() -> anyhow::Result<Option<RustlsConfig>> {
    match (
        std::env::var("WIAB_TLS_CERT"),
        std::env::var("WIAB_TLS_KEY"),
    ) {
        (Ok(cert), Ok(key)) => RustlsConfig::from_pem_file(cert, key)
            .await
            .map(Some)
            .context("failed to load TLS cert/key"),
        _ => Ok(None),
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "wiab=info,tower_http=info".to_owned()),
        )
        .init();
}
