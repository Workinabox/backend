use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use authbox_core::auth::{AuthError, FederatedIdentity, FederatedIdentityStore};

/// In-memory federated-identity store keyed by `(issuer, subject)`.
#[derive(Clone, Default)]
pub struct InMemoryFederatedIdentityStore {
    by_subject: Arc<RwLock<HashMap<(String, String), FederatedIdentity>>>,
}

impl InMemoryFederatedIdentityStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FederatedIdentityStore for InMemoryFederatedIdentityStore {
    async fn find(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<FederatedIdentity>, AuthError> {
        Ok(self
            .by_subject
            .read()
            .expect("federated store read lock poisoned")
            .get(&(issuer.to_owned(), subject.to_owned()))
            .cloned())
    }

    async fn link(&self, identity: FederatedIdentity) -> Result<(), AuthError> {
        self.by_subject
            .write()
            .expect("federated store write lock poisoned")
            .insert(
                (identity.issuer().to_owned(), identity.subject().to_owned()),
                identity,
            );
        Ok(())
    }
}
