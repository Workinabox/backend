use thiserror::Error;

/// What the runtime needs in order to boot a microVM. Built by the application service from a
/// `Vm` aggregate; the runtime resolves the template name to a concrete rootfs/kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmSpec {
    pub id: String,
    pub template: String,
    pub vcpus: u32,
    pub mem_mib: u32,
}

/// What the runtime reports back once a microVM has booted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeHandle {
    pub guest_ip: String,
    pub pid: i64,
}

#[derive(Debug, Error)]
pub enum VmRuntimeError {
    #[error("vm runtime error: {0}")]
    Runtime(String),
}

/// Port for the hypervisor that actually runs microVMs. Defined here as an application
/// dependency; infrastructure provides the Firecracker implementation (and a no-op one for
/// hosts without KVM). Launch allocates host resources (tap, overlay), boots the guest, and
/// returns its endpoint; shutdown tears everything down. Keyed by the string VM id.
#[allow(async_fn_in_trait)]
pub trait VmRuntime: Send + Sync {
    async fn launch(&self, spec: VmSpec) -> Result<RuntimeHandle, VmRuntimeError>;
    async fn shutdown(&self, vm_id: &str) -> Result<(), VmRuntimeError>;
}
