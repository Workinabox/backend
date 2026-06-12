use crate::repo::RepoId;

/// Port that mints the next sequential `R-###` identifier. Sequential human-readable ids
/// need shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait RepoNumbering: Send + Sync {
    fn next(&self) -> RepoId;
}
