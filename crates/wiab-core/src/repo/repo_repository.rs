use crate::repo::{Repo, RepoId};

/// Port for persisting repo aggregates. One repository per aggregate root.
pub trait RepoRepository: Send + Sync + 'static {
    fn save(&self, repo: Repo);
    fn get(&self, id: &RepoId) -> Option<Repo>;
    fn list(&self) -> Vec<Repo>;
}
