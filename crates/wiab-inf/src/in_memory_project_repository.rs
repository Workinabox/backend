use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::project::{Project, ProjectId, ProjectRepository};
use wiab_core::repository::{RepoError, SaveError, Version};

#[derive(Debug, Clone, Default)]
pub struct InMemoryProjectRepository {
    projects: Arc<RwLock<HashMap<ProjectId, (Project, u64)>>>,
}

impl InMemoryProjectRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ProjectRepository for InMemoryProjectRepository {
    async fn save(&self, project: Project, expected: Version) -> Result<Version, SaveError> {
        let mut projects = self
            .projects
            .write()
            .expect("project repository write lock poisoned");
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
            .expect("project repository read lock poisoned")
            .get(id)
            .map(|(project, version)| (project.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Project>, RepoError> {
        Ok(self
            .projects
            .read()
            .expect("project repository read lock poisoned")
            .values()
            .map(|(project, _)| project.clone())
            .collect())
    }
}
