use crate::task::TaskId;

/// Port that mints the next sequential `T-###` identifier. Sequential human-readable ids need
/// shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait TaskNumbering: Send + Sync {
    fn next(&self) -> TaskId;
}
