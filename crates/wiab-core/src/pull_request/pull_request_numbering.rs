use crate::pull_request::PullRequestId;

/// Port that mints the next sequential `PR-###` identifier. Sequential human-readable ids
/// need shared persistent state the domain cannot hold, so it is an infrastructure seam.
pub trait PullRequestNumbering: Send + Sync {
    fn next(&self) -> PullRequestId;
}
