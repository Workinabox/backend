use crate::agent::{AgentError, AgentId, AgentSnapshot};
use crate::organization::OrganizationId;

/// An agent: an `A-###` id, the organization it belongs to, a name, and a description.
/// Agents belong to an organization, not to a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    id: AgentId,
    organization_id: OrganizationId,
    name: String,
    description: String,
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
        })
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

    pub fn update(&mut self, name: String, description: String) -> Result<(), AgentError> {
        if name.trim().is_empty() {
            return Err(AgentError::EmptyName);
        }
        self.name = name;
        self.description = description;
        Ok(())
    }

    pub fn snapshot(&self) -> AgentSnapshot {
        AgentSnapshot {
            id: self.id.to_string(),
            organization_id: self.organization_id.to_string(),
            name: self.name.clone(),
            description: self.description.clone(),
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
    fn snapshot_mirrors_fields() {
        let agent = Agent::new(
            AgentId::from_number(1),
            OrganizationId::from_number(2),
            "Scout".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        let snapshot = agent.snapshot();
        assert_eq!(snapshot.id, "A-1");
        assert_eq!(snapshot.organization_id, "O-2");
        assert_eq!(snapshot.name, "Scout");
        assert_eq!(snapshot.description, "desc");
    }
}
