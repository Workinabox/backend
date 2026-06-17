use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use authbox_core::auth::{AuthError, AuthFlow, AuthFlowStore};

/// In-memory OIDC login-state store keyed by `state`. `take` is single-use.
#[derive(Clone, Default)]
pub struct InMemoryAuthFlowStore {
    by_state: Arc<RwLock<HashMap<String, AuthFlow>>>,
}

impl InMemoryAuthFlowStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AuthFlowStore for InMemoryAuthFlowStore {
    async fn put(&self, flow: AuthFlow) -> Result<(), AuthError> {
        self.by_state
            .write()
            .expect("auth flow store write lock poisoned")
            .insert(flow.state().to_owned(), flow);
        Ok(())
    }

    async fn take(&self, state: &str) -> Result<Option<AuthFlow>, AuthError> {
        Ok(self
            .by_state
            .write()
            .expect("auth flow store write lock poisoned")
            .remove(state))
    }
}
