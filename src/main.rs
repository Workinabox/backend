mod bootstrap;
mod config;

use std::net::SocketAddr;

use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use clap::{CommandFactory, FromArgMatches, Parser, parser::ValueSource};
use tracing::{info, warn};
use wiab_inf::{http_router, spawn_git_ssh_server};

use crate::config::{AppConfig, ServeConfig};

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
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    // Parse via ArgMatches so we can report where each value came from, then resolve the full
    // configuration once (env is read only inside `AppConfig::load`).
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches)?;
    let config = AppConfig::load(&cli)?;

    init_tracing(&config.serve.rust_log);
    info!(
        "config: persistence = {} ({})",
        config.serve.persistence,
        source_label(&matches, "persistence", "WIAB_PERSISTENCE", "--persistence"),
    );
    if config
        .serve
        .persistence
        .trim()
        .eq_ignore_ascii_case("postgres")
    {
        info!(
            "config: database_url = {} ({})",
            redact_password(&config.serve.database_url),
            source_label(&matches, "database_url", "DATABASE_URL", "--database-url"),
        );
    }

    // TLS first: the server's own certificate is handed to team containers so they can
    // verify it, and that has to be known before the services that pass it are built.
    let (tls, certificate_pem) = load_tls_config(&config.serve, &config.auth.base_url).await?;

    let state = bootstrap::build_app_state(&config, certificate_pem).await?;

    // Git SSH transport runs on its own port alongside the HTTPS server.
    let ssh_addr = config.serve.git_ssh_addr.clone();
    let ssh_host_key = config.serve.git_ssh_host_key.clone();
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
    info!("wiab backend listening on https://{addr}");

    axum_server::bind_rustls(addr, tls)
        .serve(app.into_make_service())
        .await
        .context("backend server terminated unexpectedly")?;

    Ok(())
}

/// Loads the TLS cert/key from `WIAB_TLS_CERT`/`WIAB_TLS_KEY` (PEM), or generates a
/// self-signed cert. The backend always serves HTTPS; a reverse proxy connects to it over
/// TLS (with verification disabled for the self-signed case).
///
/// Also returns the server's certificate in PEM. Team containers are handed it so they can
/// verify this backend rather than skip verification — without it a team cannot reach a
/// backend using the generated certificate at all, which is the default.
async fn load_tls_config(
    serve: &ServeConfig,
    base_url: &str,
) -> anyhow::Result<(RustlsConfig, Option<String>)> {
    match (&serve.tls_cert, &serve.tls_key) {
        (Some(cert), Some(key)) => {
            let pem = std::fs::read_to_string(cert)
                .with_context(|| format!("failed to read TLS cert {cert}"))?;
            let config = RustlsConfig::from_pem_file(cert, key)
                .await
                .context("failed to load TLS cert/key")?;
            Ok((config, Some(pem)))
        }
        _ => {
            warn!(
                "WIAB_TLS_CERT/WIAB_TLS_KEY unset; generating a self-signed certificate \
                 (clients/proxies must skip verification or trust it)"
            );
            let cert = rcgen::generate_simple_self_signed(self_signed_names(base_url))
                .context("failed to generate self-signed certificate")?;
            let pem = cert.cert.pem();
            let config = RustlsConfig::from_pem(
                pem.clone().into_bytes(),
                cert.key_pair.serialize_pem().into_bytes(),
            )
            .await
            .context("failed to build TLS config from self-signed certificate")?;
            Ok((config, Some(pem)))
        }
    }
}

/// Names the generated certificate must cover.
///
/// `localhost` and `127.0.0.1` for anything on the host, `host.docker.internal` because a
/// team container reaches its backend by that name, and whatever host `WIAB_BASE_URL`
/// advertises — a certificate that does not cover the name clients are told to use is no
/// use to them.
fn self_signed_names(base_url: &str) -> Vec<String> {
    let mut names = vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "host.docker.internal".to_owned(),
    ];
    if let Some(host) = base_url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .map(|host| host.split(':').next().unwrap_or(host))
        && !host.is_empty()
        && !names.iter().any(|existing| existing == host)
    {
        names.push(host.to_owned());
    }
    names
}

fn init_tracing(filter: &str) {
    tracing_subscriber::fmt()
        .with_env_filter(filter.to_owned())
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_certificate_covers_the_advertised_host() {
        let names = self_signed_names("https://wiab.example.com:8080/");
        assert!(names.contains(&"wiab.example.com".to_owned()));
        assert!(names.contains(&"host.docker.internal".to_owned()));
        assert!(names.contains(&"localhost".to_owned()));
    }

    #[test]
    fn a_base_url_that_adds_nothing_is_not_duplicated() {
        let names = self_signed_names("https://localhost:8080");
        assert_eq!(names.iter().filter(|n| *n == "localhost").count(), 1);
    }

    #[test]
    fn a_malformed_base_url_still_yields_the_defaults() {
        assert_eq!(self_signed_names("not-a-url").len(), 3);
    }
}
