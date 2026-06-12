use crate::pipeline::PipelineId;

/// Port that mints the next sequential `PL-###` identifier. Sequential human-readable ids
/// need shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait PipelineNumbering: Send + Sync {
    fn next(&self) -> PipelineId;
}
