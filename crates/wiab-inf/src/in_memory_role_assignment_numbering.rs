use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use wiab_core::access::{RoleAssignmentId, RoleAssignmentNumbering};

/// Mints sequential `G-###` numbers from an in-process atomic counter starting at 1.
#[derive(Debug, Clone, Default)]
pub struct InMemoryRoleAssignmentNumbering {
    counter: Arc<AtomicU64>,
}

impl InMemoryRoleAssignmentNumbering {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RoleAssignmentNumbering for InMemoryRoleAssignmentNumbering {
    fn next(&self) -> RoleAssignmentId {
        RoleAssignmentId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
    }
}
