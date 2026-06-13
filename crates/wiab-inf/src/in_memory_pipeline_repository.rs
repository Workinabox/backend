use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::pipeline::{Pipeline, PipelineId, PipelineRepository};
use wiab_core::repository::{RepoError, SaveError, Version};

#[derive(Debug, Clone, Default)]
pub struct InMemoryPipelineRepository {
    pipelines: Arc<RwLock<HashMap<PipelineId, (Pipeline, u64)>>>,
}

impl InMemoryPipelineRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PipelineRepository for InMemoryPipelineRepository {
    async fn save(&self, pipeline: Pipeline, expected: Version) -> Result<Version, SaveError> {
        let mut pipelines = self
            .pipelines
            .write()
            .expect("pipeline repository write lock poisoned");
        let current = pipelines
            .get(&pipeline.id())
            .map(|(_, version)| *version)
            .unwrap_or(0);
        if current != expected.value() {
            return Err(SaveError::Conflict);
        }
        let next = expected.next();
        pipelines.insert(pipeline.id(), (pipeline, next.value()));
        Ok(next)
    }

    async fn get(&self, id: &PipelineId) -> Result<Option<(Pipeline, Version)>, RepoError> {
        Ok(self
            .pipelines
            .read()
            .expect("pipeline repository read lock poisoned")
            .get(id)
            .map(|(pipeline, version)| (pipeline.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Pipeline>, RepoError> {
        Ok(self
            .pipelines
            .read()
            .expect("pipeline repository read lock poisoned")
            .values()
            .map(|(pipeline, _)| pipeline.clone())
            .collect())
    }
}
