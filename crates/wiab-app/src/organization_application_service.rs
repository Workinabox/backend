use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::organization::{
    Organization, OrganizationId, OrganizationNumbering, OrganizationRepository,
    OrganizationSnapshot,
};
use wiab_core::repository::{SaveError, Version};

use crate::organization_requests::{CreateOrganizationRequest, UpdateOrganizationRequest};

/// Orchestrates use cases over the `Organization` aggregate.
///
/// Methods are async and fallible: persistence may be remote. Lost updates are prevented by
/// optimistic concurrency — a mutation loads the aggregate with its version, applies the
/// change, and retries when a concurrent save advanced the version in between.
pub struct OrganizationApplicationService<R: OrganizationRepository> {
    organization_repository: R,
    numbering: Arc<dyn OrganizationNumbering>,
}

impl<R: OrganizationRepository> OrganizationApplicationService<R> {
    pub fn new(organization_repository: R, numbering: Arc<dyn OrganizationNumbering>) -> Self {
        Self {
            organization_repository,
            numbering,
        }
    }

    pub async fn list_organizations(&self) -> anyhow::Result<Vec<OrganizationSnapshot>> {
        let mut organizations = self.organization_repository.list().await?;
        organizations.sort_by_key(|organization| organization.id().number());
        Ok(organizations
            .into_iter()
            .map(|organization| organization.snapshot())
            .collect())
    }

    pub async fn organization_snapshot(
        &self,
        organization_id: &str,
    ) -> anyhow::Result<Option<OrganizationSnapshot>> {
        let id: OrganizationId = organization_id.parse()?;
        Ok(self
            .organization_repository
            .get(&id)
            .await?
            .map(|(organization, _)| organization.snapshot()))
    }

    pub async fn create_organization(
        &self,
        request: CreateOrganizationRequest,
    ) -> anyhow::Result<OrganizationSnapshot> {
        let organization =
            Organization::new(self.numbering.next(), request.name, request.description)?;
        let snapshot = organization.snapshot();
        self.organization_repository
            .save(organization, Version::NEW)
            .await?;
        Ok(snapshot)
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub async fn update_organization(
        &self,
        organization_id: &str,
        request: UpdateOrganizationRequest,
    ) -> anyhow::Result<Option<OrganizationSnapshot>> {
        let id: OrganizationId = organization_id.parse()?;
        loop {
            let Some((mut organization, version)) = self.organization_repository.get(&id).await?
            else {
                return Ok(None);
            };
            organization.update(request.name.clone(), request.description.clone())?;
            let snapshot = organization.snapshot();
            match self
                .organization_repository
                .save(organization, version)
                .await
            {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::repository::{RepoError, SaveError, Version};

    use super::*;

    #[derive(Default)]
    struct TestOrganizationRepository {
        organizations: RwLock<HashMap<OrganizationId, (Organization, u64)>>,
    }

    impl OrganizationRepository for TestOrganizationRepository {
        async fn save(
            &self,
            organization: Organization,
            expected: Version,
        ) -> Result<Version, SaveError> {
            let mut organizations = self
                .organizations
                .write()
                .expect("test repository write lock poisoned");
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

        async fn get(
            &self,
            id: &OrganizationId,
        ) -> Result<Option<(Organization, Version)>, RepoError> {
            Ok(self
                .organizations
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(organization, version)| {
                    (organization.clone(), Version::from_value(*version))
                }))
        }

        async fn list(&self) -> Result<Vec<Organization>, RepoError> {
            Ok(self
                .organizations
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(organization, _)| organization.clone())
                .collect())
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

    async fn create(
        service: &OrganizationApplicationService<TestOrganizationRepository>,
        name: &str,
    ) -> OrganizationSnapshot {
        service
            .create_organization(CreateOrganizationRequest {
                name: name.to_owned(),
                description: String::new(),
            })
            .await
            .expect("organization should be created")
    }

    #[tokio::test]
    async fn create_organization_assigns_incrementing_ids() {
        let service = service();
        assert_eq!(create(&service, "First").await.id, "O-1");
        assert_eq!(create(&service, "Second").await.id, "O-2");
    }

    #[tokio::test]
    async fn create_organization_rejects_empty_name() {
        let service = service();
        assert!(
            service
                .create_organization(CreateOrganizationRequest {
                    name: "  ".to_owned(),
                    description: String::new(),
                })
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn list_organizations_sorts_by_id() {
        let service = service();
        create(&service, "First").await;
        create(&service, "Second").await;
        service
            .organization_repository
            .save(
                Organization::new(
                    OrganizationId::from_number(10),
                    "Tenth".to_owned(),
                    String::new(),
                )
                .unwrap(),
                Version::NEW,
            )
            .await
            .unwrap();
        let ids = service
            .list_organizations()
            .await
            .unwrap()
            .into_iter()
            .map(|organization| organization.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["O-1", "O-2", "O-10"]);
    }

    #[tokio::test]
    async fn organization_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(
            service
                .organization_snapshot("O-9")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn organization_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.organization_snapshot("bogus").await.is_err());
    }

    #[tokio::test]
    async fn update_organization_replaces_fields() {
        let service = service();
        let organization = create(&service, "Gos & co").await;
        let updated = service
            .update_organization(
                &organization.id,
                UpdateOrganizationRequest {
                    name: "Acme".to_owned(),
                    description: "rockets".to_owned(),
                },
            )
            .await
            .unwrap()
            .expect("organization should exist");
        assert_eq!(updated.name, "Acme");
        assert_eq!(updated.description, "rockets");

        let reloaded = service
            .organization_snapshot(&organization.id)
            .await
            .unwrap()
            .expect("organization should exist");
        assert_eq!(reloaded.name, "Acme");
    }

    #[tokio::test]
    async fn update_missing_organization_returns_none() {
        let service = service();
        let result = service
            .update_organization(
                "O-9",
                UpdateOrganizationRequest {
                    name: "Acme".to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_organization_rejects_empty_name() {
        let service = service();
        let organization = create(&service, "Gos & co").await;
        assert!(
            service
                .update_organization(
                    &organization.id,
                    UpdateOrganizationRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }
}
