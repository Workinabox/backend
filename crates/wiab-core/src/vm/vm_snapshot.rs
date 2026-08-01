use serde::{Deserialize, Serialize};

/// Serializable read view of a `Vm`. Callers and responses use this rather than the domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSnapshot {
    pub id: String,
    pub organization_id: String,
    /// The agent or team the sandbox was booted for (`A-3` or `TM-1`).
    pub owner_id: String,
    pub template: String,
    pub state: String,
    /// The guest IP once the VM is running; `None` while `Creating`, `Stopped`, or `Failed`.
    pub guest_ip: Option<String>,
    pub vcpus: u32,
    pub mem_mib: u32,
}
