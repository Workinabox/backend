use crate::board::BoardId;

/// Port that mints the next sequential `B-###` identifier. Sequential human-readable ids
/// need shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait BoardNumbering: Send + Sync {
    fn next(&self) -> BoardId;
}
