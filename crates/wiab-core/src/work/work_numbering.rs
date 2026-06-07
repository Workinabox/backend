use crate::work::WorkId;

/// Port that mints the next sequential `W-###` identifier. Sequential human-readable ids
/// need shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait WorkNumbering: Send + Sync {
    fn next(&self) -> WorkId;
}
