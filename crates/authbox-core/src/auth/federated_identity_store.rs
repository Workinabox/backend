use crate::auth::{AuthError, FederatedIdentity};

/// Port for persisting federated-identity links, resolved by `(issuer, subject)`.
#[allow(async_fn_in_trait)]
pub trait FederatedIdentityStore: Send + Sync + 'static {
    async fn find(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<FederatedIdentity>, AuthError>;
    async fn link(&self, identity: FederatedIdentity) -> Result<(), AuthError>;
}
