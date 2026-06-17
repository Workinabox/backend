use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use authbox_core::auth::{AuthError, CredentialStore, PasswordCredential, PrincipalId};

/// In-memory password-credential store keyed by principal (one password per user).
#[derive(Clone, Default)]
pub struct InMemoryCredentialStore {
    by_principal: Arc<RwLock<HashMap<String, PasswordCredential>>>,
}

impl InMemoryCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CredentialStore for InMemoryCredentialStore {
    async fn find_password(
        &self,
        principal: &PrincipalId,
    ) -> Result<Option<PasswordCredential>, AuthError> {
        Ok(self
            .by_principal
            .read()
            .expect("credential store read lock poisoned")
            .get(principal.as_str())
            .cloned())
    }

    async fn save_password(&self, credential: PasswordCredential) -> Result<(), AuthError> {
        self.by_principal
            .write()
            .expect("credential store write lock poisoned")
            .insert(credential.principal().as_str().to_owned(), credential);
        Ok(())
    }
}
