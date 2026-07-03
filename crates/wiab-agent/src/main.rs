//! The in-guest agent. Runs inside a Firecracker microVM (delivered on the `/dev/vdc` agent
//! drive, started by the `wiab-agent` systemd unit). It reads its agent id + model endpoint from
//! the agent-drive config, announces itself, and runs a heartbeat loop — reporting to the
//! backend over vsock when the broker is listening, and always logging to the console.
//!
//! This is the first real slice of the agent. The full reasoning loop (docs/AGENT_MODEL.md) is
//! separate work; this establishes that a per-agent process runs inside its VM and reports back.

use std::io::Write;
use std::time::Duration;

const HEARTBEAT: Duration = Duration::from_secs(30);

/// vsock: guests reach the host on CID 2; the backend broker listens on this port.
#[cfg(target_os = "linux")]
const VSOCK_HOST_CID: u32 = 2;
#[cfg(target_os = "linux")]
const VSOCK_PORT: u32 = 5000;

/// Resolve a config value from the environment, falling back to the agent-drive `agent.env`.
fn config(key: &str) -> Option<String> {
    if let Ok(value) = std::env::var(key) {
        if !value.is_empty() {
            return Some(value);
        }
    }
    let text = std::fs::read_to_string("/opt/agent/agent.env").ok()?;
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return Some(v.trim().trim_matches('"').to_owned());
            }
        }
    }
    None
}

fn log(message: &str) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "[wiab-agent] {message}");
    let _ = out.flush();
}

fn main() {
    let agent_id = config("WIAB_AGENT_ID").unwrap_or_else(|| "A-?".to_owned());
    let endpoint = config("WIAB_MODEL_ENDPOINT").unwrap_or_default();
    log(&format!(
        "up as {agent_id}; model endpoint: {}",
        if endpoint.is_empty() {
            "<none>"
        } else {
            &endpoint
        }
    ));

    let mut beat: u64 = 0;
    loop {
        beat += 1;
        let message = format!("{agent_id} heartbeat #{beat}");
        log(&message);
        #[cfg(target_os = "linux")]
        report_over_vsock(&message);
        std::thread::sleep(HEARTBEAT);
    }
}

/// Best-effort report to the backend over vsock. Non-fatal: if the broker isn't listening the
/// heartbeat still reaches the host via the serial console.
#[cfg(target_os = "linux")]
fn report_over_vsock(message: &str) {
    use std::io::Write as _;
    if let Ok(mut stream) = vsock::VsockStream::connect_with_cid_port(VSOCK_HOST_CID, VSOCK_PORT) {
        let _ = writeln!(stream, "{message}");
    }
}
