use crate::work::{Work, WorkId};

/// Port for persisting work aggregates. One repository per aggregate root; the whole tree
/// persists as part of its root, so only root works are stored and listed.
pub trait WorkRepository: Send + Sync + 'static {
    fn save(&self, work: Work);
    fn get(&self, id: &WorkId) -> Option<Work>;
    fn list(&self) -> Vec<Work>;
}
