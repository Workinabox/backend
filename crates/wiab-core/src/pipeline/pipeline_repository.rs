use crate::pipeline::{Pipeline, PipelineId};

/// Port for persisting pipeline aggregates. One repository per aggregate root.
pub trait PipelineRepository: Send + Sync + 'static {
    fn save(&self, pipeline: Pipeline);
    fn get(&self, id: &PipelineId) -> Option<Pipeline>;
    fn list(&self) -> Vec<Pipeline>;
}
