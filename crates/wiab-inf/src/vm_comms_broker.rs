//! Host side of the agent↔backend vsock channel.
//!
//! Firecracker exposes a guest→host vsock connection as a Unix socket on the host named
//! `<uds_path>_<port>`. The guest `wiab-agent` connects to host CID 2 on [`AGENT_VSOCK_PORT`];
//! Firecracker then connects to that Unix socket, and we read the agent's check-ins. Best-effort
//! observability: bind/accept failures are logged, never fatal to the VM.

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::UnixListener;

/// Longest agent report line we will keep. The guest runs agent-controlled code, so the length
/// is chosen by an untrusted party; without a cap one connection can make the host buffer an
/// arbitrarily long "line" it never terminates.
const MAX_REPORT_LINE: usize = 2048;

/// Port the guest agent reports on (must match `wiab-agent`).
pub const AGENT_VSOCK_PORT: u32 = 5000;

/// The host-side Unix socket path Firecracker uses for guest→host connections on
/// [`AGENT_VSOCK_PORT`], given the vsock base path inside the jail.
pub fn agent_socket_path(vsock_base: &std::path::Path) -> PathBuf {
    let mut name = vsock_base.as_os_str().to_owned();
    name.push(format!("_{AGENT_VSOCK_PORT}"));
    PathBuf::from(name)
}

/// Makes a guest-supplied line safe to log: control characters escaped, length capped.
///
/// The content comes from agent code inside the sandbox, which is the whole point of the
/// sandbox — so it is untrusted input going straight into the host's log. Escaping control
/// characters stops it forging log lines or injecting terminal escapes into an operator's
/// console; the cap stops one report flooding the log.
fn sanitize_report(line: &str) -> String {
    let mut out = String::with_capacity(line.len().min(MAX_REPORT_LINE));
    for character in line.chars() {
        if out.len() >= MAX_REPORT_LINE {
            out.push_str("… (truncated)");
            break;
        }
        if character.is_control() {
            out.push_str(&character.escape_debug().to_string());
        } else {
            out.push(character);
        }
    }
    out
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
                        // Cap what one connection can make us buffer before we ever see a
                        // newline. `lines()` alone will grow a single line without limit.
                        let bounded = stream.take((MAX_REPORT_LINE * 64) as u64);
                        let mut lines = BufReader::new(bounded).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            tracing::info!("agent report[{vm_id}]: {}", sanitize_report(&line));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_cannot_forge_a_log_line() {
        let forged = sanitize_report("ok\n2026-01-01 INFO impersonated line");
        assert!(!forged.contains('\n'), "{forged}");
        assert!(forged.contains("\\n"), "{forged}");
    }

    #[test]
    fn terminal_escapes_are_neutralised() {
        let escaped = sanitize_report("\u{1b}[2J\u{1b}[1;31mALERT");
        assert!(!escaped.contains('\u{1b}'), "{escaped}");
    }

    #[test]
    fn a_long_report_is_truncated() {
        let long = sanitize_report(&"a".repeat(MAX_REPORT_LINE * 4));
        assert!(long.len() < MAX_REPORT_LINE + 32, "length {}", long.len());
        assert!(long.ends_with("… (truncated)"), "{long}");
    }

    #[test]
    fn an_ordinary_report_is_unchanged() {
        assert_eq!(
            sanitize_report("booted; agent ready"),
            "booted; agent ready"
        );
    }
}
