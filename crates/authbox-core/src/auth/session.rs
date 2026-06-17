use crate::auth::{PrincipalId, SessionId};

/// A browser session: a server-side record resolved from an opaque cookie secret (only the
/// secret's hash is stored, never the secret). Carries idle and absolute expiries (RFC3339,
/// compared lexically) and the hash of a CSRF secret. Its own aggregate — sessions churn
/// far faster than users, so they are not versioned (last-write-wins).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    id: SessionId,
    principal: PrincipalId,
    token_hash: String,
    csrf_hash: String,
    created_at: String,
    last_seen_at: String,
    idle_expires_at: String,
    absolute_expires_at: String,
    revoked: bool,
}

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SessionId,
        principal: PrincipalId,
        token_hash: String,
        csrf_hash: String,
        now: String,
        idle_expires_at: String,
        absolute_expires_at: String,
    ) -> Self {
        Self {
            id,
            principal,
            token_hash,
            csrf_hash,
            created_at: now.clone(),
            last_seen_at: now,
            idle_expires_at,
            absolute_expires_at,
            revoked: false,
        }
    }

    /// Reconstitute a session from persisted state (used by store implementations).
    #[allow(clippy::too_many_arguments)]
    pub fn from_persistence(
        id: SessionId,
        principal: PrincipalId,
        token_hash: String,
        csrf_hash: String,
        created_at: String,
        last_seen_at: String,
        idle_expires_at: String,
        absolute_expires_at: String,
        revoked: bool,
    ) -> Self {
        Self {
            id,
            principal,
            token_hash,
            csrf_hash,
            created_at,
            last_seen_at,
            idle_expires_at,
            absolute_expires_at,
            revoked,
        }
    }

    pub fn id(&self) -> SessionId {
        self.id
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }

    pub fn token_hash(&self) -> &str {
        &self.token_hash
    }

    pub fn csrf_hash(&self) -> &str {
        &self.csrf_hash
    }

    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    pub fn last_seen_at(&self) -> &str {
        &self.last_seen_at
    }

    pub fn idle_expires_at(&self) -> &str {
        &self.idle_expires_at
    }

    pub fn absolute_expires_at(&self) -> &str {
        &self.absolute_expires_at
    }

    pub fn is_revoked(&self) -> bool {
        self.revoked
    }

    /// Whether the session may still authenticate a request at `now`: not revoked and before
    /// both the idle and absolute expiries.
    pub fn is_active(&self, now_rfc3339: &str) -> bool {
        !self.revoked
            && now_rfc3339 < self.idle_expires_at.as_str()
            && now_rfc3339 < self.absolute_expires_at.as_str()
    }

    pub fn matches_csrf(&self, csrf_hash: &str) -> bool {
        self.csrf_hash == csrf_hash
    }

    /// Slide the idle window forward on activity. The absolute expiry is never extended.
    pub fn touch(&mut self, now_rfc3339: String, idle_expires_at: String) {
        self.last_seen_at = now_rfc3339;
        self.idle_expires_at = idle_expires_at;
    }

    pub fn revoke(&mut self) {
        self.revoked = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new(
            SessionId::new(),
            PrincipalId::new("U-1"),
            "token-hash".to_owned(),
            "csrf-hash".to_owned(),
            "2026-06-01T00:00:00Z".to_owned(),
            "2026-06-01T08:00:00Z".to_owned(),
            "2026-06-08T00:00:00Z".to_owned(),
        )
    }

    #[test]
    fn active_within_both_windows() {
        let session = session();
        assert!(session.is_active("2026-06-01T01:00:00Z"));
    }

    #[test]
    fn inactive_past_idle_expiry() {
        let session = session();
        assert!(!session.is_active("2026-06-01T08:00:01Z"));
    }

    #[test]
    fn inactive_past_absolute_expiry_even_if_touched() {
        let mut session = session();
        // Idle window slid far forward, but the absolute cap still bites.
        session.touch(
            "2026-06-07T23:00:00Z".to_owned(),
            "2026-06-09T00:00:00Z".to_owned(),
        );
        assert!(!session.is_active("2026-06-08T00:00:01Z"));
    }

    #[test]
    fn revoked_is_never_active() {
        let mut session = session();
        session.revoke();
        assert!(!session.is_active("2026-06-01T01:00:00Z"));
    }

    #[test]
    fn csrf_match_is_exact() {
        let session = session();
        assert!(session.matches_csrf("csrf-hash"));
        assert!(!session.matches_csrf("nope"));
    }
}
