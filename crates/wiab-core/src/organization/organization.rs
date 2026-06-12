use crate::organization::{OrganizationError, OrganizationId, OrganizationSnapshot};

/// An organization: an `O-###` id, a name, and a description. The root of the company
/// hierarchy — projects and agents belong to an organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organization {
    id: OrganizationId,
    name: String,
    description: String,
}

impl Organization {
    pub fn new(
        id: OrganizationId,
        name: String,
        description: String,
    ) -> Result<Self, OrganizationError> {
        if name.trim().is_empty() {
            return Err(OrganizationError::EmptyName);
        }
        Ok(Self {
            id,
            name,
            description,
        })
    }

    pub fn id(&self) -> OrganizationId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn update(&mut self, name: String, description: String) -> Result<(), OrganizationError> {
        if name.trim().is_empty() {
            return Err(OrganizationError::EmptyName);
        }
        self.name = name;
        self.description = description;
        Ok(())
    }

    pub fn snapshot(&self) -> OrganizationSnapshot {
        OrganizationSnapshot {
            id: self.id.to_string(),
            name: self.name.clone(),
            description: self.description.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn organization(number: u64, name: &str) -> Organization {
        Organization::new(
            OrganizationId::from_number(number),
            name.to_owned(),
            String::new(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_empty_name() {
        let error = Organization::new(
            OrganizationId::from_number(1),
            "  ".to_owned(),
            String::new(),
        )
        .unwrap_err();
        assert_eq!(error, OrganizationError::EmptyName);
    }

    #[test]
    fn exposes_getters() {
        let organization = Organization::new(
            OrganizationId::from_number(1),
            "Gos & co".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        assert_eq!(organization.id(), OrganizationId::from_number(1));
        assert_eq!(organization.name(), "Gos & co");
        assert_eq!(organization.description(), "desc");
    }

    #[test]
    fn update_replaces_name_and_description() {
        let mut organization = organization(1, "Gos & co");
        organization
            .update("Acme".to_owned(), "rockets".to_owned())
            .unwrap();
        assert_eq!(organization.name(), "Acme");
        assert_eq!(organization.description(), "rockets");
    }

    #[test]
    fn update_rejects_empty_name() {
        let mut organization = organization(1, "Gos & co");
        let error = organization
            .update("  ".to_owned(), "rockets".to_owned())
            .unwrap_err();
        assert_eq!(error, OrganizationError::EmptyName);
        assert_eq!(organization.name(), "Gos & co");
        assert_eq!(organization.description(), "");
    }

    #[test]
    fn snapshot_mirrors_fields() {
        let organization = Organization::new(
            OrganizationId::from_number(1),
            "Gos & co".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        let snapshot = organization.snapshot();
        assert_eq!(snapshot.id, "O-1");
        assert_eq!(snapshot.name, "Gos & co");
        assert_eq!(snapshot.description, "desc");
    }
}
