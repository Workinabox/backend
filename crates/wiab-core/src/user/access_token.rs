use crate::user::{TokenId, TokenScope, TokenScopeSnapshot, TokenSnapshot, UserError};

/// A hashed access token owned by a `User`, used to authenticate git/API over HTTPS.
///
/// The plaintext is never stored — only its SHA-256 hash (computed by infrastructure)
/// and a non-secret `display` (e.g. `wiab_pat_…a1b2`) for listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken {
    id: TokenId,
    label: String,
    hash: String,
    display: String,
    created_at: String,
    expires_at: Option<String>,
    last_used_at: Option<String>,
    scope: TokenScope,
}

impl AccessToken {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: TokenId,
        label: String,
        hash: String,
        display: String,
        created_at: String,
        expires_at: Option<String>,
        scope: TokenScope,
    ) -> Result<Self, UserError> {
        if label.trim().is_empty() {
            return Err(UserError::EmptyTokenLabel);
        }
        if hash.trim().is_empty() {
            return Err(UserError::EmptyTokenHash);
        }
        Ok(Self {
            id,
            label,
            hash,
            display,
            created_at,
            expires_at,
            last_used_at: None,
            scope,
        })
    }

    /// Reconstitute an access token from persisted state (used by repository
    /// implementations). Bypasses validation: the data was already validated on creation.
    #[allow(clippy::too_many_arguments)]
    pub fn from_persistence(
        id: TokenId,
        label: String,
        hash: String,
        display: String,
        created_at: String,
        expires_at: Option<String>,
        last_used_at: Option<String>,
        scope: TokenScope,
    ) -> AccessToken {
        Self {
            id,
            label,
            hash,
            display,
            created_at,
            expires_at,
            last_used_at,
            scope,
        }
    }

    pub fn id(&self) -> TokenId {
        self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn display(&self) -> &str {
        &self.display
    }

    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    pub fn expires_at(&self) -> Option<&str> {
        self.expires_at.as_deref()
    }

    pub fn last_used_at(&self) -> Option<&str> {
        self.last_used_at.as_deref()
    }

    pub fn matches_hash(&self, hash: &str) -> bool {
        self.hash == hash
    }

    pub fn scope(&self) -> &TokenScope {
        &self.scope
    }

    /// RFC3339 timestamps compare lexicographically, so a plain string compare answers
    /// expiry without parsing.
    pub fn is_expired(&self, now_rfc3339: &str) -> bool {
        self.expires_at
            .as_ref()
            .is_some_and(|expiry| now_rfc3339 >= expiry.as_str())
    }

    pub fn mark_used(&mut self, now_rfc3339: String) {
        self.last_used_at = Some(now_rfc3339);
    }

    pub fn snapshot(&self) -> TokenSnapshot {
        TokenSnapshot {
            id: self.id.to_string(),
            label: self.label.clone(),
            display: self.display.clone(),
            created_at: self.created_at.clone(),
            expires_at: self.expires_at.clone(),
            last_used_at: self.last_used_at.clone(),
            scope: TokenScopeSnapshot {
                read_only: self.scope.is_read_only(),
                repos: self
                    .scope
                    .repos()
                    .map(|repos| repos.iter().map(|repo| repo.to_string()).collect()),
                orgs: self
                    .scope
                    .orgs()
                    .map(|orgs| orgs.iter().map(|org| org.to_string()).collect()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user::TokenScope;

    fn token(expires_at: Option<&str>) -> AccessToken {
        AccessToken::new(
            TokenId::new(),
            "ci".to_owned(),
            "hash-xyz".to_owned(),
            "wiab_pat_…abcd".to_owned(),
            "2026-01-01T00:00:00Z".to_owned(),
            expires_at.map(|value| value.to_owned()),
            TokenScope::unrestricted(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_blank_label_or_hash() {
        assert_eq!(
            AccessToken::new(
                TokenId::new(),
                "  ".to_owned(),
                "h".to_owned(),
                "d".to_owned(),
                "t".to_owned(),
                None,
                TokenScope::unrestricted(),
            )
            .unwrap_err(),
            UserError::EmptyTokenLabel
        );
        assert_eq!(
            AccessToken::new(
                TokenId::new(),
                "label".to_owned(),
                "  ".to_owned(),
                "d".to_owned(),
                "t".to_owned(),
                None,
                TokenScope::unrestricted(),
            )
            .unwrap_err(),
            UserError::EmptyTokenHash
        );
    }

    #[test]
    fn matches_hash_and_exposes_scope_and_id() {
        let token = token(None);
        assert!(token.matches_hash("hash-xyz"));
        assert!(!token.matches_hash("nope"));
        assert!(!token.scope().is_read_only());
        // id() is stable across calls.
        assert_eq!(token.id(), token.id());
    }

    #[test]
    fn expiry_compares_lexically() {
        let token = token(Some("2026-06-01T00:00:00Z"));
        assert!(token.is_expired("2026-07-01T00:00:00Z"));
        assert!(!token.is_expired("2026-05-01T00:00:00Z"));
        assert!(!self::token(None).is_expired("2030-01-01T00:00:00Z"));
    }

    #[test]
    fn mark_used_surfaces_in_snapshot() {
        let mut token = token(None);
        assert!(token.snapshot().last_used_at.is_none());
        token.mark_used("2026-06-13T00:00:00Z".to_owned());
        let snapshot = token.snapshot();
        assert_eq!(
            snapshot.last_used_at.as_deref(),
            Some("2026-06-13T00:00:00Z")
        );
        assert_eq!(snapshot.label, "ci");
        assert_eq!(snapshot.display, "wiab_pat_…abcd");
        assert!(!snapshot.scope.read_only);
        assert!(snapshot.scope.repos.is_none());
    }
}
