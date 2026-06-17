use crate::auth::{AuthError, PasswordCredential, PrincipalId};

/// Port for persisting password credentials, keyed by principal (one password per user).
#[allow(async_fn_in_trait)]
pub trait CredentialStore: Send + Sync + 'static {
    async fn find_password(
        &self,
        principal: &PrincipalId,
    ) -> Result<Option<PasswordCredential>, AuthError>;
    async fn save_password(&self, credential: PasswordCredential) -> Result<(), AuthError>;
}
