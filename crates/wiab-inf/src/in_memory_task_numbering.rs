use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use wiab_core::task::{TaskId, TaskNumbering};

/// Mints sequential `T-###` numbers from an in-process atomic counter starting at 1.
#[derive(Debug, Clone, Default)]
pub struct InMemoryTaskNumbering {
    counter: Arc<AtomicU64>,
}

impl InMemoryTaskNumbering {
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

impl TaskNumbering for InMemoryTaskNumbering {
    fn next(&self) -> TaskId {
        TaskId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
    }
}
