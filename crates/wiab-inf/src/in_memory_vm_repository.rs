use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::vm::{Vm, VmId, VmRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryVmRepository {
    vms: Arc<RwLock<HashMap<VmId, (Vm, u64)>>>,
}

impl InMemoryVmRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl VmRepository for InMemoryVmRepository {
    async fn save(&self, vm: Vm, expected: Version) -> Result<Version, SaveError> {
        let mut vms = self.vms.write().expect("vm repository write lock poisoned");
        let current = vms.get(&vm.id()).map(|(_, version)| *version).unwrap_or(0);
        if current != expected.value() {
            return Err(SaveError::Conflict);
        }
        let next = expected.next();
        vms.insert(vm.id(), (vm, next.value()));
        Ok(next)
    }

    async fn get(&self, id: &VmId) -> Result<Option<(Vm, Version)>, RepoError> {
        Ok(self
            .vms
            .read()
            .expect("vm repository read lock poisoned")
            .get(id)
            .map(|(vm, version)| (vm.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Vm>, RepoError> {
        Ok(self
            .vms
            .read()
            .expect("vm repository read lock poisoned")
            .values()
            .map(|(vm, _)| vm.clone())
            .collect())
    }
}
