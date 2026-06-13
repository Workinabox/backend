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

    pub fn id(&self) -> TokenId {
        self.id
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
