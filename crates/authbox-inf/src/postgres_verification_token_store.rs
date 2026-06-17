use std::fmt::Display;

use authbox_core::auth::{
    AuthError, PrincipalId, VerificationPurpose, VerificationToken, VerificationTokenStore,
};
use deadpool_postgres::Pool;

/// PostgreSQL-backed single-use verification-token store. `consume` deletes-and-returns in
/// one statement, so a reset link is usable at most once.
#[derive(Clone)]
pub struct PostgresVerificationTokenStore {
    pool: Pool,
}

impl PostgresVerificationTokenStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn backend<E: Display>(error: E) -> AuthError {
    AuthError::Backend(error.to_string())
}

impl VerificationTokenStore for PostgresVerificationTokenStore {
    async fn put(&self, token: VerificationToken) -> Result<(), AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        client
            .execute(
                "INSERT INTO verification_token (token_hash, purpose, principal_id, expires_at) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (token_hash) DO UPDATE SET \
                 purpose = EXCLUDED.purpose, principal_id = EXCLUDED.principal_id, \
                 expires_at = EXCLUDED.expires_at",
                &[
                    &token.token_hash(),
                    &token.purpose().as_str(),
                    &token.principal().as_str(),
                    &token.expires_at(),
                ],
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn consume(&self, token_hash: &str) -> Result<Option<VerificationToken>, AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        let row = client
            .query_opt(
                "DELETE FROM verification_token WHERE token_hash = $1 \
                 RETURNING purpose, principal_id, expires_at",
                &[&token_hash],
            )
            .await
            .map_err(backend)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let purpose: String = row.get(0);
        let purpose: VerificationPurpose = purpose.parse()?;
        let principal: String = row.get(1);
        let expires_at: String = row.get(2);
        Ok(Some(VerificationToken::new(
            purpose,
            token_hash.to_owned(),
            PrincipalId::new(principal),
            expires_at,
        )))
    }
}
