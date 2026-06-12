use crate::organization::OrganizationId;
use crate::project::{ProjectError, ProjectId, ProjectSnapshot};

/// A project: a `P-###` id, the organization it belongs to, a name, and a description.
/// Works, boards, repos, and pipelines belong to a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    id: ProjectId,
    organization_id: OrganizationId,
    name: String,
    description: String,
}

impl Project {
    pub fn new(
        id: ProjectId,
        organization_id: OrganizationId,
        name: String,
        description: String,
    ) -> Result<Self, ProjectError> {
        if name.trim().is_empty() {
            return Err(ProjectError::EmptyName);
        }
        Ok(Self {
            id,
            organization_id,
            name,
            description,
        })
    }

    pub fn id(&self) -> ProjectId {
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

    pub fn update(&mut self, name: String, description: String) -> Result<(), ProjectError> {
        if name.trim().is_empty() {
            return Err(ProjectError::EmptyName);
        }
        self.name = name;
        self.description = description;
        Ok(())
    }

    pub fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot {
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

    fn project(number: u64, name: &str) -> Project {
        Project::new(
            ProjectId::from_number(number),
            OrganizationId::from_number(1),
            name.to_owned(),
            String::new(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_empty_name() {
        let error = Project::new(
            ProjectId::from_number(1),
            OrganizationId::from_number(1),
            "  ".to_owned(),
            String::new(),
        )
        .unwrap_err();
        assert_eq!(error, ProjectError::EmptyName);
    }

    #[test]
    fn exposes_getters() {
        let project = Project::new(
            ProjectId::from_number(1),
            OrganizationId::from_number(2),
            "Workinabox".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        assert_eq!(project.id(), ProjectId::from_number(1));
        assert_eq!(project.organization_id(), OrganizationId::from_number(2));
        assert_eq!(project.name(), "Workinabox");
        assert_eq!(project.description(), "desc");
    }

    #[test]
    fn update_replaces_name_and_description_but_not_organization() {
        let mut project = project(1, "Workinabox");
        project
            .update("Rocket".to_owned(), "to the moon".to_owned())
            .unwrap();
        assert_eq!(project.name(), "Rocket");
        assert_eq!(project.description(), "to the moon");
        assert_eq!(project.organization_id(), OrganizationId::from_number(1));
    }

    #[test]
    fn update_rejects_empty_name() {
        let mut project = project(1, "Workinabox");
        let error = project
            .update("  ".to_owned(), "to the moon".to_owned())
            .unwrap_err();
        assert_eq!(error, ProjectError::EmptyName);
        assert_eq!(project.name(), "Workinabox");
        assert_eq!(project.description(), "");
    }

    #[test]
    fn snapshot_mirrors_fields() {
        let project = Project::new(
            ProjectId::from_number(1),
            OrganizationId::from_number(2),
            "Workinabox".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        let snapshot = project.snapshot();
        assert_eq!(snapshot.id, "P-1");
        assert_eq!(snapshot.organization_id, "O-2");
        assert_eq!(snapshot.name, "Workinabox");
        assert_eq!(snapshot.description, "desc");
    }
}
