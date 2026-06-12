use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::project::{Project, ProjectId, ProjectRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryProjectRepository {
    projects: Arc<RwLock<HashMap<ProjectId, Project>>>,
}

impl InMemoryProjectRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ProjectRepository for InMemoryProjectRepository {
    fn save(&self, project: Project) {
        self.projects
            .write()
            .expect("project repository write lock poisoned")
            .insert(project.id(), project);
    }

    fn get(&self, id: &ProjectId) -> Option<Project> {
        self.projects
            .read()
            .expect("project repository read lock poisoned")
            .get(id)
            .cloned()
    }

    fn list(&self) -> Vec<Project> {
        self.projects
            .read()
            .expect("project repository read lock poisoned")
            .values()
            .cloned()
            .collect()
    }
}
