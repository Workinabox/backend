use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::organization::{Organization, OrganizationId, OrganizationRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryOrganizationRepository {
    organizations: Arc<RwLock<HashMap<OrganizationId, Organization>>>,
}

impl InMemoryOrganizationRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl OrganizationRepository for InMemoryOrganizationRepository {
    fn save(&self, organization: Organization) {
        self.organizations
            .write()
            .expect("organization repository write lock poisoned")
            .insert(organization.id(), organization);
    }

    fn get(&self, id: &OrganizationId) -> Option<Organization> {
        self.organizations
            .read()
            .expect("organization repository read lock poisoned")
            .get(id)
            .cloned()
    }

    fn list(&self) -> Vec<Organization> {
        self.organizations
            .read()
            .expect("organization repository read lock poisoned")
            .values()
            .cloned()
            .collect()
    }
}
