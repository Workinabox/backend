//! Docker implementation of the [`VmRuntime`] port.
//!
//! Runs one container per agent from an agent-native image (`wiab-agent` is the image's
//! `ENTRYPOINT`, configured entirely via env). This is the backend used wherever Firecracker
//! isn't: local dev, CI, and — accepting the weaker boundary — production.
//!
//! Unlike a Firecracker microVM (own guest kernel behind KVM), a container shares the host
//! kernel, so launches are hardened: the image runs as a non-root user, all capabilities are
//! dropped, `no-new-privileges` is set, and the rootfs is read-only with a `/tmp` tmpfs (the
//! agent writes only to stdout). Operators needing a hardware boundary keep Firecracker via
//! `WIAB_FIRECRACKER_ENABLED` on a KVM host.
//!
//! The container is named `wiab-<vm-id>`, so shutdown is stateless (keyed by the id), the same
//! way the Firecracker runtime keys off the id rather than an in-process handle.

use std::collections::HashMap;

use bollard::Docker;
use bollard::errors::Error as DockerError;
use bollard::models::{ContainerCreateBody, ContainerInspectResponse, HostConfig};
use bollard::query_parameters::{
    CreateContainerOptions, RemoveContainerOptions, StopContainerOptions,
};
use wiab_app::{RuntimeHandle, VmRuntime, VmRuntimeError, VmSpec};

/// Image naming + networking, read from env with defaults.
#[derive(Clone, Debug)]
pub struct DockerConfig {
    /// Image = `<image_prefix><template>:<image_tag>` (e.g. `wiab-agent-developer:latest`).
    pub image_prefix: String,
    pub image_tag: String,
    /// Optional user-defined network to attach containers to; empty = Docker's default bridge.
    pub network: String,
    /// Passed to the guest agent as `WIAB_MODEL_ENDPOINT` (same var the Firecracker runtime uses).
    pub model_endpoint: String,
    /// Seconds to wait for a graceful stop before Docker force-kills the container.
    pub stop_timeout_secs: i32,
}

impl DockerConfig {
    pub fn from_env() -> Self {
        let get = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_owned());
        Self {
            image_prefix: get("WIAB_DOCKER_IMAGE_PREFIX", "wiab-agent-"),
            image_tag: get("WIAB_DOCKER_IMAGE_TAG", "latest"),
            network: get("WIAB_DOCKER_NETWORK", ""),
            model_endpoint: get("WIAB_MODEL_ENDPOINT", ""),
            stop_timeout_secs: get("WIAB_DOCKER_STOP_TIMEOUT", "10").parse().unwrap_or(10),
        }
    }
}

/// Turn a bollard error into a `VmRuntimeError`, tagged with what we were doing.
fn err(context: &str, e: DockerError) -> VmRuntimeError {
    VmRuntimeError::Runtime(format!("{context}: {e}"))
}

/// A daemon 404 — the container is already gone, so stop/remove can treat it as success.
fn is_not_found(e: &DockerError) -> bool {
    matches!(
        e,
        DockerError::DockerResponseServerError {
            status_code: 404,
            ..
        }
    )
}

pub struct DockerRuntime {
    config: DockerConfig,
    docker: Docker,
}

impl DockerRuntime {
    /// Connect to the Docker daemon (honors `DOCKER_HOST`, else the local socket) and ping it, so
    /// an unreachable daemon is a clear startup warning rather than a cryptic first-launch error.
    pub async fn new(config: DockerConfig) -> Result<Self, VmRuntimeError> {
        let docker =
            Docker::connect_with_defaults().map_err(|e| err("connect to docker daemon", e))?;
        match docker.version().await {
            Ok(v) => tracing::info!(
                "docker runtime: connected (engine {})",
                v.version.unwrap_or_else(|| "?".to_owned())
            ),
            Err(e) => tracing::warn!(
                "docker runtime: daemon not reachable at startup ({e}); launches will fail until it is"
            ),
        }
        Ok(Self { config, docker })
    }

