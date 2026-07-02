use serde::{Deserialize, Serialize};

/// Serializable read view of a `Vm`. Callers and responses use this rather than the domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSnapshot {
    pub id: String,
    pub organization_id: String,
    pub template: String,
    pub state: String,
    /// The guest IP once the VM is running; `None` while `Creating`, `Stopped`, or `Failed`.
    pub guest_ip: Option<String>,
    pub vcpus: u32,
    pub mem_mib: u32,
}
