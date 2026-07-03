use crate::agent::{AgentError, AgentId, AgentSnapshot};
use crate::organization::OrganizationId;
use crate::vm::{VmId, VmTemplate};

/// An agent: an `A-###` id, the organization it belongs to, a name, and a description.
/// Agents belong to an organization, not to a project.
///
/// An agent may be given a **VM type** (`vm_template`) and **activated**: activation records
/// the `Vm` booted for it and flips it `active`; deactivation clears that. The VM type can only
/// be changed while inactive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    id: AgentId,
    organization_id: OrganizationId,
    name: String,
    description: String,
    vm_template: Option<VmTemplate>,
    active: bool,
    vm_id: Option<VmId>,
}

impl Agent {
    pub fn new(
        id: AgentId,
        organization_id: OrganizationId,
        name: String,
        description: String,
    ) -> Result<Self, AgentError> {
        if name.trim().is_empty() {
            return Err(AgentError::EmptyName);
        }
        Ok(Self {
            id,
            organization_id,
            name,
            description,
            vm_template: None,
            active: false,
            vm_id: None,
        })
    }

    /// Rebuild an agent from persisted fields. Used by repository implementations to rehydrate;
    /// application code goes through [`Agent::new`] and the mutators.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        id: AgentId,
        organization_id: OrganizationId,
        name: String,
        description: String,
        vm_template: Option<VmTemplate>,
        active: bool,
        vm_id: Option<VmId>,
    ) -> Self {
        Self {
            id,
            organization_id,
            name,
            description,
            vm_template,
            active,
            vm_id,
        }
    }

    pub fn id(&self) -> AgentId {
        self.id
    }

    pub fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn vm_template(&self) -> Option<&VmTemplate> {
        self.vm_template.as_ref()
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn vm_id(&self) -> Option<VmId> {
        self.vm_id
    }

    pub fn update(&mut self, name: String, description: String) -> Result<(), AgentError> {
        if name.trim().is_empty() {
            return Err(AgentError::EmptyName);
        }
        self.name = name;
        self.description = description;
        Ok(())
    }

    /// Assign (or clear) the VM type. Only legal while inactive.
    pub fn assign_vm_template(&mut self, template: Option<VmTemplate>) -> Result<(), AgentError> {
        if self.active {
            return Err(AgentError::ActiveTemplateChange);
        }
        self.vm_template = template;
        Ok(())
    }

    /// Mark the agent active with the VM booted for it. Requires a VM type and inactive state.
    pub fn activate(&mut self, vm_id: VmId) -> Result<(), AgentError> {
        if self.active {
            return Err(AgentError::AlreadyActive);
        }
        if self.vm_template.is_none() {
            return Err(AgentError::NoVmTemplate);
        }
        self.active = true;
        self.vm_id = Some(vm_id);
        Ok(())
    }

    /// Mark the agent inactive and forget its VM. Only legal while active.
    pub fn deactivate(&mut self) -> Result<(), AgentError> {
        if !self.active {
            return Err(AgentError::NotActive);
        }
        self.active = false;
        self.vm_id = None;
        Ok(())
    }

    pub fn snapshot(&self) -> AgentSnapshot {
        AgentSnapshot {
            id: self.id.to_string(),
            organization_id: self.organization_id.to_string(),
            name: self.name.clone(),
            description: self.description.clone(),
            vm_type: self.vm_template.as_ref().map(|t| t.name().to_owned()),
            active: self.active,
            vm_id: self.vm_id.map(|v| v.to_string()),
            guest_ip: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(number: u64, name: &str) -> Agent {
        Agent::new(
            AgentId::from_number(number),
            OrganizationId::from_number(1),
            name.to_owned(),
            String::new(),
        )
        .unwrap()
    }

    fn developer() -> VmTemplate {
        VmTemplate::new("developer").unwrap()
    }

    #[test]
    fn rejects_empty_name() {
        let error = Agent::new(
            AgentId::from_number(1),
            OrganizationId::from_number(1),
            "  ".to_owned(),
            String::new(),
        )
        .unwrap_err();
        assert_eq!(error, AgentError::EmptyName);
    }

    #[test]
    fn new_agent_is_inactive_without_template() {
        let agent = agent(1, "Scout");
        assert!(!agent.is_active());
        assert_eq!(agent.vm_template(), None);
        assert_eq!(agent.vm_id(), None);
    }

    #[test]
    fn exposes_getters() {
        let agent = Agent::new(
            AgentId::from_number(1),
            OrganizationId::from_number(2),
            "Scout".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        assert_eq!(agent.id(), AgentId::from_number(1));
        assert_eq!(agent.organization_id(), OrganizationId::from_number(2));
        assert_eq!(agent.name(), "Scout");
        assert_eq!(agent.description(), "desc");
    }

    #[test]
    fn update_replaces_name_and_description_but_not_organization() {
        let mut agent = agent(1, "Scout");
        agent
            .update("Builder".to_owned(), "ships code".to_owned())
            .unwrap();
        assert_eq!(agent.name(), "Builder");
        assert_eq!(agent.description(), "ships code");
        assert_eq!(agent.organization_id(), OrganizationId::from_number(1));
    }

    #[test]
    fn update_rejects_empty_name() {
        let mut agent = agent(1, "Scout");
        let error = agent
            .update("  ".to_owned(), "ships code".to_owned())
            .unwrap_err();
        assert_eq!(error, AgentError::EmptyName);
        assert_eq!(agent.name(), "Scout");
        assert_eq!(agent.description(), "");
    }

    #[test]
    fn assign_vm_template_sets_it() {
        let mut agent = agent(1, "Scout");
        agent.assign_vm_template(Some(developer())).unwrap();
        assert_eq!(agent.vm_template(), Some(&developer()));
    }

    #[test]
    fn activate_requires_a_template() {
        let mut agent = agent(1, "Scout");
        assert_eq!(
            agent.activate(VmId::from_number(1)).unwrap_err(),
            AgentError::NoVmTemplate
        );
    }

    #[test]
    fn activate_then_deactivate_round_trip() {
        let mut agent = agent(1, "Scout");
        agent.assign_vm_template(Some(developer())).unwrap();
        agent.activate(VmId::from_number(7)).unwrap();
        assert!(agent.is_active());
        assert_eq!(agent.vm_id(), Some(VmId::from_number(7)));

        assert_eq!(
            agent.activate(VmId::from_number(8)).unwrap_err(),
            AgentError::AlreadyActive
        );

        agent.deactivate().unwrap();
        assert!(!agent.is_active());
        assert_eq!(agent.vm_id(), None);
        assert_eq!(agent.deactivate().unwrap_err(), AgentError::NotActive);
    }

    #[test]
    fn template_cannot_change_while_active() {
        let mut agent = agent(1, "Scout");
        agent.assign_vm_template(Some(developer())).unwrap();
        agent.activate(VmId::from_number(1)).unwrap();
        assert_eq!(
            agent.assign_vm_template(Some(VmTemplate::new("base").unwrap())),
            Err(AgentError::ActiveTemplateChange)
        );
    }

    #[test]
    fn snapshot_mirrors_fields() {
        let mut agent = Agent::new(
            AgentId::from_number(1),
            OrganizationId::from_number(2),
            "Scout".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        agent.assign_vm_template(Some(developer())).unwrap();
        agent.activate(VmId::from_number(5)).unwrap();
        let snapshot = agent.snapshot();
        assert_eq!(snapshot.id, "A-1");
        assert_eq!(snapshot.organization_id, "O-2");
        assert_eq!(snapshot.name, "Scout");
        assert_eq!(snapshot.description, "desc");
        assert_eq!(snapshot.vm_type.as_deref(), Some("developer"));
        assert!(snapshot.active);
        assert_eq!(snapshot.vm_id.as_deref(), Some("VM-5"));
        assert_eq!(snapshot.guest_ip, None);
    }

    #[test]
    fn from_parts_round_trips_all_fields() {
        let agent = Agent::from_parts(
            AgentId::from_number(2),
            OrganizationId::from_number(3),
            "Scout".to_owned(),
            "desc".to_owned(),
            Some(developer()),
            true,
            Some(VmId::from_number(9)),
        );
        assert_eq!(agent.id(), AgentId::from_number(2));
        assert_eq!(agent.organization_id(), OrganizationId::from_number(3));
        assert_eq!(agent.name(), "Scout");
        assert_eq!(agent.description(), "desc");
        assert_eq!(agent.vm_template(), Some(&developer()));
        assert!(agent.is_active());
        assert_eq!(agent.vm_id(), Some(VmId::from_number(9)));
    }
}
