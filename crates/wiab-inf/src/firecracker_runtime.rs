//! Firecracker implementation of the [`VmRuntime`] port.
//!
//! Boots one microVM per agent: the read-only role image (`/dev/vda`) + a per-instance writable
//! overlay (`/dev/vdb`) + a read-only agent drive (`/dev/vdc`) carrying the `wiab-agent` binary
//! and the agent's config. The guest kernel + initramfs assemble an overlayfs root (lower = the
//! role image, upper = the overlay). Firecracker is launched under `jailer` for isolation; the
//! privileged steps (tap creation, `jailer`) run via `sudo` using the locked-down sudoers rule
//! the IaC installs.
//!
//! This runs only on a Linux host with `/dev/kvm`; elsewhere the bootstrap selects
//! `InMemoryVmRuntime`. It shells out and cannot be exercised without KVM, so it is verified on
//! the demo VM, not in local/CI builds.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use tokio::process::Command;
use wiab_app::{RuntimeHandle, VmRuntime, VmRuntimeError, VmSpec};

/// Filesystem + network layout, read from env with defaults matching the IaC.
#[derive(Clone, Debug)]
pub struct FirecrackerConfig {
    /// Holds `base.ext4`, `developer.ext4`, `vmlinux`, `initramfs` (pulled by wiab-deploy).
    pub images_dir: PathBuf,
    /// Per-instance working dirs (`<vms_dir>/<vm-id>/`).
    pub vms_dir: PathBuf,
    /// Holds the deployed `wiab-agent` binary.
    pub agent_dir: PathBuf,
    /// Base dir jailer chroots under (`<jail_base>/firecracker/<id>/root`).
    pub jail_base: PathBuf,
    pub jailer_bin: PathBuf,
    pub firecracker_bin: PathBuf,
    /// Privileged control helper (runs via sudo) used to reach a jailed firecracker's API socket
    /// and to kill it — the jail chroot is owned by the jail uid, so the backend can't do either
    /// directly. See `scripts/provision.sh` (`wiab-vmm-ctl`) in the IaC.
    pub vmm_ctl_bin: PathBuf,
    /// First three octets of `WIAB_MICROVM_SUBNET` (e.g. `172.16.0`).
    pub subnet_prefix: String,
    /// Size of the per-instance overlay drive, MiB.
    pub overlay_mib: u64,
    /// vcpus/mem come from the spec; this is the model endpoint passed to the guest agent.
    pub model_endpoint: String,
    /// uid/gid the jailed firecracker drops to.
    pub jail_uid: u32,
    pub jail_gid: u32,
}

impl FirecrackerConfig {
    pub fn from_env() -> Self {
        let get = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_owned());
        let subnet = get("WIAB_MICROVM_SUBNET", "172.16.0.0/24");
        let subnet_prefix = subnet
            .split('/')
            .next()
            .unwrap_or("172.16.0.0")
            .rsplit_once('.')
            .map(|(head, _)| head.to_owned())
            .unwrap_or_else(|| "172.16.0".to_owned());
        Self {
            images_dir: get("WIAB_IMAGES_DIR", "/var/lib/wiab/images").into(),
            vms_dir: get("WIAB_VMS_DIR", "/var/lib/wiab/vms").into(),
            agent_dir: get("WIAB_AGENT_DIR", "/var/lib/wiab/agent").into(),
            jail_base: get("WIAB_JAIL_BASE", "/srv/jailer").into(),
            jailer_bin: get("WIAB_JAILER_BIN", "/usr/local/bin/jailer").into(),
            firecracker_bin: get("WIAB_FIRECRACKER_BIN", "/usr/local/bin/firecracker").into(),
            vmm_ctl_bin: get("WIAB_VMM_CTL_BIN", "/usr/local/bin/wiab-vmm-ctl").into(),
            subnet_prefix,
            overlay_mib: get("WIAB_VM_OVERLAY_MIB", "2048").parse().unwrap_or(2048),
            model_endpoint: get("WIAB_MODEL_ENDPOINT", ""),
            jail_uid: get("WIAB_JAIL_UID", "123").parse().unwrap_or(123),
            jail_gid: get("WIAB_JAIL_GID", "100").parse().unwrap_or(100),
        }
    }
}

