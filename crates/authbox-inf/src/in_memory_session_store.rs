use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use authbox_core::auth::{AuthError, PrincipalId, Session, SessionStore};

/// In-memory session store keyed by the cookie-secret hash. Shares an `Arc` so clones see
/// the same data (matching the in-memory repository pattern).
#[derive(Clone, Default)]
pub struct InMemorySessionStore {
    by_token_hash: Arc<RwLock<HashMap<String, Session>>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SessionStore for InMemorySessionStore {
    async fn put(&self, session: Session) -> Result<(), AuthError> {
        self.by_token_hash
            .write()
            .expect("session store write lock poisoned")
            .insert(session.token_hash().to_owned(), session);
        Ok(())
    }

    async fn find_by_token_hash(&self, token_hash: &str) -> Result<Option<Session>, AuthError> {
        Ok(self
            .by_token_hash
            .read()
            .expect("session store read lock poisoned")
            .get(token_hash)
            .cloned())
    }

    async fn revoke_all_for_principal(&self, principal: &PrincipalId) -> Result<(), AuthError> {
        for session in self
            .by_token_hash
            .write()
            .expect("session store write lock poisoned")
            .values_mut()
        {
            if session.principal() == principal {
                session.revoke();
            }
        }
        Ok(())
    }
}
