mod bootstrap;

use std::net::SocketAddr;

use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use tracing::{info, warn};
use wiab_inf::{http_router, spawn_git_ssh_server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let state = bootstrap::build_app_state().await?;

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
