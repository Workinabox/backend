use std::fmt::Display;

use authbox_core::auth::{AuthError, PrincipalId, Session, SessionId, SessionStore};
use deadpool_postgres::Pool;

/// PostgreSQL-backed session store. One row per session in `auth_session`, resolved by the
/// indexed `token_hash`. `put` upserts by id (sessions are last-write-wins, not versioned).
#[derive(Clone)]
pub struct PostgresSessionStore {
    pool: Pool,
}

impl PostgresSessionStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn backend<E: Display>(error: E) -> AuthError {
    AuthError::Backend(error.to_string())
}

impl SessionStore for PostgresSessionStore {
    async fn put(&self, session: Session) -> Result<(), AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        client
            .execute(
                "INSERT INTO auth_session \
                 (id, token_hash, csrf_hash, principal_id, created_at, last_seen_at, \
                 idle_expires_at, absolute_expires_at, revoked) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
                 ON CONFLICT (id) DO UPDATE SET \
                 token_hash = EXCLUDED.token_hash, csrf_hash = EXCLUDED.csrf_hash, \
                 principal_id = EXCLUDED.principal_id, created_at = EXCLUDED.created_at, \
                 last_seen_at = EXCLUDED.last_seen_at, idle_expires_at = EXCLUDED.idle_expires_at, \
                 absolute_expires_at = EXCLUDED.absolute_expires_at, revoked = EXCLUDED.revoked",
                &[
                    &session.id().to_string(),
                    &session.token_hash(),
                    &session.csrf_hash(),
                    &session.principal().as_str(),
                    &session.created_at(),
                    &session.last_seen_at(),
                    &session.idle_expires_at(),
                    &session.absolute_expires_at(),
                    &session.is_revoked(),
                ],
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn find_by_token_hash(&self, token_hash: &str) -> Result<Option<Session>, AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        let row = client
            .query_opt(
                "SELECT id, token_hash, csrf_hash, principal_id, created_at, last_seen_at, \
                 idle_expires_at, absolute_expires_at, revoked \
                 FROM auth_session WHERE token_hash = $1",
                &[&token_hash],
            )
            .await
            .map_err(backend)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let id: String = row.get(0);
        let id: SessionId = id.parse()?;
        let token_hash: String = row.get(1);
        let csrf_hash: String = row.get(2);
        let principal: String = row.get(3);
        let created_at: String = row.get(4);
        let last_seen_at: String = row.get(5);
        let idle_expires_at: String = row.get(6);
        let absolute_expires_at: String = row.get(7);
        let revoked: bool = row.get(8);
        Ok(Some(Session::from_persistence(
            id,
            PrincipalId::new(principal),
            token_hash,
            csrf_hash,
            created_at,
            last_seen_at,
            idle_expires_at,
            absolute_expires_at,
            revoked,
        )))
    }

    async fn revoke_all_for_principal(&self, principal: &PrincipalId) -> Result<(), AuthError> {
        let client = self.pool.get().await.map_err(backend)?;
        client
            .execute(
                "UPDATE auth_session SET revoked = TRUE WHERE principal_id = $1",
                &[&principal.as_str()],
            )
            .await
            .map_err(backend)?;
        Ok(())
    }
}
