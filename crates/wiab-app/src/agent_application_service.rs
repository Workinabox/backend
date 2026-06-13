use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::agent::{Agent, AgentId, AgentNumbering, AgentRepository, AgentSnapshot};
use wiab_core::organization::{OrganizationId, OrganizationRepository};
use wiab_core::repository::{SaveError, Version};

use crate::agent_requests::{CreateAgentRequest, UpdateAgentRequest};

/// Orchestrates use cases over the `Agent` aggregate.
///
/// Methods are async and fallible: persistence may be remote. Lost updates are prevented by
/// optimistic concurrency — a mutation loads the aggregate with its version, applies the
/// change, and retries when a concurrent save advanced the version in between. Holds the
/// organization repository to verify the parent organization exists.
pub struct AgentApplicationService<A: AgentRepository, O: OrganizationRepository> {
    agent_repository: A,
    organization_repository: O,
    numbering: Arc<dyn AgentNumbering>,
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
        }
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
        Ok(Some(
            agents.into_iter().map(|agent| agent.snapshot()).collect(),
        ))
    }

    pub async fn agent_snapshot(&self, agent_id: &str) -> anyhow::Result<Option<AgentSnapshot>> {
        let id: AgentId = agent_id.parse()?;
        Ok(self
            .agent_repository
            .get(&id)
            .await?
            .map(|(agent, _)| agent.snapshot()))
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
        let agent = Agent::new(self.numbering.next(), id, request.name, request.description)?;
        let snapshot = agent.snapshot();
        self.agent_repository.save(agent, Version::NEW).await?;
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no agent with the given id exists.
    pub async fn update_agent(
        &self,
        agent_id: &str,
        request: UpdateAgentRequest,
    ) -> anyhow::Result<Option<AgentSnapshot>> {
        let id: AgentId = agent_id.parse()?;
        loop {
            let Some((mut agent, version)) = self.agent_repository.get(&id).await? else {
                return Ok(None);
            };
            agent.update(request.name.clone(), request.description.clone())?;
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

    fn service() -> AgentApplicationService<TestAgentRepository, TestOrganizationRepository> {
        AgentApplicationService::new(
            TestAgentRepository::default(),
            TestOrganizationRepository::default(),
            Arc::new(TestAgentNumbering::default()),
        )
    }

    async fn seed_organization(
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
        service
            .organization_repository
            .save(organization, Version::NEW)
            .await
            .unwrap();
        id
    }

    async fn create(
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
                    },
                )
                .await
                .is_err()
        );
    }
}
