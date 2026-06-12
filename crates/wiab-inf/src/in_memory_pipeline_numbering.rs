use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use wiab_core::pipeline::{PipelineId, PipelineNumbering};

/// Mints sequential `PL-###` numbers from an in-process atomic counter starting at 1.
#[derive(Debug, Clone, Default)]
pub struct InMemoryPipelineNumbering {
    counter: Arc<AtomicU64>,
}

impl InMemoryPipelineNumbering {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PipelineNumbering for InMemoryPipelineNumbering {
    fn next(&self) -> PipelineId {
        PipelineId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
    }
}
