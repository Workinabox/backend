use std::sync::{Arc, Mutex};

use wiab_core::agent::{Agent, AgentId, AgentNumbering, AgentRepository, AgentSnapshot};
use wiab_core::organization::{OrganizationId, OrganizationRepository};

use crate::agent_requests::{CreateAgentRequest, UpdateAgentRequest};

/// Orchestrates use cases over the `Agent` aggregate.
///
/// Methods are synchronous: `Agent` has no external/async collaborators, so a plain
/// `std::sync::Mutex` guard held across each load-mutate-save is enough to prevent lost
/// updates. Holds the organization repository to verify the parent organization exists.
pub struct AgentApplicationService<A: AgentRepository, O: OrganizationRepository> {
    agent_repository: A,
    organization_repository: O,
    numbering: Arc<dyn AgentNumbering>,
    mutation_guard: Mutex<()>,
}

impl<A: AgentRepository, O: OrganizationRepository> AgentApplicationService<A, O> {
    pub fn new(
        agent_repository: A,
        organization_repository: O,
        numbering: Arc<dyn AgentNumbering>,
    ) -> Self {
        Self {
            agent_repository,
            organization_repository,
            numbering,
            mutation_guard: Mutex::new(()),
        }
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub fn list_agents(&self, organization_id: &str) -> anyhow::Result<Option<Vec<AgentSnapshot>>> {
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).is_none() {
            return Ok(None);
        }
        let mut agents = self
            .agent_repository
            .list()
            .into_iter()
            .filter(|agent| agent.organization_id() == id)
            .collect::<Vec<_>>();
        agents.sort_by_key(|agent| agent.id().number());
        Ok(Some(
            agents.into_iter().map(|agent| agent.snapshot()).collect(),
        ))
    }

