use crate::agent::{Agent, AgentId};

/// Port for persisting agent aggregates. One repository per aggregate root.
pub trait AgentRepository: Send + Sync + 'static {
    fn save(&self, agent: Agent);
    fn get(&self, id: &AgentId) -> Option<Agent>;
    fn list(&self) -> Vec<Agent>;
}
