use crate::organization::OrganizationId;

/// Port that mints the next sequential `O-###` identifier. Sequential human-readable ids
/// need shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait OrganizationNumbering: Send + Sync {
    fn next(&self) -> OrganizationId;
}
