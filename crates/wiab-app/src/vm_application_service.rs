use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::organization::{OrganizationId, OrganizationRepository};
use wiab_core::repository::{SaveError, Version};
use wiab_core::vm::{Vm, VmId, VmNumbering, VmRepository, VmResources, VmSnapshot, VmTemplate};

use crate::vm_requests::ProvisionVmRequest;
use crate::vm_runtime::{VmRuntime, VmSpec};

/// Orchestrates use cases over the `Vm` aggregate.
///
/// `provision_vm` creates the aggregate (`Creating`), asks the [`VmRuntime`] to boot it, and
/// records the result (`Running` with an endpoint, or `Failed`). `stop_vm` tears the runtime
/// down and moves the aggregate to `Stopped`. Holds the organization repository to verify the
/// parent organization exists, mirroring `AgentApplicationService`.
pub struct VmApplicationService<R: VmRepository, O: OrganizationRepository, RT: VmRuntime> {
    vm_repository: R,
    organization_repository: O,
    runtime: RT,
    numbering: Arc<dyn VmNumbering>,
}

impl<R: VmRepository, O: OrganizationRepository, RT: VmRuntime> VmApplicationService<R, O, RT> {
    pub fn new(
        vm_repository: R,
        organization_repository: O,
        runtime: RT,
        numbering: Arc<dyn VmNumbering>,
    ) -> Self {
        Self {
            vm_repository,
            organization_repository,
            runtime,
            numbering,
        }
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub async fn list_vms(&self, organization_id: &str) -> anyhow::Result<Option<Vec<VmSnapshot>>> {
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let mut vms = self
            .vm_repository
            .list()
            .await?
            .into_iter()
            .filter(|vm| vm.organization_id() == id)
            .collect::<Vec<_>>();
        vms.sort_by_key(|vm| vm.id().number());
        Ok(Some(vms.into_iter().map(|vm| vm.snapshot()).collect()))
    }

    pub async fn get_vm(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>> {
        let id: VmId = vm_id.parse()?;
        Ok(self
            .vm_repository
            .get(&id)
            .await?
            .map(|(vm, _)| vm.snapshot()))
    }

    /// Create a VM and boot it. Returns `Ok(None)` when the organization does not exist.
    ///
    /// The aggregate is persisted `Creating` before the boot attempt so a crash mid-launch
    /// leaves a record. On a runtime failure the VM is marked `Failed` and the error surfaced.
    pub async fn provision_vm(
        &self,
        organization_id: &str,
        request: ProvisionVmRequest,
    ) -> anyhow::Result<Option<VmSnapshot>> {
        let organization_id: OrganizationId = organization_id.parse()?;
        if self
            .organization_repository
            .get(&organization_id)
            .await?
            .is_none()
        {
            return Ok(None);
        }

        let template = VmTemplate::new(request.template)?;
        let defaults = VmResources::default();
        let resources = VmResources::new(
            request.vcpus.unwrap_or(defaults.vcpus()),
            request.mem_mib.unwrap_or(defaults.mem_mib()),
        );
        let mut vm = Vm::new(self.numbering.next(), organization_id, template, resources);
        let version = self.vm_repository.save(vm.clone(), Version::NEW).await?;

        let spec = VmSpec {
            id: vm.id().to_string(),
            template: vm.template().name().to_owned(),
            vcpus: resources.vcpus(),
            mem_mib: resources.mem_mib(),
        };
        match self.runtime.launch(spec).await {
            Ok(handle) => {
                vm.mark_running(handle.guest_ip)?;
                let snapshot = vm.snapshot();
                self.vm_repository.save(vm, version).await?;
                Ok(Some(snapshot))
            }
            Err(error) => {
                vm.mark_failed();
                // Best-effort record of the failure; surface the original launch error.
                let _ = self.vm_repository.save(vm, version).await;
                Err(anyhow!(error))
            }
        }
    }

    /// Stop and tear down a running VM. Returns `Ok(None)` when no VM with the id exists.
    pub async fn stop_vm(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>> {
        let id: VmId = vm_id.parse()?;
        if self.vm_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        self.runtime.shutdown(&id.to_string()).await?;
        loop {
            let Some((mut vm, version)) = self.vm_repository.get(&id).await? else {
                return Ok(None);
            };
            vm.stop()?;
            let snapshot = vm.snapshot();
            match self.vm_repository.save(vm, version).await {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::organization::Organization;
    use wiab_core::repository::{RepoError, SaveError, Version};

    use super::*;
    use crate::vm_runtime::{RuntimeHandle, VmRuntimeError};

    #[derive(Default)]
    struct TestVmRepository {
        vms: RwLock<HashMap<VmId, (Vm, u64)>>,
    }

    impl VmRepository for TestVmRepository {
        async fn save(&self, vm: Vm, expected: Version) -> Result<Version, SaveError> {
            let mut vms = self.vms.write().expect("test repository write lock poisoned");
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
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(vm, version)| (vm.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Vm>, RepoError> {
            Ok(self
                .vms
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(vm, _)| vm.clone())
                .collect())
        }
    }

    #[derive(Default)]
    struct TestOrganizationRepository {
        organizations: RwLock<HashMap<OrganizationId, (Organization, u64)>>,
    }

    impl OrganizationRepository for TestOrganizationRepository {
        async fn save(
            &self,
            organization: Organization,
            expected: Version,
        ) -> Result<Version, SaveError> {
            let mut organizations = self
                .organizations
                .write()
                .expect("test repository write lock poisoned");
            let current = organizations
                .get(&organization.id())
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            organizations.insert(organization.id(), (organization, next.value()));
            Ok(next)
        }

        async fn get(
            &self,
            id: &OrganizationId,
        ) -> Result<Option<(Organization, Version)>, RepoError> {
            Ok(self
                .organizations
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(organization, version)| {
                    (organization.clone(), Version::from_value(*version))
                }))
        }

        async fn list(&self) -> Result<Vec<Organization>, RepoError> {
            Ok(self
                .organizations
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(organization, _)| organization.clone())
                .collect())
        }
    }

    #[derive(Default)]
    struct TestVmNumbering {
        counter: AtomicU64,
    }

    impl VmNumbering for TestVmNumbering {
        fn next(&self) -> VmId {
            VmId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    /// Runtime that always boots successfully at a fixed endpoint (stands in for the no-op
    /// runtime on hosts without KVM).
    #[derive(Default)]
    struct StubRuntime;

    impl VmRuntime for StubRuntime {
        async fn launch(&self, _spec: VmSpec) -> Result<RuntimeHandle, VmRuntimeError> {
            Ok(RuntimeHandle {
                guest_ip: "172.16.0.2".to_owned(),
                pid: 4242,
            })
        }

        async fn shutdown(&self, _vm_id: &str) -> Result<(), VmRuntimeError> {
            Ok(())
        }
    }

    /// Runtime whose launch always fails.
    #[derive(Default)]
    struct FailingRuntime;

    impl VmRuntime for FailingRuntime {
        async fn launch(&self, _spec: VmSpec) -> Result<RuntimeHandle, VmRuntimeError> {
            Err(VmRuntimeError::Runtime("no kvm".to_owned()))
        }

        async fn shutdown(&self, _vm_id: &str) -> Result<(), VmRuntimeError> {
            Ok(())
        }
    }

    fn service<RT: VmRuntime>(
        runtime: RT,
    ) -> VmApplicationService<TestVmRepository, TestOrganizationRepository, RT> {
        VmApplicationService::new(
            TestVmRepository::default(),
            TestOrganizationRepository::default(),
            runtime,
            Arc::new(TestVmNumbering::default()),
        )
    }

    async fn seed_organization<RT: VmRuntime>(
        service: &VmApplicationService<TestVmRepository, TestOrganizationRepository, RT>,
        number: u64,
    ) -> String {
        let organization = Organization::new(
            OrganizationId::from_number(number),
            format!("Org {number}"),
            String::new(),
        )
        .unwrap();
        let id = organization.id().to_string();
        service
            .organization_repository
            .save(organization, Version::NEW)
            .await
            .unwrap();
        id
    }

    fn provision(template: &str) -> ProvisionVmRequest {
        ProvisionVmRequest {
            template: template.to_owned(),
            vcpus: None,
            mem_mib: None,
        }
    }

    #[tokio::test]
    async fn provision_boots_to_running_with_endpoint() {
        let service = service(StubRuntime);
        let organization_id = seed_organization(&service, 1).await;
        let vm = service
            .provision_vm(&organization_id, provision("developer"))
            .await
            .unwrap()
            .expect("organization should exist");
        assert_eq!(vm.id, "VM-1");
        assert_eq!(vm.template, "developer");
        assert_eq!(vm.state, "running");
        assert_eq!(vm.guest_ip.as_deref(), Some("172.16.0.2"));
    }

    #[tokio::test]
    async fn provision_get_stop_round_trip() {
        let service = service(StubRuntime);
        let organization_id = seed_organization(&service, 1).await;
        let vm = service
            .provision_vm(&organization_id, provision("developer"))
            .await
            .unwrap()
            .unwrap();

        let fetched = service.get_vm(&vm.id).await.unwrap().unwrap();
        assert_eq!(fetched.state, "running");

        let stopped = service.stop_vm(&vm.id).await.unwrap().unwrap();
        assert_eq!(stopped.state, "stopped");
        assert_eq!(stopped.guest_ip, None);

        let reloaded = service.get_vm(&vm.id).await.unwrap().unwrap();
        assert_eq!(reloaded.state, "stopped");
    }

    #[tokio::test]
    async fn provision_applies_resource_overrides() {
        let service = service(StubRuntime);
        let organization_id = seed_organization(&service, 1).await;
        let vm = service
            .provision_vm(
                &organization_id,
                ProvisionVmRequest {
                    template: "developer".to_owned(),
                    vcpus: Some(4),
                    mem_mib: Some(8192),
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(vm.vcpus, 4);
        assert_eq!(vm.mem_mib, 8192);
    }

    #[tokio::test]
    async fn provision_under_missing_organization_returns_none() {
        let service = service(StubRuntime);
        let result = service
            .provision_vm("O-9", provision("developer"))
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn provision_rejects_empty_template() {
        let service = service(StubRuntime);
        let organization_id = seed_organization(&service, 1).await;
        assert!(
            service
                .provision_vm(&organization_id, provision("  "))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn provision_marks_failed_and_errors_when_runtime_fails() {
        let service = service(FailingRuntime);
        let organization_id = seed_organization(&service, 1).await;
        let result = service
            .provision_vm(&organization_id, provision("developer"))
            .await;
        assert!(result.is_err());
        // The failed VM is still recorded (VM-1), in the Failed state.
        let vm = service.get_vm("VM-1").await.unwrap().expect("recorded");
        assert_eq!(vm.state, "failed");
        assert_eq!(vm.guest_ip, None);
    }

    #[tokio::test]
    async fn list_vms_partitions_by_organization() {
        let service = service(StubRuntime);
        let first = seed_organization(&service, 1).await;
        let second = seed_organization(&service, 2).await;
        service
            .provision_vm(&first, provision("base"))
            .await
            .unwrap();
        service
            .provision_vm(&second, provision("developer"))
            .await
            .unwrap();
        service
            .provision_vm(&first, provision("developer"))
            .await
            .unwrap();

        let first_ids = service
            .list_vms(&first)
            .await
            .unwrap()
            .unwrap()
            .into_iter()
            .map(|vm| vm.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["VM-1", "VM-3"]);

        let second_ids = service
            .list_vms(&second)
            .await
            .unwrap()
            .unwrap()
            .into_iter()
            .map(|vm| vm.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["VM-2"]);
    }

    #[tokio::test]
    async fn stop_missing_vm_returns_none() {
        let service = service(StubRuntime);
        assert!(service.stop_vm("VM-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_vm_rejects_malformed_id() {
        let service = service(StubRuntime);
        assert!(service.get_vm("bogus").await.is_err());
    }
}
