use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::agent::{
    Agent, AgentError, AgentId, AgentNumbering, AgentRepository, AgentSnapshot,
};
use wiab_core::organization::{OrganizationId, OrganizationRepository};
use wiab_core::repository::{SaveError, Version};
use wiab_core::vm::{VmId, VmTemplate};

use crate::agent_requests::{CreateAgentRequest, UpdateAgentRequest};
use crate::vm_provisioning::VmProvisioning;
use crate::vm_requests::ProvisionVmRequest;

/// Orchestrates use cases over the `Agent` aggregate, including **activation** — which
/// provisions a VM of the agent's assigned type (via the [`VmProvisioning`] port) and records it
/// on the agent — and its inverse. Mutations use optimistic concurrency: load with version,
/// apply, retry on conflict. Holds the organization repository to verify the parent org exists.
pub struct AgentApplicationService<A: AgentRepository, O: OrganizationRepository, V: VmProvisioning>
{
    agent_repository: A,
    organization_repository: O,
    vm: V,
    numbering: Arc<dyn AgentNumbering>,
}

impl<A: AgentRepository, O: OrganizationRepository, V: VmProvisioning>
    AgentApplicationService<A, O, V>
{
    pub fn new(
        agent_repository: A,
        organization_repository: O,
        vm: V,
        numbering: Arc<dyn AgentNumbering>,
    ) -> Self {
        Self {
            agent_repository,
            organization_repository,
            vm,
            numbering,
        }
    }

    fn parse_template(vm_type: Option<String>) -> anyhow::Result<Option<VmTemplate>> {
        Ok(vm_type.map(VmTemplate::new).transpose()?)
    }

    /// Build a snapshot enriched with the active VM's guest IP.
    async fn enrich(&self, agent: &Agent) -> anyhow::Result<AgentSnapshot> {
        let mut snapshot = agent.snapshot();
        if let Some(vm_id) = agent.vm_id() {
            let vm = self.vm.get(&vm_id.to_string()).await?;
            if let Some(vm) = vm {
                snapshot.guest_ip = vm.guest_ip;
            }
        }
        Ok(snapshot)
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub async fn list_agents(
        &self,
        organization_id: &str,
    ) -> anyhow::Result<Option<Vec<AgentSnapshot>>> {
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let mut agents = self
            .agent_repository
            .list()
            .await?
            .into_iter()
            .filter(|agent| agent.organization_id() == id)
            .collect::<Vec<_>>();
        agents.sort_by_key(|agent| agent.id().number());
        let mut snapshots = Vec::with_capacity(agents.len());
        for agent in &agents {
            snapshots.push(self.enrich(agent).await?);
        }
        Ok(Some(snapshots))
    }

    pub async fn agent_snapshot(&self, agent_id: &str) -> anyhow::Result<Option<AgentSnapshot>> {
        let id: AgentId = agent_id.parse()?;
        match self.agent_repository.get(&id).await? {
            None => Ok(None),
            Some((agent, _)) => Ok(Some(self.enrich(&agent).await?)),
        }
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub async fn create_agent(
        &self,
        organization_id: &str,
        request: CreateAgentRequest,
    ) -> anyhow::Result<Option<AgentSnapshot>> {
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let mut agent = Agent::new(self.numbering.next(), id, request.name, request.description)?;
        agent.assign_vm_template(Self::parse_template(request.vm_type)?)?;
        let snapshot = agent.snapshot();
        self.agent_repository.save(agent, Version::NEW).await?;
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no agent with the given id exists. The VM type is only changed when
    /// it differs (so a plain name/description edit of an active agent is allowed; changing the VM
    /// type while active is rejected by the aggregate).
    pub async fn update_agent(
        &self,
        agent_id: &str,
        request: UpdateAgentRequest,
    ) -> anyhow::Result<Option<AgentSnapshot>> {
        let id: AgentId = agent_id.parse()?;
        let desired = Self::parse_template(request.vm_type)?;
        loop {
            let Some((mut agent, version)) = self.agent_repository.get(&id).await? else {
                return Ok(None);
            };
            agent.update(request.name.clone(), request.description.clone())?;
            if agent.vm_template() != desired.as_ref() {
                agent.assign_vm_template(desired.clone())?;
            }
            let snapshot = agent.snapshot();
            match self.agent_repository.save(agent, version).await {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    /// Activate the agent: provision a VM of its assigned type and record it. `Ok(None)` when no
    /// agent with the given id exists.
    pub async fn activate_agent(&self, agent_id: &str) -> anyhow::Result<Option<AgentSnapshot>> {
        let id: AgentId = agent_id.parse()?;
        let Some((agent, _)) = self.agent_repository.get(&id).await? else {
            return Ok(None);
        };
        if agent.is_active() {
            return Err(anyhow!(AgentError::AlreadyActive));
        }
        let Some(template) = agent.vm_template() else {
            return Err(anyhow!(AgentError::NoVmTemplate));
        };
        let organization_id = agent.organization_id().to_string();
        let request = ProvisionVmRequest {
            template: template.name().to_owned(),
            vcpus: None,
            mem_mib: None,
            // An agent's container is configured entirely from the backend's own environment.
            env: Vec::new(),
        };
        let Some(vm) = self
            .vm
            .provision(&organization_id, agent_id, request)
            .await?
        else {
            return Ok(None);
        };
        let vm_id: VmId = vm.id.parse()?;
        loop {
            let Some((mut agent, version)) = self.agent_repository.get(&id).await? else {
                return Ok(None);
            };
            agent.activate(vm_id)?;
            let mut snapshot = agent.snapshot();
            match self.agent_repository.save(agent, version).await {
                Ok(_) => {
                    snapshot.guest_ip = vm.guest_ip.clone();
                    return Ok(Some(snapshot));
                }
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }

    /// Deactivate the agent: stop its VM and clear it. `Ok(None)` when no agent with the id exists.
    pub async fn deactivate_agent(&self, agent_id: &str) -> anyhow::Result<Option<AgentSnapshot>> {
        let id: AgentId = agent_id.parse()?;
        let Some((agent, _)) = self.agent_repository.get(&id).await? else {
            return Ok(None);
        };
        if !agent.is_active() {
            return Err(anyhow!(AgentError::NotActive));
        }
        if let Some(vm_id) = agent.vm_id() {
            self.vm.stop(&vm_id.to_string()).await?;
        }
        loop {
            let Some((mut agent, version)) = self.agent_repository.get(&id).await? else {
                return Ok(None);
            };
            agent.deactivate()?;
            let snapshot = agent.snapshot();
            match self.agent_repository.save(agent, version).await {
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
    use wiab_core::vm::VmSnapshot;

    use super::*;

    #[derive(Default)]
    struct TestAgentRepository {
        agents: RwLock<HashMap<AgentId, (Agent, u64)>>,
    }

    impl AgentRepository for TestAgentRepository {
        async fn save(&self, agent: Agent, expected: Version) -> Result<Version, SaveError> {
            let mut agents = self
                .agents
                .write()
                .expect("test repository write lock poisoned");
            let current = agents
                .get(&agent.id())
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            agents.insert(agent.id(), (agent, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &AgentId) -> Result<Option<(Agent, Version)>, RepoError> {
            Ok(self
                .agents
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(agent, version)| (agent.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Agent>, RepoError> {
            Ok(self
                .agents
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(agent, _)| agent.clone())
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
    struct TestAgentNumbering {
        counter: AtomicU64,
    }

    impl AgentNumbering for TestAgentNumbering {
        fn next(&self) -> AgentId {
            AgentId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    /// Stub VM provisioning: pretends to boot at a fixed endpoint (stands in for the vm service).
    struct StubVmProvisioning;

    fn stub_vm(id: &str, state: &str, guest_ip: Option<&str>) -> VmSnapshot {
        VmSnapshot {
            id: id.to_owned(),
            organization_id: "O-1".to_owned(),
            owner_id: "A-1".to_owned(),
            template: "developer".to_owned(),
            state: state.to_owned(),
            guest_ip: guest_ip.map(str::to_owned),
            vcpus: 2,
            mem_mib: 1024,
        }
    }

    impl VmProvisioning for StubVmProvisioning {
        async fn provision(
            &self,
            _organization_id: &str,
            _agent_id: &str,
            _request: ProvisionVmRequest,
        ) -> anyhow::Result<Option<VmSnapshot>> {
            Ok(Some(stub_vm("VM-1", "running", Some("172.16.0.9"))))
        }

        async fn stop(&self, _vm_id: &str) -> anyhow::Result<Option<VmSnapshot>> {
            Ok(Some(stub_vm("VM-1", "stopped", None)))
        }

        async fn get(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>> {
            Ok(Some(stub_vm(vm_id, "running", Some("172.16.0.9"))))
        }
    }

    type Svc = AgentApplicationService<
        TestAgentRepository,
        TestOrganizationRepository,
        StubVmProvisioning,
    >;

    fn service() -> Svc {
        AgentApplicationService::new(
            TestAgentRepository::default(),
            TestOrganizationRepository::default(),
            StubVmProvisioning,
            Arc::new(TestAgentNumbering::default()),
        )
    }

    async fn seed_organization(service: &Svc, number: u64) -> String {
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

    async fn create(service: &Svc, organization_id: &str, name: &str) -> AgentSnapshot {
        service
            .create_agent(
                organization_id,
                CreateAgentRequest {
                    name: name.to_owned(),
                    description: String::new(),
                    vm_type: None,
                },
            )
            .await
            .expect("organization id should be valid")
            .expect("organization should exist")
    }

    #[tokio::test]
    async fn create_agent_assigns_incrementing_ids() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        assert_eq!(create(&service, &organization_id, "First").await.id, "A-1");
        assert_eq!(create(&service, &organization_id, "Second").await.id, "A-2");
    }

    #[tokio::test]
    async fn create_agent_records_organization_id() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let agent = create(&service, &organization_id, "Scout").await;
        assert_eq!(agent.organization_id, organization_id);
    }

    #[tokio::test]
    async fn create_agent_under_missing_organization_returns_none() {
        let service = service();
        let result = service
            .create_agent(
                "O-9",
                CreateAgentRequest {
                    name: "Scout".to_owned(),
                    description: String::new(),
                    vm_type: None,
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn create_agent_rejects_malformed_organization_id() {
        let service = service();
        assert!(
            service
                .create_agent(
                    "bogus",
                    CreateAgentRequest {
                        name: "Scout".to_owned(),
                        description: String::new(),
                        vm_type: None,
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn create_agent_rejects_empty_name() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        assert!(
            service
                .create_agent(
                    &organization_id,
                    CreateAgentRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                        vm_type: None,
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn list_agents_partitions_by_organization() {
        let service = service();
        let first_organization = seed_organization(&service, 1).await;
        let second_organization = seed_organization(&service, 2).await;
        create(&service, &first_organization, "First").await;
        create(&service, &second_organization, "Second").await;
        create(&service, &first_organization, "Third").await;
        service
            .agent_repository
            .save(
                Agent::new(
                    AgentId::from_number(10),
                    OrganizationId::from_number(1),
                    "Tenth".to_owned(),
                    String::new(),
                )
                .unwrap(),
                Version::NEW,
            )
            .await
            .unwrap();

        let first_ids = service
            .list_agents(&first_organization)
            .await
            .unwrap()
            .expect("organization should exist")
            .into_iter()
            .map(|agent| agent.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["A-1", "A-3", "A-10"]);

        let second_ids = service
            .list_agents(&second_organization)
            .await
            .unwrap()
            .expect("organization should exist")
            .into_iter()
            .map(|agent| agent.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["A-2"]);
    }

    #[tokio::test]
    async fn list_agents_for_missing_organization_returns_none() {
        let service = service();
        assert!(service.list_agents("O-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_agents_rejects_malformed_organization_id() {
        let service = service();
        assert!(service.list_agents("bogus").await.is_err());
    }

    #[tokio::test]
    async fn agent_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.agent_snapshot("A-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn agent_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.agent_snapshot("bogus").await.is_err());
    }

    #[tokio::test]
    async fn update_agent_replaces_fields_but_not_organization() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let agent = create(&service, &organization_id, "Scout").await;
        let updated = service
            .update_agent(
                &agent.id,
                UpdateAgentRequest {
                    name: "Builder".to_owned(),
                    description: "ships code".to_owned(),
                    vm_type: None,
                },
            )
            .await
            .unwrap()
            .expect("agent should exist");
        assert_eq!(updated.name, "Builder");
        assert_eq!(updated.description, "ships code");
        assert_eq!(updated.organization_id, organization_id);

        let reloaded = service
            .agent_snapshot(&agent.id)
            .await
            .unwrap()
            .expect("agent should exist");
        assert_eq!(reloaded.name, "Builder");
    }

    #[tokio::test]
    async fn update_missing_agent_returns_none() {
        let service = service();
        let result = service
            .update_agent(
                "A-9",
                UpdateAgentRequest {
                    name: "Builder".to_owned(),
                    description: String::new(),
                    vm_type: None,
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_agent_rejects_empty_name() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let agent = create(&service, &organization_id, "Scout").await;
        assert!(
            service
                .update_agent(
                    &agent.id,
                    UpdateAgentRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                        vm_type: None,
                    },
                )
                .await
                .is_err()
        );
    }

    async fn create_with_type(
        service: &Svc,
        organization_id: &str,
        vm_type: &str,
    ) -> AgentSnapshot {
        service
            .create_agent(
                organization_id,
                CreateAgentRequest {
                    name: "Dev".to_owned(),
                    description: String::new(),
                    vm_type: Some(vm_type.to_owned()),
                },
            )
            .await
            .unwrap()
            .expect("organization should exist")
    }

    #[tokio::test]
    async fn create_records_vm_type() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let created = create_with_type(&service, &organization_id, "developer").await;
        assert_eq!(created.vm_type.as_deref(), Some("developer"));
        assert!(!created.active);
        assert_eq!(created.vm_id, None);
    }

    #[tokio::test]
    async fn activate_then_deactivate() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let created = create_with_type(&service, &organization_id, "developer").await;

        let activated = service
            .activate_agent(&created.id)
            .await
            .unwrap()
            .expect("agent should exist");
        assert!(activated.active);
        assert_eq!(activated.vm_id.as_deref(), Some("VM-1"));
        assert_eq!(activated.guest_ip.as_deref(), Some("172.16.0.9"));

        let deactivated = service
            .deactivate_agent(&created.id)
            .await
            .unwrap()
            .expect("agent should exist");
        assert!(!deactivated.active);
        assert_eq!(deactivated.vm_id, None);
    }

    #[tokio::test]
    async fn activate_without_vm_type_errors() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let agent = create(&service, &organization_id, "Scout").await;
        assert!(service.activate_agent(&agent.id).await.is_err());
    }

    #[tokio::test]
    async fn activate_missing_agent_returns_none() {
        let service = service();
        assert!(service.activate_agent("A-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn deactivate_inactive_agent_errors() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let agent = create(&service, &organization_id, "Scout").await;
        assert!(service.deactivate_agent(&agent.id).await.is_err());
    }

    #[tokio::test]
    async fn active_agent_snapshot_includes_guest_ip() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let created = create_with_type(&service, &organization_id, "developer").await;
        service.activate_agent(&created.id).await.unwrap();
        let fetched = service
            .agent_snapshot(&created.id)
            .await
            .unwrap()
            .expect("agent should exist");
        assert!(fetched.active);
        assert_eq!(fetched.guest_ip.as_deref(), Some("172.16.0.9"));
    }
}
