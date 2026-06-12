use std::sync::{Arc, Mutex};

use wiab_core::organization::{
    Organization, OrganizationId, OrganizationNumbering, OrganizationRepository,
    OrganizationSnapshot,
};

use crate::organization_requests::{CreateOrganizationRequest, UpdateOrganizationRequest};

/// Orchestrates use cases over the `Organization` aggregate.
///
/// Methods are synchronous: `Organization` has no external/async collaborators, so a plain
/// `std::sync::Mutex` guard held across each load-mutate-save is enough to prevent lost
/// updates.
pub struct OrganizationApplicationService<R: OrganizationRepository> {
    organization_repository: R,
    numbering: Arc<dyn OrganizationNumbering>,
    mutation_guard: Mutex<()>,
}

impl<R: OrganizationRepository> OrganizationApplicationService<R> {
    pub fn new(organization_repository: R, numbering: Arc<dyn OrganizationNumbering>) -> Self {
        Self {
            organization_repository,
            numbering,
            mutation_guard: Mutex::new(()),
        }
    }

    pub fn list_organizations(&self) -> Vec<OrganizationSnapshot> {
        let mut organizations = self.organization_repository.list();
        organizations.sort_by_key(|organization| organization.id().number());
        organizations
            .into_iter()
            .map(|organization| organization.snapshot())
            .collect()
    }

    pub fn organization_snapshot(
        &self,
        organization_id: &str,
    ) -> anyhow::Result<Option<OrganizationSnapshot>> {
        let id: OrganizationId = organization_id.parse()?;
        Ok(self
            .organization_repository
            .get(&id)
            .map(|organization| organization.snapshot()))
    }

    pub fn create_organization(
        &self,
        request: CreateOrganizationRequest,
    ) -> anyhow::Result<OrganizationSnapshot> {
        let _guard = self.lock();
        let organization =
            Organization::new(self.numbering.next(), request.name, request.description)?;
        let snapshot = organization.snapshot();
        self.organization_repository.save(organization);
        Ok(snapshot)
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub fn update_organization(
        &self,
        organization_id: &str,
        request: UpdateOrganizationRequest,
    ) -> anyhow::Result<Option<OrganizationSnapshot>> {
        let _guard = self.lock();
        let id: OrganizationId = organization_id.parse()?;
        let Some(mut organization) = self.organization_repository.get(&id) else {
            return Ok(None);
        };
        organization.update(request.name, request.description)?;
        let snapshot = organization.snapshot();
        self.organization_repository.save(organization);
        Ok(Some(snapshot))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_guard
            .lock()
            .expect("organization mutation guard poisoned")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[derive(Default)]
    struct TestOrganizationRepository {
        organizations: RwLock<HashMap<OrganizationId, Organization>>,
    }

    impl OrganizationRepository for TestOrganizationRepository {
        fn save(&self, organization: Organization) {
            self.organizations
                .write()
                .expect("test repository write lock poisoned")
                .insert(organization.id(), organization);
        }

        fn get(&self, id: &OrganizationId) -> Option<Organization> {
            self.organizations
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .cloned()
        }

        fn list(&self) -> Vec<Organization> {
            self.organizations
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .cloned()
                .collect()
        }
    }

    #[derive(Default)]
    struct TestOrganizationNumbering {
        counter: AtomicU64,
    }

    impl OrganizationNumbering for TestOrganizationNumbering {
        fn next(&self) -> OrganizationId {
            OrganizationId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    fn service() -> OrganizationApplicationService<TestOrganizationRepository> {
        OrganizationApplicationService::new(
            TestOrganizationRepository::default(),
            Arc::new(TestOrganizationNumbering::default()),
        )
    }

    fn create(
        service: &OrganizationApplicationService<TestOrganizationRepository>,
        name: &str,
    ) -> OrganizationSnapshot {
        service
            .create_organization(CreateOrganizationRequest {
                name: name.to_owned(),
                description: String::new(),
            })
            .expect("organization should be created")
    }

    #[test]
    fn create_organization_assigns_incrementing_ids() {
        let service = service();
        assert_eq!(create(&service, "First").id, "O-1");
        assert_eq!(create(&service, "Second").id, "O-2");
    }

    #[test]
    fn create_organization_rejects_empty_name() {
        let service = service();
        assert!(
            service
                .create_organization(CreateOrganizationRequest {
                    name: "  ".to_owned(),
                    description: String::new(),
                })
                .is_err()
        );
    }

    #[test]
    fn list_organizations_sorts_by_id() {
        let service = service();
        create(&service, "First");
        create(&service, "Second");
        service.organization_repository.save(
            Organization::new(
                OrganizationId::from_number(10),
                "Tenth".to_owned(),
                String::new(),
            )
            .unwrap(),
        );
        let ids = service
            .list_organizations()
            .into_iter()
            .map(|organization| organization.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["O-1", "O-2", "O-10"]);
    }

    #[test]
    fn organization_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.organization_snapshot("O-9").unwrap().is_none());
    }

    #[test]
    fn organization_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.organization_snapshot("bogus").is_err());
    }

    #[test]
    fn update_organization_replaces_fields() {
        let service = service();
        let organization = create(&service, "Gos & co");
        let updated = service
            .update_organization(
                &organization.id,
                UpdateOrganizationRequest {
                    name: "Acme".to_owned(),
                    description: "rockets".to_owned(),
                },
            )
            .unwrap()
            .expect("organization should exist");
        assert_eq!(updated.name, "Acme");
        assert_eq!(updated.description, "rockets");

        let reloaded = service
            .organization_snapshot(&organization.id)
            .unwrap()
            .expect("organization should exist");
        assert_eq!(reloaded.name, "Acme");
    }

    #[test]
    fn update_missing_organization_returns_none() {
        let service = service();
        let result = service
            .update_organization(
                "O-9",
                UpdateOrganizationRequest {
                    name: "Acme".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_organization_rejects_empty_name() {
        let service = service();
        let organization = create(&service, "Gos & co");
        assert!(
            service
                .update_organization(
                    &organization.id,
                    UpdateOrganizationRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }
}
