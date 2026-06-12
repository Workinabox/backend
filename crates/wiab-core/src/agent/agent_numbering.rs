use crate::agent::AgentId;

/// Port that mints the next sequential `A-###` identifier. Sequential human-readable ids
/// need shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait AgentNumbering: Send + Sync {
    fn next(&self) -> AgentId;
}
