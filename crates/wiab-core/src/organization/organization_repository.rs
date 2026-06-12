use crate::organization::{Organization, OrganizationId};

/// Port for persisting organization aggregates. One repository per aggregate root.
pub trait OrganizationRepository: Send + Sync + 'static {
    fn save(&self, organization: Organization);
    fn get(&self, id: &OrganizationId) -> Option<Organization>;
    fn list(&self) -> Vec<Organization>;
}
