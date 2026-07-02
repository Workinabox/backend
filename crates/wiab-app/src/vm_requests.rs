/// Request to provision (create + start) a microVM from a template. `vcpus`/`mem_mib` override
/// the per-VM defaults when set.
pub struct ProvisionVmRequest {
    pub template: String,
    pub vcpus: Option<u32>,
    pub mem_mib: Option<u32>,
}
