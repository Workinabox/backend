use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::organization::{OrganizationId, OrganizationRepository};
use wiab_core::project::{
    Project, ProjectId, ProjectNumbering, ProjectRepository, ProjectSnapshot,
};
use wiab_core::repository::{SaveError, Version};

use crate::project_requests::{CreateProjectRequest, UpdateProjectRequest};

/// Orchestrates use cases over the `Project` aggregate.
///
/// Methods are async and fallible: persistence may be remote. Lost updates are prevented by
/// optimistic concurrency — a mutation loads the aggregate with its version, applies the
/// change, and retries when a concurrent save advanced the version in between. Holds the
/// organization repository to verify the parent organization exists.
pub struct ProjectApplicationService<P: ProjectRepository, O: OrganizationRepository> {
    project_repository: P,
    organization_repository: O,
    numbering: Arc<dyn ProjectNumbering>,
}

impl<P: ProjectRepository, O: OrganizationRepository> ProjectApplicationService<P, O> {
    pub fn new(
        project_repository: P,
        organization_repository: O,
        numbering: Arc<dyn ProjectNumbering>,
    ) -> Self {
        Self {
            project_repository,
            organization_repository,
            numbering,
        }
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub async fn list_projects(
        &self,
        organization_id: &str,
    ) -> anyhow::Result<Option<Vec<ProjectSnapshot>>> {
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let mut projects = self
            .project_repository
            .list()
            .await?
            .into_iter()
            .filter(|project| project.organization_id() == id)
            .collect::<Vec<_>>();
        projects.sort_by_key(|project| project.id().number());
        Ok(Some(
            projects
                .into_iter()
                .map(|project| project.snapshot())
                .collect(),
        ))
    }

    pub async fn project_snapshot(
        &self,
        project_id: &str,
    ) -> anyhow::Result<Option<ProjectSnapshot>> {
        let id: ProjectId = project_id.parse()?;
        Ok(self
            .project_repository
            .get(&id)
            .await?
            .map(|(project, _)| project.snapshot()))
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub async fn create_project(
        &self,
        organization_id: &str,
        request: CreateProjectRequest,
    ) -> anyhow::Result<Option<ProjectSnapshot>> {
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let project = Project::new(self.numbering.next(), id, request.name, request.description)?;
        let snapshot = project.snapshot();
        self.project_repository.save(project, Version::NEW).await?;
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub async fn update_project(
        &self,
        project_id: &str,
        request: UpdateProjectRequest,
    ) -> anyhow::Result<Option<ProjectSnapshot>> {
        let id: ProjectId = project_id.parse()?;
        loop {
            let Some((mut project, version)) = self.project_repository.get(&id).await? else {
                return Ok(None);
            };
            project.update(request.name.clone(), request.description.clone())?;
            let snapshot = project.snapshot();
            match self.project_repository.save(project, version).await {
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

    use wiab_core::organization::Organization;
    use wiab_core::repository::{RepoError, SaveError, Version};

    use super::*;

    #[derive(Default)]
    struct TestProjectRepository {
        projects: RwLock<HashMap<ProjectId, (Project, u64)>>,
    }

    impl ProjectRepository for TestProjectRepository {
        async fn save(&self, project: Project, expected: Version) -> Result<Version, SaveError> {
            let mut projects = self
                .projects
                .write()
                .expect("test repository write lock poisoned");
            let current = projects
                .get(&project.id())
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            projects.insert(project.id(), (project, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &ProjectId) -> Result<Option<(Project, Version)>, RepoError> {
            Ok(self
                .projects
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(project, version)| (project.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Project>, RepoError> {
            Ok(self
                .projects
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(project, _)| project.clone())
                .collect())
        }
    }

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
    struct TestProjectNumbering {
        counter: AtomicU64,
    }

    impl ProjectNumbering for TestProjectNumbering {
        fn next(&self) -> ProjectId {
            ProjectId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    fn service() -> ProjectApplicationService<TestProjectRepository, TestOrganizationRepository> {
        ProjectApplicationService::new(
            TestProjectRepository::default(),
            TestOrganizationRepository::default(),
            Arc::new(TestProjectNumbering::default()),
        )
    }

    async fn seed_organization(
        service: &ProjectApplicationService<TestProjectRepository, TestOrganizationRepository>,
        number: u64,
    ) -> String {
        let organization = Organization::new(
            OrganizationId::from_number(number),
            format!("Org {number}"),
            String::new(),
        )
        .unwrap();
        let id = organization.id().to_string();
        service
            .organization_repository
            .save(organization, Version::NEW)
            .await
            .unwrap();
        id
    }

    async fn create(
        service: &ProjectApplicationService<TestProjectRepository, TestOrganizationRepository>,
        organization_id: &str,
        name: &str,
    ) -> ProjectSnapshot {
        service
            .create_project(
                organization_id,
                CreateProjectRequest {
                    name: name.to_owned(),
                    description: String::new(),
                },
            )
            .await
            .expect("organization id should be valid")
            .expect("organization should exist")
    }

    #[tokio::test]
    async fn create_project_assigns_incrementing_ids() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        assert_eq!(create(&service, &organization_id, "First").await.id, "P-1");
        assert_eq!(create(&service, &organization_id, "Second").await.id, "P-2");
    }

    #[tokio::test]
    async fn create_project_records_organization_id() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let project = create(&service, &organization_id, "Workinabox").await;
        assert_eq!(project.organization_id, organization_id);
    }

    #[tokio::test]
    async fn create_project_under_missing_organization_returns_none() {
        let service = service();
        let result = service
            .create_project(
                "O-9",
                CreateProjectRequest {
                    name: "Workinabox".to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn create_project_rejects_malformed_organization_id() {
        let service = service();
        assert!(
            service
                .create_project(
                    "bogus",
                    CreateProjectRequest {
                        name: "Workinabox".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn create_project_rejects_empty_name() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        assert!(
            service
                .create_project(
                    &organization_id,
                    CreateProjectRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn list_projects_partitions_by_organization() {
        let service = service();
        let first_organization = seed_organization(&service, 1).await;
        let second_organization = seed_organization(&service, 2).await;
        create(&service, &first_organization, "First").await;
        create(&service, &second_organization, "Second").await;
        create(&service, &first_organization, "Third").await;
        service
            .project_repository
            .save(
                Project::new(
                    ProjectId::from_number(10),
                    OrganizationId::from_number(1),
                    "Tenth".to_owned(),
                    String::new(),
                )
                .unwrap(),
                Version::NEW,
            )
            .await
            .unwrap();

        let first_ids = service
            .list_projects(&first_organization)
            .await
            .unwrap()
            .expect("organization should exist")
            .into_iter()
            .map(|project| project.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["P-1", "P-3", "P-10"]);

        let second_ids = service
            .list_projects(&second_organization)
            .await
            .unwrap()
            .expect("organization should exist")
            .into_iter()
            .map(|project| project.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["P-2"]);
    }

    #[tokio::test]
    async fn list_projects_for_missing_organization_returns_none() {
        let service = service();
        assert!(service.list_projects("O-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_projects_rejects_malformed_organization_id() {
        let service = service();
        assert!(service.list_projects("bogus").await.is_err());
    }

    #[tokio::test]
    async fn project_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.project_snapshot("P-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn project_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.project_snapshot("bogus").await.is_err());
    }

    #[tokio::test]
    async fn update_project_replaces_fields_but_not_organization() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let project = create(&service, &organization_id, "Workinabox").await;
        let updated = service
            .update_project(
                &project.id,
                UpdateProjectRequest {
                    name: "Rocket".to_owned(),
                    description: "to the moon".to_owned(),
                },
            )
            .await
            .unwrap()
            .expect("project should exist");
        assert_eq!(updated.name, "Rocket");
        assert_eq!(updated.description, "to the moon");
        assert_eq!(updated.organization_id, organization_id);

        let reloaded = service
            .project_snapshot(&project.id)
            .await
            .unwrap()
            .expect("project should exist");
        assert_eq!(reloaded.name, "Rocket");
    }

    #[tokio::test]
    async fn update_missing_project_returns_none() {
        let service = service();
        let result = service
            .update_project(
                "P-9",
                UpdateProjectRequest {
                    name: "Rocket".to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_project_rejects_empty_name() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let project = create(&service, &organization_id, "Workinabox").await;
        assert!(
            service
                .update_project(
                    &project.id,
                    UpdateProjectRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }
}
