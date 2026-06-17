use std::fmt::Display;

use authbox_core::auth::{AuthError, AuthFlow, AuthFlowStore};
use deadpool_postgres::Pool;

/// PostgreSQL-backed OIDC login-state store. `take` deletes-and-returns in one statement so
/// the `state` parameter is genuinely single-use.
#[derive(Clone)]
pub struct PostgresAuthFlowStore {
    pool: Pool,
}

impl PostgresAuthFlowStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn backend<E: Display>(error: E) -> AuthError {
    AuthError::Backend(error.to_string())
}

impl AuthFlowStore for PostgresAuthFlowStore {
    async fn put(&self, flow: AuthFlow) -> Result<(), AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        client
            .execute(
                "INSERT INTO auth_flow \
                 (state, connection, nonce, pkce_verifier, return_to, expires_at) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &flow.state(),
                    &flow.connection(),
                    &flow.nonce(),
                    &flow.pkce_verifier(),
                    &flow.return_to(),
                    &flow.expires_at(),
                ],
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn take(&self, state: &str) -> Result<Option<AuthFlow>, AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        let row = client
            .query_opt(
                "DELETE FROM auth_flow WHERE state = $1 \
                 RETURNING state, connection, nonce, pkce_verifier, return_to, expires_at",
                &[&state],
            )
            .await
            .map_err(backend)?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(AuthFlow::new(
            row.get(0),
            row.get(1),
            row.get(2),
            row.get(3),
            row.get(4),
            row.get(5),
        )))
    }
}
