//! Per-IP rate limiting for the endpoints an attacker guesses against.
//!
//! Nothing throttled password login, password-reset confirmation, signup, or token issuance, so
//! online guessing was bounded only by the network. Argon2's cost makes each attempt expensive
//! for *us*, not for the attacker.
//!
//! Two limits rather than one: the credential-guessing endpoints get a tight one, and the git
//! transport a looser one, because a normal `git clone` legitimately makes a burst of requests
//! and must not be throttled into failure.
//!
//! **The client address has to be trustworthy.** `SmartIpKeyExtractor` reads
//! `X-Forwarded-For` before falling back to the socket peer, which is right behind a proxy and
//! forgeable without one. `frontend/nginx.conf` therefore *overwrites* that header with
//! `$remote_addr` rather than appending to it — appending would let a client prepend its own
//! value and get a fresh bucket per request, which is worse than no limiter at all because it
//! looks like protection. If this backend is ever exposed directly, drop
//! `SmartIpKeyExtractor` for `PeerIpKeyExtractor`.

use std::sync::Arc;

use tower_governor::governor::{GovernorConfig, GovernorConfigBuilder};
use tower_governor::key_extractor::SmartIpKeyExtractor;

/// Steady-state allowance on the credential endpoints: one request every two seconds per IP.
const AUTH_REPLENISH_MILLIS: u64 = 2_000;

/// How many attempts an IP may make back-to-back before the steady rate applies. Enough for a
/// person mistyping a password a few times; useless for guessing.
const AUTH_BURST: u32 = 10;

/// The git transport replenishes faster — a clone is many requests from one legitimate client.
const GIT_REPLENISH_MILLIS: u64 = 100;
const GIT_BURST: u32 = 60;

/// Rate-limit configuration, built once and shared by the layers that use it.
pub struct RateLimits {
    pub auth: Arc<GovernorConfig<SmartIpKeyExtractor, governor::middleware::NoOpMiddleware>>,
    pub git: Arc<GovernorConfig<SmartIpKeyExtractor, governor::middleware::NoOpMiddleware>>,
}

impl RateLimits {
    pub fn new() -> Self {
        Self {
            auth: Arc::new(build(AUTH_REPLENISH_MILLIS, AUTH_BURST)),
            git: Arc::new(build(GIT_REPLENISH_MILLIS, GIT_BURST)),
        }
    }
}

impl Default for RateLimits {
    fn default() -> Self {
        Self::new()
    }
}

fn build(
    replenish_millis: u64,
    burst: u32,
) -> GovernorConfig<SmartIpKeyExtractor, governor::middleware::NoOpMiddleware> {
    GovernorConfigBuilder::default()
        .per_millisecond(replenish_millis)
        .burst_size(burst)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
        .expect("rate-limit configuration is constant and valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_configuration_is_constructible() {
        // `finish()` returns None for a nonsensical combination (e.g. a zero burst), and the
        // builder is the only thing standing between a typo here and a panic at startup.
        let limits = RateLimits::new();
        assert!(!Arc::ptr_eq(&limits.auth, &limits.git));
    }

    #[test]
    fn credential_endpoints_are_stricter_than_git() {
        // A clone is many requests from one legitimate client; a login is not.
        assert!(GIT_REPLENISH_MILLIS < AUTH_REPLENISH_MILLIS);
        assert!(GIT_BURST > AUTH_BURST);
    }
}
