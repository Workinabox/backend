use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use authbox_core::auth::{AuthError, VerificationToken, VerificationTokenStore};

/// In-memory single-use verification-token store keyed by token hash.
#[derive(Clone, Default)]
pub struct InMemoryVerificationTokenStore {
    by_hash: Arc<RwLock<HashMap<String, VerificationToken>>>,
}

impl InMemoryVerificationTokenStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl VerificationTokenStore for InMemoryVerificationTokenStore {
    async fn put(&self, token: VerificationToken) -> Result<(), AuthError> {
        self.by_hash
            .write()
            .expect("verification store write lock poisoned")
            .insert(token.token_hash().to_owned(), token);
        Ok(())
    }

    async fn consume(&self, token_hash: &str) -> Result<Option<VerificationToken>, AuthError> {
        Ok(self
            .by_hash
            .write()
            .expect("verification store write lock poisoned")
            .remove(token_hash))
    }
}
