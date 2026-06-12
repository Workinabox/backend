use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use wiab_core::repo::{RepoId, RepoNumbering};

/// Mints sequential `R-###` numbers from an in-process atomic counter starting at 1.
#[derive(Debug, Clone, Default)]
pub struct InMemoryRepoNumbering {
    counter: Arc<AtomicU64>,
}

impl InMemoryRepoNumbering {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RepoNumbering for InMemoryRepoNumbering {
    fn next(&self) -> RepoId {
        RepoId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
    }
}
