use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::pipeline::{Pipeline, PipelineId, PipelineRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryPipelineRepository {
    pipelines: Arc<RwLock<HashMap<PipelineId, Pipeline>>>,
}

impl InMemoryPipelineRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PipelineRepository for InMemoryPipelineRepository {
    fn save(&self, pipeline: Pipeline) {
        self.pipelines
            .write()
            .expect("pipeline repository write lock poisoned")
            .insert(pipeline.id(), pipeline);
    }

    fn get(&self, id: &PipelineId) -> Option<Pipeline> {
        self.pipelines
            .read()
            .expect("pipeline repository read lock poisoned")
            .get(id)
            .cloned()
    }

    fn list(&self) -> Vec<Pipeline> {
        self.pipelines
            .read()
            .expect("pipeline repository read lock poisoned")
            .values()
            .cloned()
            .collect()
    }
}
