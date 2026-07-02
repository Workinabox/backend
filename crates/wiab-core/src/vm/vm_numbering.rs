use crate::vm::VmId;

/// Port that mints the next sequential `VM-###` identifier. Sequential human-readable ids
/// need shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait VmNumbering: Send + Sync {
    fn next(&self) -> VmId;
}
