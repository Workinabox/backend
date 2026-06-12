use crate::project::{Project, ProjectId};

/// Port for persisting project aggregates. One repository per aggregate root.
pub trait ProjectRepository: Send + Sync + 'static {
    fn save(&self, project: Project);
    fn get(&self, id: &ProjectId) -> Option<Project>;
    fn list(&self) -> Vec<Project>;
}
