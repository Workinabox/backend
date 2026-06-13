use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::agent::{Agent, AgentId, AgentRepository};
use wiab_core::repository::{RepoError, SaveError, Version};

#[derive(Debug, Clone, Default)]
pub struct InMemoryAgentRepository {
    agents: Arc<RwLock<HashMap<AgentId, (Agent, u64)>>>,
}

impl InMemoryAgentRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AgentRepository for InMemoryAgentRepository {
    async fn save(&self, agent: Agent, expected: Version) -> Result<Version, SaveError> {
        let mut agents = self
            .agents
            .write()
            .expect("agent repository write lock poisoned");
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
            .expect("agent repository read lock poisoned")
            .get(id)
            .map(|(agent, version)| (agent.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Agent>, RepoError> {
        Ok(self
            .agents
            .read()
            .expect("agent repository read lock poisoned")
            .values()
            .map(|(agent, _)| agent.clone())
            .collect())
    }
}
