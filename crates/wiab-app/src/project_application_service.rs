use std::sync::{Arc, Mutex};

use wiab_core::organization::{OrganizationId, OrganizationRepository};
use wiab_core::project::{
    Project, ProjectId, ProjectNumbering, ProjectRepository, ProjectSnapshot,
};

use crate::project_requests::{CreateProjectRequest, UpdateProjectRequest};

/// Orchestrates use cases over the `Project` aggregate.
///
/// Methods are synchronous: `Project` has no external/async collaborators, so a plain
/// `std::sync::Mutex` guard held across each load-mutate-save is enough to prevent lost
/// updates. Holds the organization repository to verify the parent organization exists.
pub struct ProjectApplicationService<P: ProjectRepository, O: OrganizationRepository> {
    project_repository: P,
    organization_repository: O,
    numbering: Arc<dyn ProjectNumbering>,
    mutation_guard: Mutex<()>,
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
            mutation_guard: Mutex::new(()),
        }
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub fn list_projects(
        &self,
        organization_id: &str,
    ) -> anyhow::Result<Option<Vec<ProjectSnapshot>>> {
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).is_none() {
            return Ok(None);
        }
        let mut projects = self
            .project_repository
            .list()
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

    pub fn project_snapshot(&self, project_id: &str) -> anyhow::Result<Option<ProjectSnapshot>> {
        let id: ProjectId = project_id.parse()?;
        Ok(self
            .project_repository
            .get(&id)
            .map(|project| project.snapshot()))
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub fn create_project(
        &self,
        organization_id: &str,
        request: CreateProjectRequest,
    ) -> anyhow::Result<Option<ProjectSnapshot>> {
        let _guard = self.lock();
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).is_none() {
            return Ok(None);
        }
        let project = Project::new(self.numbering.next(), id, request.name, request.description)?;
        let snapshot = project.snapshot();
        self.project_repository.save(project);
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub fn update_project(
        &self,
        project_id: &str,
        request: UpdateProjectRequest,
    ) -> anyhow::Result<Option<ProjectSnapshot>> {
        let _guard = self.lock();
        let id: ProjectId = project_id.parse()?;
        let Some(mut project) = self.project_repository.get(&id) else {
            return Ok(None);
        };
        project.update(request.name, request.description)?;
        let snapshot = project.snapshot();
        self.project_repository.save(project);
        Ok(Some(snapshot))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_guard
            .lock()
            .expect("project mutation guard poisoned")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::organization::Organization;

    use super::*;

    #[derive(Default)]
    struct TestProjectRepository {
        projects: RwLock<HashMap<ProjectId, Project>>,
    }

    impl ProjectRepository for TestProjectRepository {
        fn save(&self, project: Project) {
            self.projects
                .write()
                .expect("test repository write lock poisoned")
                .insert(project.id(), project);
        }

        fn get(&self, id: &ProjectId) -> Option<Project> {
            self.projects
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .cloned()
        }

        fn list(&self) -> Vec<Project> {
            self.projects
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .cloned()
                .collect()
        }
    }

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

    fn seed_organization(
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
        service.organization_repository.save(organization);
        id
    }

    fn create(
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
            .expect("organization id should be valid")
            .expect("organization should exist")
    }

    #[test]
    fn create_project_assigns_incrementing_ids() {
        let service = service();
        let organization_id = seed_organization(&service, 1);
        assert_eq!(create(&service, &organization_id, "First").id, "P-1");
        assert_eq!(create(&service, &organization_id, "Second").id, "P-2");
    }

    #[test]
    fn create_project_records_organization_id() {
        let service = service();
        let organization_id = seed_organization(&service, 1);
        let project = create(&service, &organization_id, "Workinabox");
        assert_eq!(project.organization_id, organization_id);
    }

    #[test]
    fn create_project_under_missing_organization_returns_none() {
        let service = service();
        let result = service
            .create_project(
                "O-9",
                CreateProjectRequest {
                    name: "Workinabox".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn create_project_rejects_malformed_organization_id() {
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
                .is_err()
        );
    }

    #[test]
    fn create_project_rejects_empty_name() {
        let service = service();
        let organization_id = seed_organization(&service, 1);
        assert!(
            service
                .create_project(
                    &organization_id,
                    CreateProjectRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn list_projects_partitions_by_organization() {
        let service = service();
        let first_organization = seed_organization(&service, 1);
        let second_organization = seed_organization(&service, 2);
        create(&service, &first_organization, "First");
        create(&service, &second_organization, "Second");
        create(&service, &first_organization, "Third");
        service.project_repository.save(
            Project::new(
                ProjectId::from_number(10),
                OrganizationId::from_number(1),
                "Tenth".to_owned(),
                String::new(),
            )
            .unwrap(),
        );

        let first_ids = service
            .list_projects(&first_organization)
            .unwrap()
            .expect("organization should exist")
            .into_iter()
            .map(|project| project.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["P-1", "P-3", "P-10"]);

        let second_ids = service
            .list_projects(&second_organization)
            .unwrap()
            .expect("organization should exist")
            .into_iter()
            .map(|project| project.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["P-2"]);
    }

    #[test]
    fn list_projects_for_missing_organization_returns_none() {
        let service = service();
        assert!(service.list_projects("O-9").unwrap().is_none());
    }

    #[test]
    fn list_projects_rejects_malformed_organization_id() {
        let service = service();
        assert!(service.list_projects("bogus").is_err());
    }

    #[test]
    fn project_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.project_snapshot("P-9").unwrap().is_none());
    }

    #[test]
    fn project_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.project_snapshot("bogus").is_err());
    }

    #[test]
    fn update_project_replaces_fields_but_not_organization() {
        let service = service();
        let organization_id = seed_organization(&service, 1);
        let project = create(&service, &organization_id, "Workinabox");
        let updated = service
            .update_project(
                &project.id,
                UpdateProjectRequest {
                    name: "Rocket".to_owned(),
                    description: "to the moon".to_owned(),
                },
            )
            .unwrap()
            .expect("project should exist");
        assert_eq!(updated.name, "Rocket");
        assert_eq!(updated.description, "to the moon");
        assert_eq!(updated.organization_id, organization_id);

        let reloaded = service
            .project_snapshot(&project.id)
            .unwrap()
            .expect("project should exist");
        assert_eq!(reloaded.name, "Rocket");
    }

    #[test]
    fn update_missing_project_returns_none() {
        let service = service();
        let result = service
            .update_project(
                "P-9",
                UpdateProjectRequest {
                    name: "Rocket".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_project_rejects_empty_name() {
        let service = service();
        let organization_id = seed_organization(&service, 1);
        let project = create(&service, &organization_id, "Workinabox");
        assert!(
            service
                .update_project(
                    &project.id,
                    UpdateProjectRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }
}