/// Turns a shell-out into a `VmRuntimeError` on non-zero exit.
async fn run(cmd: &mut Command) -> Result<(), VmRuntimeError> {
    let output = cmd
        .output()
        .await
        .map_err(|e| VmRuntimeError::Runtime(format!("spawn failed: {e}")))?;
    if !output.status.success() {
        return Err(VmRuntimeError::Runtime(format!(
            "command {:?} failed ({}): {}",
            cmd.as_std().get_program(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
        )));
    }
    Ok(())
}

pub struct FirecrackerRuntime {
    config: FirecrackerConfig,
}

impl FirecrackerRuntime {
    pub fn new(config: FirecrackerConfig) -> Self {
        Self { config }
    }

    fn vm_number(id: &str) -> u64 {
        id.strip_prefix("VM-")
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    }

    /// Guest gets `<prefix>.(n+1)` (so n=1 → .2), gateway is `<prefix>.1`.
    fn guest_ip(&self, n: u64) -> String {
        format!("{}.{}", self.config.subnet_prefix, n + 1)
    }

    fn gateway_ip(&self) -> String {
        format!("{}.1", self.config.subnet_prefix)
    }

    fn tap_name(id: &str) -> String {
        // Interface names cap at 15 chars: "fc-<number>".
        format!("fc-{}", Self::vm_number(id))
    }

    fn jail_root(&self, id: &str) -> PathBuf {
        self.config
            .jail_base
            .join("firecracker")
            .join(id)
            .join("root")
    }

    /// Create + bring up a tap on the microVM subnet, addressed with the shared gateway IP.
    async fn create_tap(&self, tap: &str) -> Result<(), VmRuntimeError> {
        let gw = self.gateway_ip();
        // Idempotent-ish: delete any stale tap first (ignore failure), then create.
        let _ = Command::new("sudo")
            .args(["ip", "tuntap", "del", "dev", tap, "mode", "tap"])
            .output()
            .await;
        // `user <jail_uid>` hands the tap to the uid the jailed firecracker drops to, so it can
        // open the tap without CAP_NET_ADMIN.
        let jail_uid = self.config.jail_uid.to_string();
        run(Command::new("sudo").args([
            "ip", "tuntap", "add", "dev", tap, "mode", "tap", "user", &jail_uid,
        ]))
        .await?;
        run(Command::new("sudo").args(["ip", "addr", "add", &format!("{gw}/24"), "dev", tap]))
            .await?;
        run(Command::new("sudo").args(["ip", "link", "set", "dev", tap, "up"])).await
    }

    async fn delete_tap(tap: &str) {
        let _ = Command::new("sudo")
            .args(["ip", "tuntap", "del", "dev", tap, "mode", "tap"])
            .output()
            .await;
    }

    /// Create a blank ext4 overlay drive of `overlay_mib` MiB at `path`.
    async fn create_overlay(&self, path: &std::path::Path) -> Result<(), VmRuntimeError> {
        run(Command::new("truncate").args([
            "-s",
            &format!("{}M", self.config.overlay_mib),
            &path.to_string_lossy(),
        ]))
        .await?;
        run(Command::new("mkfs.ext4").args(["-F", &path.to_string_lossy()])).await
    }

    /// Build the read-only agent drive: a tiny ext4 holding `wiab-agent` + `agent.env`.
    async fn build_agent_drive(
        &self,
        stage: &std::path::Path,
        out: &std::path::Path,
        spec: &VmSpec,
    ) -> Result<(), VmRuntimeError> {
        let ioerr = |e: std::io::Error| VmRuntimeError::Runtime(format!("agent drive io: {e}"));
        std::fs::create_dir_all(stage).map_err(ioerr)?;
        std::fs::copy(
            self.config.agent_dir.join("wiab-agent"),
            stage.join("wiab-agent"),
        )
        .map_err(ioerr)?;
        let env = format!(
            "WIAB_AGENT_ID={}\nWIAB_MODEL_ENDPOINT={}\n",
            spec.agent_id, self.config.model_endpoint
        );
        std::fs::write(stage.join("agent.env"), env).map_err(ioerr)?;
        // mke2fs -d populates directly from the staging dir (no mount/root).
        run(Command::new("truncate").args(["-s", "64M", &out.to_string_lossy()])).await?;
        run(Command::new("mke2fs").args([
            "-F",
            "-t",
            "ext4",
            "-d",
            &stage.to_string_lossy(),
            &out.to_string_lossy(),
        ]))
        .await
    }
}

impl VmRuntime for FirecrackerRuntime {
    async fn launch(&self, spec: VmSpec) -> Result<RuntimeHandle, VmRuntimeError> {
        let n = Self::vm_number(&spec.id);
        let guest_ip = self.guest_ip(n);
        let gateway = self.gateway_ip();
        let tap = Self::tap_name(&spec.id);
        let root = self.jail_root(&spec.id);
        let ioerr = |e: std::io::Error| VmRuntimeError::Runtime(format!("io: {e}"));

        // Jailer chroots into <root>; place all resources there and reference them relatively.
        std::fs::create_dir_all(&root).map_err(ioerr)?;

        // Role image (read-only lower): hardlink into the jail (avoids copying GBs).
        let role_image = self
            .config
            .images_dir
            .join(format!("{}.ext4", spec.template));
        let jailed_rootfs = root.join("rootfs.ext4");
        let _ = std::fs::remove_file(&jailed_rootfs);
        std::fs::hard_link(&role_image, &jailed_rootfs)
            .or_else(|_| std::fs::copy(&role_image, &jailed_rootfs).map(|_| ()))
            .map_err(ioerr)?;
        // Kernel + initramfs.
        for name in ["vmlinux", "initramfs"] {
            std::fs::copy(self.config.images_dir.join(name), root.join(name)).map_err(ioerr)?;
        }
        // Per-instance overlay + agent drive.
        self.create_overlay(&root.join("overlay.ext4")).await?;
        let stage = self.config.vms_dir.join(&spec.id).join("agent-stage");
        self.build_agent_drive(&stage, &root.join("agent.ext4"), &spec)
            .await?;

        // Firecracker boot config (paths relative to the chroot).
        let boot_args = format!(
            "console=ttyS0 reboot=k panic=1 pci=off init=/sbin/init \
             ip={guest_ip}::{gateway}:255.255.255.0::eth0:off"
        );
        let config = serde_json::json!({
            "boot-source": {
                "kernel_image_path": "vmlinux",
                "initrd_path": "initramfs",
                "boot_args": boot_args,
            },
            "drives": [
                { "drive_id": "rootfs", "path_on_host": "rootfs.ext4", "is_root_device": false, "is_read_only": true },
                { "drive_id": "overlay", "path_on_host": "overlay.ext4", "is_root_device": false, "is_read_only": false },
                { "drive_id": "agent", "path_on_host": "agent.ext4", "is_root_device": false, "is_read_only": true }
            ],
            "machine-config": { "vcpu_count": spec.vcpus, "mem_size_mib": spec.mem_mib },
            "network-interfaces": [
                { "iface_id": "eth0", "host_dev_name": tap, "guest_mac": mac_for(n) }
            ],
            "vsock": { "guest_cid": 3, "uds_path": "vsock.sock" }
        });
        std::fs::write(
            root.join("config.json"),
            serde_json::to_vec_pretty(&config)
                .map_err(|e| VmRuntimeError::Runtime(e.to_string()))?,
        )
        .map_err(ioerr)?;

        // The jailed firecracker runs as jail_uid/jail_gid, but every file above was created by our
        // own uid, and we can't chown to another uid without privilege. Widen perms instead so the
        // jailed process can use its chroot: 0777 on the instance dir (to create its vsock/API
        // sockets in the chroot cwd) and 0666 on the writable overlay drive. The read-only images
        // stay world-readable (0644), so no change is needed for them.
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o777)).map_err(ioerr)?;
        std::fs::set_permissions(
            root.join("overlay.ext4"),
            std::fs::Permissions::from_mode(0o666),
        )
        .map_err(ioerr)?;

        // Host networking, then launch firecracker under jailer (detached; it runs the guest).
        self.create_tap(&tap).await?;
        let child = Command::new("sudo")
            .arg(self.config.jailer_bin.to_string_lossy().to_string())
            .args([
                "--id",
                &spec.id,
                "--uid",
                &self.config.jail_uid.to_string(),
                "--gid",
                &self.config.jail_gid.to_string(),
                "--chroot-base-dir",
                &self.config.jail_base.to_string_lossy(),
                "--exec-file",
                &self.config.firecracker_bin.to_string_lossy(),
                "--",
                // Expose the API socket (not --no-api) so the backend keeps a control handle on
                // the VM — graceful shutdown (SendCtrlAltDel), pause/snapshot later — reached via
                // the privileged control helper (the socket lives inside the jail uid's chroot).
                "--api-sock",
                "firecracker.sock",
                "--config-file",
                "config.json",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| VmRuntimeError::Runtime(format!("jailer spawn: {e}")))?;
        let pid = child.id().map(|p| p as i64).unwrap_or(-1);
        // Detach: the jailer/firecracker process outlives this call and runs the guest.
        std::mem::forget(child);

        // Start listening for this VM's agent check-ins over vsock.
        crate::vm_comms_broker::spawn_agent_listener(
            crate::vm_comms_broker::agent_socket_path(&root.join("vsock.sock")),
            spec.id.clone(),
        );

        Ok(RuntimeHandle { guest_ip, pid })
    }

    async fn shutdown(&self, vm_id: &str) -> Result<(), VmRuntimeError> {
        // Graceful stop: ask firecracker to Ctrl-Alt-Del the guest (a clean systemd shutdown), then
        // wait for firecracker to exit on its own; only force-kill if it doesn't. Both the API call
        // and the kill go through the privileged helper — the jail chroot (and so the API socket)
        // is owned by the jail uid, unreachable by the backend directly.
        let ctl = self.config.vmm_ctl_bin.to_string_lossy().to_string();
        let _ = Command::new("sudo")
            .args([ctl.as_str(), vm_id, "ctrl-alt-del"])
            .output()
            .await;
        // firecracker's cmdline carries `--id <vm_id>`; poll for it to disappear (up to ~15s).
        let pattern = format!("firecracker --id {vm_id}");
        let mut exited = false;
        for _ in 0..15 {
            let alive = Command::new("pgrep")
                .args(["-f", &pattern])
                .output()
                .await
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !alive {
                exited = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        if !exited {
            let _ = Command::new("sudo")
                .args([ctl.as_str(), vm_id, "kill"])
                .output()
                .await;
        }
        Self::delete_tap(&Self::tap_name(vm_id)).await;
        let _ = std::fs::remove_dir_all(self.config.vms_dir.join(vm_id));
        let _ = std::fs::remove_dir_all(
            self.jail_root(vm_id)
                .parent()
                .unwrap_or(&self.config.jail_base),
        );
        Ok(())
    }
}

/// Deterministic locally-administered MAC from the VM number.
fn mac_for(n: u64) -> String {
    let b = n.to_le_bytes();
    format!(
        "06:00:{:02x}:{:02x}:{:02x}:{:02x}",
        b[2],
        b[1],
        b[0],
        (n & 0xff) as u8
    )
}
