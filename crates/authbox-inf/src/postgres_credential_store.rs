use std::fmt::Display;

use authbox_core::auth::{
    AuthError, CredentialStore, PasswordCredential, PasswordState, PrincipalId,
};
use deadpool_postgres::Pool;

/// PostgreSQL-backed password store. One row per principal in `user_password`.
#[derive(Clone)]
pub struct PostgresCredentialStore {
    pool: Pool,
}

impl PostgresCredentialStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn backend<E: Display>(error: E) -> AuthError {
    AuthError::Backend(error.to_string())
}

impl CredentialStore for PostgresCredentialStore {
    async fn find_password(
        &self,
        principal: &PrincipalId,
    ) -> Result<Option<PasswordCredential>, AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        let row = client
            .query_opt(
                "SELECT phc_hash, state, updated_at FROM user_password WHERE user_id = $1",
                &[&principal.as_str()],
            )
            .await
            .map_err(backend)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let phc_hash: String = row.get(0);
        let state: String = row.get(1);
        let updated_at: String = row.get(2);
        let state: PasswordState = state.parse()?;
        Ok(Some(PasswordCredential::from_persistence(
            principal.clone(),
            phc_hash,
            state,
            updated_at,
        )))
    }

    async fn save_password(&self, credential: PasswordCredential) -> Result<(), AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        client
            .execute(
                "INSERT INTO user_password (user_id, phc_hash, state, updated_at) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (user_id) DO UPDATE SET \
                 phc_hash = EXCLUDED.phc_hash, state = EXCLUDED.state, \
                 updated_at = EXCLUDED.updated_at",
                &[
                    &credential.principal().as_str(),
                    &credential.phc_hash(),
                    &credential.state().as_str(),
                    &credential.updated_at(),
                ],
            )
            .await
            .map_err(backend)?;
        Ok(())
    }
}
