/// Request to provision (create + start) a microVM from a template. `vcpus`/`mem_mib` override
/// the per-VM defaults when set.
pub struct ProvisionVmRequest {
    pub template: String,
    pub vcpus: Option<u32>,
    pub mem_mib: Option<u32>,
    /// Extra environment for the guest, on top of what the runtime always sets. A team uses
    /// this to tell its container which board to poll and which repo to clone — settings that
    /// differ per owner, so they cannot come from the backend's own environment.
    pub env: Vec<(String, String)>,
}
