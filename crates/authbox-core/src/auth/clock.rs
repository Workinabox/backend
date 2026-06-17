/// Time seam: the current instant and computed future instants as RFC3339 strings.
///
/// RFC3339 timestamps compare lexicographically, so expiry checks elsewhere are plain
/// string comparisons. This port lives in the domain so the auth model carries no date
/// math — the infrastructure impl computes `now` and `now + seconds`.
pub trait Clock: Send + Sync {
    fn now_rfc3339(&self) -> String;

    /// `now + seconds`, as an RFC3339 timestamp — used to compute session/token expiries.
    fn rfc3339_in(&self, seconds: i64) -> String;
}
