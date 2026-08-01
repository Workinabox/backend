use crate::team::TeamId;

/// Port that mints the next sequential `TM-###` identifier. Sequential human-readable ids
/// need shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait TeamNumbering: Send + Sync {
    fn next(&self) -> TeamId;
}
