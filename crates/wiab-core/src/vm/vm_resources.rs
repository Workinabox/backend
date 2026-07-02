/// Compute resources allocated to a microVM.
///
/// A value object: two VMs with the same vcpu and memory sizing are interchangeable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmResources {
    vcpus: u32,
    mem_mib: u32,
}

impl VmResources {
    pub fn new(vcpus: u32, mem_mib: u32) -> Self {
        Self { vcpus, mem_mib }
    }

    pub fn vcpus(&self) -> u32 {
        self.vcpus
    }

    pub fn mem_mib(&self) -> u32 {
        self.mem_mib
    }
}

impl Default for VmResources {
    /// A small, sensible default for a headless sandbox: 2 vCPUs and 1 GiB of RAM.
    fn default() -> Self {
        Self {
            vcpus: 2,
            mem_mib: 1024,
        }
    }
}
