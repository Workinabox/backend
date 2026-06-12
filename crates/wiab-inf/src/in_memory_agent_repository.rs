use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::agent::{Agent, AgentId, AgentRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryAgentRepository {
    agents: Arc<RwLock<HashMap<AgentId, Agent>>>,
}

impl InMemoryAgentRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AgentRepository for InMemoryAgentRepository {
    fn save(&self, agent: Agent) {
        self.agents
            .write()
            .expect("agent repository write lock poisoned")
            .insert(agent.id(), agent);
    }

    fn get(&self, id: &AgentId) -> Option<Agent> {
        self.agents
            .read()
            .expect("agent repository read lock poisoned")
            .get(id)
            .cloned()
    }

    fn list(&self) -> Vec<Agent> {
        self.agents
            .read()
            .expect("agent repository read lock poisoned")
            .values()
            .cloned()
            .collect()
    }
}
