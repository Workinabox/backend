mod bootstrap;

use std::net::SocketAddr;

use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use clap::{CommandFactory, FromArgMatches, Parser, parser::ValueSource};
use tracing::{info, warn};
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

    // Git SSH transport runs on its own port alongside the HTTPS server.
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
    let tls = load_tls_config().await?;
    info!("wiab backend listening on https://{addr}");

    axum_server::bind_rustls(addr, tls)
        .serve(app.into_make_service())
        .await
        .context("backend server terminated unexpectedly")?;

    Ok(())
}

/// Loads the TLS cert/key from `WIAB_TLS_CERT`/`WIAB_TLS_KEY` (PEM), or generates a
/// self-signed cert for local development.
async fn load_tls_config() -> anyhow::Result<RustlsConfig> {
    match (
        std::env::var("WIAB_TLS_CERT"),
        std::env::var("WIAB_TLS_KEY"),
    ) {
        (Ok(cert), Ok(key)) => RustlsConfig::from_pem_file(cert, key)
            .await
            .context("failed to load TLS cert/key"),
        _ => {
            warn!(
                "WIAB_TLS_CERT/WIAB_TLS_KEY unset; generating a self-signed certificate \
                 (dev only — clients must skip verification or trust it)"
            );
            let cert = rcgen::generate_simple_self_signed(vec![
                "localhost".to_owned(),
                "127.0.0.1".to_owned(),
            ])
            .context("failed to generate self-signed certificate")?;
            RustlsConfig::from_pem(
                cert.cert.pem().into_bytes(),
                cert.key_pair.serialize_pem().into_bytes(),
            )
            .await
            .context("failed to build TLS config from self-signed certificate")
        }
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "wiab=info,tower_http=info".to_owned()),
        )
        .init();
}
