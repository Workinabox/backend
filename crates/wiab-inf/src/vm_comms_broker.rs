//! Host side of the agent↔backend vsock channel.
//!
//! Firecracker exposes a guest→host vsock connection as a Unix socket on the host named
//! `<uds_path>_<port>`. The guest `wiab-agent` connects to host CID 2 on [`AGENT_VSOCK_PORT`];
//! Firecracker then connects to that Unix socket, and we read the agent's check-ins. Best-effort
//! observability: bind/accept failures are logged, never fatal to the VM.

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;

/// Port the guest agent reports on (must match `wiab-agent`).
pub const AGENT_VSOCK_PORT: u32 = 5000;

/// The host-side Unix socket path Firecracker uses for guest→host connections on
/// [`AGENT_VSOCK_PORT`], given the vsock base path inside the jail.
pub fn agent_socket_path(vsock_base: &std::path::Path) -> PathBuf {
    let mut name = vsock_base.as_os_str().to_owned();
    name.push(format!("_{AGENT_VSOCK_PORT}"));
    PathBuf::from(name)
}

/// Spawn a listener for one VM's agent check-ins. The backend must be listening before the guest
/// connects, so this is started right after the VM is launched.
pub fn spawn_agent_listener(socket_path: PathBuf, vm_id: String) {
    tokio::spawn(async move {
        let _ = std::fs::remove_file(&socket_path);
        let listener = match UnixListener::bind(&socket_path) {
            Ok(listener) => listener,
            Err(error) => {
                tracing::warn!("vsock broker[{vm_id}]: bind {socket_path:?} failed: {error}");
                return;
            }
        };
        tracing::info!("vsock broker[{vm_id}]: listening on {socket_path:?}");
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let vm_id = vm_id.clone();
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(stream).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            tracing::info!("agent report[{vm_id}]: {line}");
                        }
                    });
                }
                Err(error) => {
                    tracing::warn!("vsock broker[{vm_id}]: accept failed: {error}");
                    break;
                }
            }
        }
    });
}
