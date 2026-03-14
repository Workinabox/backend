mod bootstrap;

use std::net::SocketAddr;

use anyhow::Context;
use tracing::info;
use wiab_inf::http_router;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let state = bootstrap::build_app_state().await?;
    let app = http_router(state);

    let addr: SocketAddr = "0.0.0.0:8080"
        .parse()
        .context("invalid backend bind address")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("failed to bind backend listener")?;
    info!("wiab backend listening on http://{addr}");

    axum::serve(listener, app)
        .await
        .context("backend server terminated unexpectedly")?;

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "wiab=info,tower_http=info".to_owned()),
        )
        .init();
}
