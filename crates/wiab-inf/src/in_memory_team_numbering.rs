use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use wiab_core::team::{TeamId, TeamNumbering};

/// Mints sequential `TM-###` numbers from an in-process atomic counter starting at 1.
#[derive(Debug, Clone, Default)]
pub struct InMemoryTeamNumbering {
    counter: Arc<AtomicU64>,
}

impl InMemoryTeamNumbering {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resume minting after `last`, so the next id is `last + 1`.
    pub fn starting_at(last: u64) -> Self {
        Self {
            counter: Arc::new(AtomicU64::new(last)),
        }
    }
}

impl TeamNumbering for InMemoryTeamNumbering {
    fn next(&self) -> TeamId {
        TeamId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
    }
}
