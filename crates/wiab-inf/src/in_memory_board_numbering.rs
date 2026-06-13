use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use wiab_core::board::{BoardId, BoardNumbering};

/// Mints sequential `B-###` numbers from an in-process atomic counter starting at 1.
#[derive(Debug, Clone, Default)]
pub struct InMemoryBoardNumbering {
    counter: Arc<AtomicU64>,
}

impl InMemoryBoardNumbering {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resume minting after `last`, so the next id is `last + 1`. Used to continue
    /// sequential numbering from the highest persisted id after a restart.
    pub fn starting_at(last: u64) -> Self {
        Self {
            counter: Arc::new(AtomicU64::new(last)),
        }
    }
}

impl BoardNumbering for InMemoryBoardNumbering {
    fn next(&self) -> BoardId {
        BoardId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
    }
}