    pub fn agent_snapshot(&self, agent_id: &str) -> anyhow::Result<Option<AgentSnapshot>> {
        let id: AgentId = agent_id.parse()?;
        Ok(self.agent_repository.get(&id).map(|agent| agent.snapshot()))
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub fn create_agent(
        &self,
        organization_id: &str,
        request: CreateAgentRequest,
    ) -> anyhow::Result<Option<AgentSnapshot>> {
        let _guard = self.lock();
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).is_none() {
            return Ok(None);
        }
        let agent = Agent::new(self.numbering.next(), id, request.name, request.description)?;
        let snapshot = agent.snapshot();
        self.agent_repository.save(agent);
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no agent with the given id exists.
    pub fn update_agent(
        &self,
        agent_id: &str,
        request: UpdateAgentRequest,
    ) -> anyhow::Result<Option<AgentSnapshot>> {
        let _guard = self.lock();
        let id: AgentId = agent_id.parse()?;
        let Some(mut agent) = self.agent_repository.get(&id) else {
            return Ok(None);
        };
        agent.update(request.name, request.description)?;
        let snapshot = agent.snapshot();
        self.agent_repository.save(agent);
        Ok(Some(snapshot))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_guard
            .lock()
            .expect("agent mutation guard poisoned")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::organization::Organization;

    use super::*;

    #[derive(Default)]
    struct TestAgentRepository {
        agents: RwLock<HashMap<AgentId, Agent>>,
    }

    impl AgentRepository for TestAgentRepository {
        fn save(&self, agent: Agent) {
            self.agents
                .write()
                .expect("test repository write lock poisoned")
                .insert(agent.id(), agent);
        }

        fn get(&self, id: &AgentId) -> Option<Agent> {
            self.agents
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .cloned()
        }

        fn list(&self) -> Vec<Agent> {
            self.agents
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .cloned()
                .collect()
        }
    }

    #[derive(Default)]
    struct TestOrganizationRepository {
        organizations: RwLock<HashMap<OrganizationId, Organization>>,
    }

    impl OrganizationRepository for TestOrganizationRepository {
        fn save(&self, organization: Organization) {
            self.organizations
                .write()
                .expect("test repository write lock poisoned")
                .insert(organization.id(), organization);
        }

        fn get(&self, id: &OrganizationId) -> Option<Organization> {
            self.organizations
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .cloned()
        }

        fn list(&self) -> Vec<Organization> {
            self.organizations
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .cloned()
                .collect()
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

    fn service() -> AgentApplicationService<TestAgentRepository, TestOrganizationRepository> {
        AgentApplicationService::new(
            TestAgentRepository::default(),
            TestOrganizationRepository::default(),
            Arc::new(TestAgentNumbering::default()),
        )
    }

    fn seed_organization(
        service: &AgentApplicationService<TestAgentRepository, TestOrganizationRepository>,
        number: u64,
    ) -> String {
        let organization = Organization::new(
            OrganizationId::from_number(number),
            format!("Org {number}"),
            String::new(),
        )
        .unwrap();
        let id = organization.id().to_string();
        service.organization_repository.save(organization);
        id
    }

    fn create(
        service: &AgentApplicationService<TestAgentRepository, TestOrganizationRepository>,
        organization_id: &str,
        name: &str,
    ) -> AgentSnapshot {
        service
            .create_agent(
                organization_id,
                CreateAgentRequest {
                    name: name.to_owned(),
                    description: String::new(),
                },
            )
            .expect("organization id should be valid")
            .expect("organization should exist")
    }

    #[test]
    fn create_agent_assigns_incrementing_ids() {
        let service = service();
        let organization_id = seed_organization(&service, 1);
        assert_eq!(create(&service, &organization_id, "First").id, "A-1");
        assert_eq!(create(&service, &organization_id, "Second").id, "A-2");
    }

    #[test]
    fn create_agent_records_organization_id() {
        let service = service();
        let organization_id = seed_organization(&service, 1);
        let agent = create(&service, &organization_id, "Scout");
        assert_eq!(agent.organization_id, organization_id);
    }

    #[test]
    fn create_agent_under_missing_organization_returns_none() {
        let service = service();
        let result = service
            .create_agent(
                "O-9",
                CreateAgentRequest {
                    name: "Scout".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn create_agent_rejects_malformed_organization_id() {
        let service = service();
        assert!(
            service
                .create_agent(
                    "bogus",
                    CreateAgentRequest {
                        name: "Scout".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn create_agent_rejects_empty_name() {
        let service = service();
        let organization_id = seed_organization(&service, 1);
        assert!(
            service
                .create_agent(
                    &organization_id,
                    CreateAgentRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn list_agents_partitions_by_organization() {
        let service = service();
        let first_organization = seed_organization(&service, 1);
        let second_organization = seed_organization(&service, 2);
        create(&service, &first_organization, "First");
        create(&service, &second_organization, "Second");
        create(&service, &first_organization, "Third");
        service.agent_repository.save(
            Agent::new(
                AgentId::from_number(10),
                OrganizationId::from_number(1),
                "Tenth".to_owned(),
                String::new(),
            )
            .unwrap(),
        );

        let first_ids = service
            .list_agents(&first_organization)
            .unwrap()
            .expect("organization should exist")
            .into_iter()
            .map(|agent| agent.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["A-1", "A-3", "A-10"]);

        let second_ids = service
            .list_agents(&second_organization)
            .unwrap()
            .expect("organization should exist")
            .into_iter()
            .map(|agent| agent.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["A-2"]);
    }

    #[test]
    fn list_agents_for_missing_organization_returns_none() {
        let service = service();
        assert!(service.list_agents("O-9").unwrap().is_none());
    }

    #[test]
    fn list_agents_rejects_malformed_organization_id() {
        let service = service();
        assert!(service.list_agents("bogus").is_err());
    }

    #[test]
    fn agent_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.agent_snapshot("A-9").unwrap().is_none());
    }

    #[test]
    fn agent_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.agent_snapshot("bogus").is_err());
    }

    #[test]
    fn update_agent_replaces_fields_but_not_organization() {
        let service = service();
        let organization_id = seed_organization(&service, 1);
        let agent = create(&service, &organization_id, "Scout");
        let updated = service
            .update_agent(
                &agent.id,
                UpdateAgentRequest {
                    name: "Builder".to_owned(),
                    description: "ships code".to_owned(),
                },
            )
            .unwrap()
            .expect("agent should exist");
        assert_eq!(updated.name, "Builder");
        assert_eq!(updated.description, "ships code");
        assert_eq!(updated.organization_id, organization_id);

        let reloaded = service
            .agent_snapshot(&agent.id)
            .unwrap()
            .expect("agent should exist");
        assert_eq!(reloaded.name, "Builder");
    }

    #[test]
    fn update_missing_agent_returns_none() {
        let service = service();
        let result = service
            .update_agent(
                "A-9",
                UpdateAgentRequest {
                    name: "Builder".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_agent_rejects_empty_name() {
        let service = service();
        let organization_id = seed_organization(&service, 1);
        let agent = create(&service, &organization_id, "Scout");
        assert!(
            service
                .update_agent(
                    &agent.id,
                    UpdateAgentRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }
}
