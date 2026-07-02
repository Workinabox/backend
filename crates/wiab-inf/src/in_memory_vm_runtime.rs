use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use wiab_app::{RuntimeHandle, VmRuntime, VmRuntimeError, VmSpec};

/// A `VmRuntime` that boots nothing real but faithfully models VM lifecycle in memory.
///
/// It is the hypervisor seam's in-memory implementation — the analogue of the `InMemory*`
/// repositories — used on hosts without KVM (local dev, CI) and in tests. `launch` records a
/// running VM and hands back a distinct fake endpoint; `shutdown` forgets it. The real
/// `FirecrackerRuntime` slots in behind the same [`VmRuntime`] trait.
#[derive(Debug, Clone, Default)]
pub struct InMemoryVmRuntime {
    running: Arc<RwLock<HashMap<String, RuntimeHandle>>>,
    counter: Arc<AtomicU64>,
}

impl InMemoryVmRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// The ids of the VMs this runtime currently considers running.
    pub fn running_ids(&self) -> Vec<String> {
        self.running
            .read()
            .expect("vm runtime read lock poisoned")
            .keys()
            .cloned()
            .collect()
    }
}

impl VmRuntime for InMemoryVmRuntime {
    async fn launch(&self, spec: VmSpec) -> Result<RuntimeHandle, VmRuntimeError> {
        let n = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        let handle = RuntimeHandle {
            guest_ip: format!("10.0.{}.{}", (n / 256) % 256, n % 256),
            pid: n as i64,
        };
        tracing::warn!(
            "InMemoryVmRuntime: not booting a real microVM for {} (template {})",
            spec.id,
            spec.template
        );
        self.running
            .write()
            .expect("vm runtime write lock poisoned")
            .insert(spec.id, handle.clone());
        Ok(handle)
    }

    async fn shutdown(&self, vm_id: &str) -> Result<(), VmRuntimeError> {
        self.running
            .write()
            .expect("vm runtime write lock poisoned")
            .remove(vm_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str) -> VmSpec {
        VmSpec {
            id: id.to_owned(),
            template: "developer".to_owned(),
            vcpus: 2,
            mem_mib: 1024,
        }
    }

    #[tokio::test]
    async fn launch_records_running_with_distinct_endpoints() {
        let runtime = InMemoryVmRuntime::new();
        let first = runtime.launch(spec("VM-1")).await.unwrap();
        let second = runtime.launch(spec("VM-2")).await.unwrap();
        assert_ne!(first.guest_ip, second.guest_ip);
        let mut running = runtime.running_ids();
        running.sort();
        assert_eq!(running, vec!["VM-1", "VM-2"]);
    }

    #[tokio::test]
    async fn shutdown_forgets_the_vm() {
        let runtime = InMemoryVmRuntime::new();
        runtime.launch(spec("VM-1")).await.unwrap();
        runtime.shutdown("VM-1").await.unwrap();
        assert!(runtime.running_ids().is_empty());
    }
}
