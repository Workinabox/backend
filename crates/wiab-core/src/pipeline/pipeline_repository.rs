use crate::pipeline::{Pipeline, PipelineId};
use crate::repository::{RepoError, SaveError, Version};

/// Port for persisting pipeline aggregates. One repository per aggregate root.
#[allow(async_fn_in_trait)]
pub trait PipelineRepository: Send + Sync + 'static {
    async fn save(&self, pipeline: Pipeline, expected: Version) -> Result<Version, SaveError>;
    async fn get(&self, id: &PipelineId) -> Result<Option<(Pipeline, Version)>, RepoError>;
    async fn list(&self) -> Result<Vec<Pipeline>, RepoError>;
}
