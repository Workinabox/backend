use std::fmt::Display;

use authbox_core::auth::{AuthError, FederatedIdentity, FederatedIdentityStore, PrincipalId};
use deadpool_postgres::Pool;

/// PostgreSQL-backed federated-identity store, resolved by the unique `(issuer, subject)`.
#[derive(Clone)]
pub struct PostgresFederatedIdentityStore {
    pool: Pool,
}

impl PostgresFederatedIdentityStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn backend<E: Display>(error: E) -> AuthError {
    AuthError::Backend(error.to_string())
}

impl FederatedIdentityStore for PostgresFederatedIdentityStore {
    async fn find(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<FederatedIdentity>, AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        let row = client
            .query_opt(
                "SELECT principal_id, issuer, subject, email, linked_at \
                 FROM federated_identity WHERE issuer = $1 AND subject = $2",
                &[&issuer, &subject],
            )
            .await
            .map_err(backend)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let principal: String = row.get(0);
        let issuer: String = row.get(1);
        let subject: String = row.get(2);
        let email: Option<String> = row.get(3);
        let linked_at: String = row.get(4);
        Ok(Some(FederatedIdentity::new(
            PrincipalId::new(principal),
            issuer,
            subject,
            email,
            linked_at,
        )))
    }

    async fn link(&self, identity: FederatedIdentity) -> Result<(), AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        client
            .execute(
                "INSERT INTO federated_identity (issuer, subject, principal_id, email, linked_at) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (issuer, subject) DO UPDATE SET \
                 principal_id = EXCLUDED.principal_id, email = EXCLUDED.email, \
                 linked_at = EXCLUDED.linked_at",
                &[
                    &identity.issuer(),
                    &identity.subject(),
                    &identity.principal().as_str(),
                    &identity.email(),
                    &identity.linked_at(),
                ],
            )
            .await
            .map_err(backend)?;
        Ok(())
    }
}
