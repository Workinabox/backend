use crate::organization::OrganizationId;
use crate::vm::{VmError, VmId, VmResources, VmSnapshot, VmState, VmTemplate};

/// A microVM sandbox: a `VM-###` id, the organization it belongs to, the template it boots
/// from, its compute sizing, its lifecycle state, and — once running — its guest IP.
///
/// The lifecycle is a small state machine (see [`VmState`]): a VM is born `Creating`, becomes
/// `Running` when the runtime reports an endpoint, and is either `Stopped` (graceful) or
/// `Failed` (launch error). Transitions are intent-revealing methods that enforce the legal
/// order; `guest_ip` is only ever set while `Running`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vm {
    id: VmId,
    organization_id: OrganizationId,
    template: VmTemplate,
    resources: VmResources,
    state: VmState,
    guest_ip: Option<String>,
}

impl Vm {
    /// Provision a new VM in the `Creating` state, not yet booted.
    pub fn new(
        id: VmId,
        organization_id: OrganizationId,
        template: VmTemplate,
        resources: VmResources,
    ) -> Self {
        Self {
            id,
            organization_id,
            template,
            resources,
            state: VmState::Creating,
            guest_ip: None,
        }
    }

    /// Rebuild a VM from persisted fields. Used by repository implementations to rehydrate an
    /// aggregate; application code goes through [`Vm::new`] and the transition methods.
    pub fn from_parts(
        id: VmId,
        organization_id: OrganizationId,
        template: VmTemplate,
        resources: VmResources,
        state: VmState,
        guest_ip: Option<String>,
    ) -> Self {
        Self {
            id,
            organization_id,
            template,
            resources,
            state,
            guest_ip,
        }
    }

    pub fn id(&self) -> VmId {
        self.id
    }

    pub fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    pub fn template(&self) -> &VmTemplate {
        &self.template
    }

    pub fn resources(&self) -> VmResources {
        self.resources
    }

    pub fn state(&self) -> VmState {
        self.state
    }

    pub fn guest_ip(&self) -> Option<&str> {
        self.guest_ip.as_deref()
    }

    /// Mark the VM running once the runtime has booted it and reported an endpoint.
    /// Only legal from `Creating`.
    pub fn mark_running(&mut self, guest_ip: String) -> Result<(), VmError> {
        if self.state != VmState::Creating {
            return Err(VmError::NotCreating);
        }
        self.state = VmState::Running;
        self.guest_ip = Some(guest_ip);
        Ok(())
    }

    /// Gracefully stop a running VM. Only legal from `Running`.
    pub fn stop(&mut self) -> Result<(), VmError> {
        if self.state != VmState::Running {
            return Err(VmError::NotRunning);
        }
        self.state = VmState::Stopped;
        self.guest_ip = None;
        Ok(())
    }

    /// Mark the VM failed (e.g. the runtime could not boot it). Terminal; clears any endpoint.
    pub fn mark_failed(&mut self) {
        self.state = VmState::Failed;
        self.guest_ip = None;
    }

    pub fn snapshot(&self) -> VmSnapshot {
        VmSnapshot {
            id: self.id.to_string(),
            organization_id: self.organization_id.to_string(),
            template: self.template.to_string(),
            state: self.state.to_string(),
            guest_ip: self.guest_ip.clone(),
            vcpus: self.resources.vcpus(),
            mem_mib: self.resources.mem_mib(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm() -> Vm {
        Vm::new(
            VmId::from_number(1),
            OrganizationId::from_number(1),
            VmTemplate::new("developer").unwrap(),
            VmResources::default(),
        )
    }

    #[test]
    fn new_vm_starts_creating_without_endpoint() {
        let vm = vm();
        assert_eq!(vm.state(), VmState::Creating);
        assert_eq!(vm.guest_ip(), None);
    }

    #[test]
    fn mark_running_from_creating_sets_endpoint() {
        let mut vm = vm();
        vm.mark_running("172.16.0.2".to_owned()).unwrap();
        assert_eq!(vm.state(), VmState::Running);
        assert_eq!(vm.guest_ip(), Some("172.16.0.2"));
    }

    #[test]
    fn mark_running_rejected_when_not_creating() {
        let mut vm = vm();
        vm.mark_running("172.16.0.2".to_owned()).unwrap();
        assert_eq!(
            vm.mark_running("172.16.0.3".to_owned()).unwrap_err(),
            VmError::NotCreating
        );
    }

    #[test]
    fn stop_from_running_clears_endpoint() {
        let mut vm = vm();
        vm.mark_running("172.16.0.2".to_owned()).unwrap();
        vm.stop().unwrap();
        assert_eq!(vm.state(), VmState::Stopped);
        assert_eq!(vm.guest_ip(), None);
    }

    #[test]
    fn stop_rejected_when_not_running() {
        let mut vm = vm();
        assert_eq!(vm.stop().unwrap_err(), VmError::NotRunning);
    }

    #[test]
    fn mark_failed_is_terminal_and_clears_endpoint() {
        let mut vm = vm();
        vm.mark_running("172.16.0.2".to_owned()).unwrap();
        vm.mark_failed();
        assert_eq!(vm.state(), VmState::Failed);
        assert_eq!(vm.guest_ip(), None);
    }

    #[test]
    fn snapshot_mirrors_fields() {
        let mut vm = vm();
        vm.mark_running("172.16.0.2".to_owned()).unwrap();
        let snapshot = vm.snapshot();
        assert_eq!(snapshot.id, "VM-1");
        assert_eq!(snapshot.organization_id, "O-1");
        assert_eq!(snapshot.template, "developer");
        assert_eq!(snapshot.state, "running");
        assert_eq!(snapshot.guest_ip.as_deref(), Some("172.16.0.2"));
        assert_eq!(snapshot.vcpus, 2);
        assert_eq!(snapshot.mem_mib, 1024);
    }
}
