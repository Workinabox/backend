use std::sync::Arc;

use wiab_core::organization::OrganizationRepository;
use wiab_core::vm::{VmRepository, VmSnapshot};

use crate::vm_application_service::VmApplicationService;
use crate::vm_requests::ProvisionVmRequest;
use crate::vm_runtime::VmRuntime;

/// The VM-provisioning capability the agent-activation flow depends on. Implemented by
/// [`VmApplicationService`]; kept as a narrow port so `AgentApplicationService` needs only this,
/// not the vm service's full generics.
#[allow(async_fn_in_trait)]
pub trait VmProvisioning: Send + Sync {
    async fn provision(
        &self,
        organization_id: &str,
        owner_id: &str,
        request: ProvisionVmRequest,
    ) -> anyhow::Result<Option<VmSnapshot>>;
    async fn stop(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>>;
    async fn get(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>>;
}

impl<R: VmRepository, O: OrganizationRepository, RT: VmRuntime> VmProvisioning
    for VmApplicationService<R, O, RT>
{
    async fn provision(
        &self,
        organization_id: &str,
        owner_id: &str,
        request: ProvisionVmRequest,
    ) -> anyhow::Result<Option<VmSnapshot>> {
        self.provision_vm(organization_id, owner_id, request).await
    }

    async fn stop(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>> {
        self.stop_vm(vm_id).await
    }

    async fn get(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>> {
        self.get_vm(vm_id).await
    }
}

/// Lets one `Arc`-shared vm service back several callers — the agent and team services both
/// provision sandboxes, and `VmApplicationService` is not `Clone`.
impl<T: VmProvisioning> VmProvisioning for Arc<T> {
    async fn provision(
        &self,
        organization_id: &str,
        owner_id: &str,
        request: ProvisionVmRequest,
    ) -> anyhow::Result<Option<VmSnapshot>> {
        (**self).provision(organization_id, owner_id, request).await
    }

    async fn stop(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>> {
        (**self).stop(vm_id).await
    }

    async fn get(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>> {
        (**self).get(vm_id).await
    }
}
