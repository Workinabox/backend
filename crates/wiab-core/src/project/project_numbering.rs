use crate::project::ProjectId;

/// Port that mints the next sequential `P-###` identifier. Sequential human-readable ids
/// need shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait ProjectNumbering: Send + Sync {
    fn next(&self) -> ProjectId;
}
