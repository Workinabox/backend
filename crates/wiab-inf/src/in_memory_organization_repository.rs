use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::organization::{Organization, OrganizationId, OrganizationRepository};
use wiab_core::repository::{RepoError, SaveError, Version};

#[derive(Debug, Clone, Default)]
pub struct InMemoryOrganizationRepository {
    organizations: Arc<RwLock<HashMap<OrganizationId, (Organization, u64)>>>,
}

impl InMemoryOrganizationRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl OrganizationRepository for InMemoryOrganizationRepository {
    async fn save(
        &self,
        organization: Organization,
        expected: Version,
    ) -> Result<Version, SaveError> {
        let mut organizations = self
            .organizations
            .write()
            .expect("organization repository write lock poisoned");
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

    async fn get(&self, id: &OrganizationId) -> Result<Option<(Organization, Version)>, RepoError> {
        Ok(self
            .organizations
            .read()
            .expect("organization repository read lock poisoned")
            .get(id)
            .map(|(organization, version)| (organization.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Organization>, RepoError> {
        Ok(self
            .organizations
            .read()
            .expect("organization repository read lock poisoned")
            .values()
            .map(|(organization, _)| organization.clone())
            .collect())
    }
}