    fn container_name(vm_id: &str) -> String {
        format!("wiab-{vm_id}")
    }

    fn image(&self, template: &str) -> String {
        format!(
            "{}{}:{}",
            self.config.image_prefix, template, self.config.image_tag
        )
    }

    /// The container's IP: the configured network if set, else the default `bridge`, else the
    /// first attached network. Empty if the container has no network (still a valid handle).
    fn guest_ip(&self, inspect: &ContainerInspectResponse) -> String {
        let Some(networks) = inspect
            .network_settings
            .as_ref()
            .and_then(|ns| ns.networks.as_ref())
        else {
            return String::new();
        };
        let endpoint = if self.config.network.is_empty() {
            networks.get("bridge").or_else(|| networks.values().next())
        } else {
            networks.get(&self.config.network)
        };
        endpoint
            .and_then(|ep| ep.ip_address.clone())
            .unwrap_or_default()
    }
}

impl VmRuntime for DockerRuntime {
    async fn launch(&self, spec: VmSpec) -> Result<RuntimeHandle, VmRuntimeError> {
        let name = Self::container_name(&spec.id);
        let image = self.image(&spec.template);

        // Hardened host config — shared-kernel isolation, so drop everything we can. The agent
        // writes only to stdout; a /tmp tmpfs lets the rootfs stay read-only.
        let tmpfs = HashMap::from([("/tmp".to_owned(), String::new())]);
        let host_config = HostConfig {
            nano_cpus: Some(i64::from(spec.vcpus) * 1_000_000_000),
            memory: Some(i64::from(spec.mem_mib) * 1024 * 1024),
            network_mode: (!self.config.network.is_empty()).then(|| self.config.network.clone()),
            readonly_rootfs: Some(true),
            cap_drop: Some(vec!["ALL".to_owned()]),
            security_opt: Some(vec!["no-new-privileges".to_owned()]),
            tmpfs: Some(tmpfs),
            ..Default::default()
        };
        let body = ContainerCreateBody {
            image: Some(image.clone()),
            env: Some(vec![
                format!("WIAB_AGENT_ID={}", spec.agent_id),
                format!("WIAB_MODEL_ENDPOINT={}", self.config.model_endpoint),
            ]),
            host_config: Some(host_config),
            ..Default::default()
        };

        // Clear any stale container left by a previous run of this VM id, then create + start.
        let _ = self
            .docker
            .remove_container(
                &name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;
        let options = CreateContainerOptions {
            name: Some(name.clone()),
            ..Default::default()
        };
        self.docker
            .create_container(Some(options), body)
            .await
            .map_err(|e| err(&format!("create container {name}"), e))?;
        self.docker
            .start_container(&name, None)
            .await
            .map_err(|e| err(&format!("start container {name}"), e))?;

        let inspect = self
            .docker
            .inspect_container(&name, None)
            .await
            .map_err(|e| err(&format!("inspect container {name}"), e))?;
        let pid = inspect.state.as_ref().and_then(|s| s.pid).unwrap_or(-1);
        let guest_ip = self.guest_ip(&inspect);
        tracing::info!("docker runtime: launched {name} ({image}) pid {pid} ip {guest_ip}");
        Ok(RuntimeHandle { guest_ip, pid })
    }

    async fn shutdown(&self, vm_id: &str) -> Result<(), VmRuntimeError> {
        let name = Self::container_name(vm_id);
        // Graceful stop (SIGTERM, then Docker force-kills after the timeout), then remove. 404s
        // mean it's already gone, so treat them as success — shutdown stays idempotent.
        if let Err(e) = self
            .docker
            .stop_container(
                &name,
                Some(StopContainerOptions {
                    t: Some(self.config.stop_timeout_secs),
                    ..Default::default()
                }),
            )
            .await
            && !is_not_found(&e)
        {
            return Err(err(&format!("stop container {name}"), e));
        }
        if let Err(e) = self
            .docker
            .remove_container(
                &name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            && !is_not_found(&e)
        {
            return Err(err(&format!("remove container {name}"), e));
        }
        Ok(())
    }
}
