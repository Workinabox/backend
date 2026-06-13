use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use wiab_core::user::{UserId, UserNumbering};

/// Mints sequential `U-###` numbers from an in-process atomic counter starting at 1.
#[derive(Debug, Clone, Default)]
pub struct InMemoryUserNumbering {
    counter: Arc<AtomicU64>,
}

impl InMemoryUserNumbering {
    pub fn new() -> Self {
        Self::default()
    }
}

impl UserNumbering for InMemoryUserNumbering {
    fn next(&self) -> UserId {
        UserId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
    }
}
