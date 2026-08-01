//! Live Docker runtime test. Requires a reachable Docker daemon and the `wiab-agent-base:latest`
//! image (build it with `iac/images/agent/build.sh base`). Ignored by default so `cargo test` on
//! a host without Docker or the image stays green; run explicitly with:
//!
//!   cargo test -p wiab-inf --test docker_runtime -- --ignored
//!
//! Unlike the Firecracker adapter (needs KVM, can't run in CI), this exercises the real launch →
//! inspect → shutdown path against Docker.

use wiab_app::{VmRuntime, VmSpec};
use wiab_inf::{DockerConfig, DockerRuntime};

#[tokio::test]
#[ignore = "requires a Docker daemon and the wiab-agent-base:latest image"]
async fn launch_inspect_shutdown_roundtrip() {
    let runtime = DockerRuntime::new(DockerConfig::from_env())
        .await
        .expect("connect to docker daemon");

    let spec = VmSpec {
        env: Vec::new(),
        id: "VM-9001".to_owned(),
        agent_id: "A-9001".to_owned(),
        template: "base".to_owned(),
        vcpus: 1,
        mem_mib: 256,
    };
    // Clean up any container left by a previous run.
    let _ = runtime.shutdown(&spec.id).await;

    let handle = runtime.launch(spec.clone()).await.expect("launch");
    assert!(
        !handle.guest_ip.is_empty(),
        "expected a real container IP, got empty"
    );
    assert!(handle.pid > 0, "expected a real pid, got {}", handle.pid);

    // The agent logs "up as <id>" on boot before anything else. Seeing OUR agent id proves the
    // env was injected and the agent actually ran — the whole point vs. the old no-op mock.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let logs = tokio::process::Command::new("docker")
        .args(["logs", "wiab-VM-9001"])
        .output()
        .await
        .expect("read docker logs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&logs.stdout),
        String::from_utf8_lossy(&logs.stderr)
    );
    assert!(
        text.contains("up as A-9001"),
        "agent did not report up; logs:\n{text}"
    );

    runtime.shutdown(&spec.id).await.expect("shutdown");
    // Idempotent: a second shutdown swallows the 404 and succeeds.
    runtime
        .shutdown(&spec.id)
        .await
        .expect("second shutdown is a no-op");

    // The container is really gone.
    let gone = tokio::process::Command::new("docker")
        .args(["inspect", "wiab-VM-9001"])
        .output()
        .await
        .expect("docker inspect");
    assert!(
        !gone.status.success(),
        "container should be removed after shutdown"
    );
}
