use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use wiab_core::organization::OrganizationId;
use wiab_core::repo::RepoId;
use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::user::{
    AccessToken, SshKey, SshKeyId, TokenId, TokenScope, User, UserId, UserKind, UserRepository,
};

/// PostgreSQL-backed user repository. One row per aggregate in `app_user`, guarded by an
/// optimistic-concurrency `version` column, with owned SSH keys and access tokens stored
/// in the `user_ssh_key` and `user_access_token` child tables. Each save rewrites the
/// child rows inside the same transaction as the parent's version CAS.
#[derive(Clone)]
pub struct PostgresUserRepository {
    pool: Pool,
}

impl PostgresUserRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn repo_error<E: std::fmt::Display>(error: E) -> RepoError {
    RepoError::Backend(error.to_string())
}

fn save_error<E: std::fmt::Display>(error: E) -> SaveError {
    SaveError::Backend(error.to_string())
}

/// JSON shape persisted in `user_access_token.scope`. `TokenScope` itself does not derive
/// Serialize/Deserialize and its ids live in other domain modules, so we encode/decode
/// through this infrastructure-local DTO using the public accessors / `TokenScope::new`.
#[derive(Serialize, Deserialize)]
struct ScopeJson {
    read_only: bool,
    repos: Option<Vec<String>>,
    orgs: Option<Vec<String>>,
}

fn scope_to_json(scope: &TokenScope) -> Result<String, SaveError> {
    let dto = ScopeJson {
        read_only: scope.is_read_only(),
        repos: scope
            .repos()
            .map(|repos| repos.iter().map(|repo| repo.to_string()).collect()),
        orgs: scope
            .orgs()
            .map(|orgs| orgs.iter().map(|org| org.to_string()).collect()),
    };
    serde_json::to_string(&dto).map_err(save_error)
}

fn scope_from_json(json: &str) -> Result<TokenScope, RepoError> {
    let dto: ScopeJson = serde_json::from_str(json).map_err(repo_error)?;
    let repos = dto
        .repos
        .map(|repos| {
            repos
                .iter()
                .map(|repo| repo.parse::<RepoId>())
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(repo_error)?;
    let orgs = dto
        .orgs
        .map(|orgs| {
            orgs.iter()
                .map(|org| org.parse::<OrganizationId>())
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(repo_error)?;
    Ok(TokenScope::new(dto.read_only, repos, orgs))
}

impl UserRepository for PostgresUserRepository {
    async fn save(&self, user: User, expected: Version) -> Result<Version, SaveError> {
        let mut client = self.pool.get().await.map_err(save_error)?;
        let id = user.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let kind = user.kind().to_string();
        let email = user.email().map(|email| email.to_owned());
        let agent_id = user.agent_id().map(|agent_id| agent_id.to_string());

        let tx = client.transaction().await.map_err(save_error)?;

        let rows = if expected == Version::NEW {
            tx.execute(
                "INSERT INTO app_user (id, version, kind, name, email, agent_id) \
                 VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (id) DO NOTHING",
                &[&id, &next_version, &kind, &user.name(), &email, &agent_id],
            )
            .await
            .map_err(save_error)?
        } else {
            tx.execute(
                "UPDATE app_user SET version = $2, kind = $3, name = $4, email = $5, \
                 agent_id = $6 WHERE id = $1 AND version = $7",
                &[
                    &id,
                    &next_version,
                    &kind,
                    &user.name(),
                    &email,
                    &agent_id,
                    &(expected.value() as i64),
                ],
            )
            .await
            .map_err(save_error)?
        };
        if rows == 0 {
            // tx drops without commit -> rolled back.
            return Err(SaveError::Conflict);
        }

        tx.execute("DELETE FROM user_ssh_key WHERE user_id = $1", &[&id])
            .await
            .map_err(save_error)?;
        for (position, key) in user.ssh_keys().iter().enumerate() {
            tx.execute(
                "INSERT INTO user_ssh_key \
                 (user_id, position, id, label, openssh_public_key, fingerprint) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &id,
                    &(position as i32),
                    &key.id().to_string(),
                    &key.label(),
                    &key.openssh_public_key(),
                    &key.fingerprint(),
                ],
            )
            .await
            .map_err(save_error)?;
        }

        tx.execute("DELETE FROM user_access_token WHERE user_id = $1", &[&id])
            .await
            .map_err(save_error)?;
        for (position, token) in user.tokens().iter().enumerate() {
            let scope = scope_to_json(token.scope())?;
            tx.execute(
                "INSERT INTO user_access_token \
                 (user_id, position, id, label, hash, display, created_at, expires_at, \
                 last_used_at, scope) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                &[
                    &id,
                    &(position as i32),
                    &token.id().to_string(),
                    &token.label(),
                    &token.hash(),
                    &token.display(),
                    &token.created_at(),
                    &token.expires_at().map(|at| at.to_owned()),
                    &token.last_used_at().map(|at| at.to_owned()),
                    &scope,
                ],
            )
            .await
            .map_err(save_error)?;
        }

        tx.commit().await.map_err(save_error)?;
        Ok(next)
    }

    async fn get(&self, id: &UserId) -> Result<Option<(User, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let id_str = id.to_string();
        let row = client
            .query_opt(
                "SELECT version, kind, name, email, agent_id FROM app_user WHERE id = $1",
                &[&id_str],
            )
            .await
            .map_err(repo_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let version: i64 = row.get(0);
        let kind: String = row.get(1);
        let name: String = row.get(2);
        let email: Option<String> = row.get(3);
        let agent_id: Option<String> = row.get(4);
        let kind: UserKind = kind.parse().map_err(repo_error)?;
        let agent_id = agent_id
            .map(|agent_id| agent_id.parse().map_err(repo_error))
            .transpose()?;

        let ssh_keys = load_ssh_keys(&client, &id_str).await?;
        let tokens = load_tokens(&client, &id_str).await?;

        let user = User::from_persistence(*id, kind, name, email, agent_id, ssh_keys, tokens);
        Ok(Some((user, Version::from_value(version as u64))))
    }

    async fn list(&self) -> Result<Vec<User>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query("SELECT id, kind, name, email, agent_id FROM app_user", &[])
            .await
            .map_err(repo_error)?;
        let mut users = Vec::with_capacity(rows.len());
        for row in rows {
            let id_str: String = row.get(0);
            let id: UserId = id_str.parse().map_err(repo_error)?;
            let kind: String = row.get(1);
            let name: String = row.get(2);
            let email: Option<String> = row.get(3);
            let agent_id: Option<String> = row.get(4);
            let kind: UserKind = kind.parse().map_err(repo_error)?;
            let agent_id = agent_id
                .map(|agent_id| agent_id.parse().map_err(repo_error))
                .transpose()?;

            let ssh_keys = load_ssh_keys(&client, &id_str).await?;
            let tokens = load_tokens(&client, &id_str).await?;

            users.push(User::from_persistence(
                id, kind, name, email, agent_id, ssh_keys, tokens,
            ));
        }
        Ok(users)
    }
}

async fn load_ssh_keys(
    client: &deadpool_postgres::Client,
    user_id: &str,
) -> Result<Vec<SshKey>, RepoError> {
    let rows = client
        .query(
            "SELECT id, label, openssh_public_key, fingerprint FROM user_ssh_key \
             WHERE user_id = $1 ORDER BY position",
            &[&user_id],
        )
        .await
        .map_err(repo_error)?;
    rows.into_iter()
        .map(|row| {
            let id: String = row.get(0);
            let id: SshKeyId = id.parse().map_err(repo_error)?;
            let label: String = row.get(1);
            let openssh_public_key: String = row.get(2);
            let fingerprint: String = row.get(3);
            Ok(SshKey::from_persistence(
                id,
                label,
                openssh_public_key,
                fingerprint,
            ))
        })
        .collect()
}

async fn load_tokens(
    client: &deadpool_postgres::Client,
    user_id: &str,
) -> Result<Vec<AccessToken>, RepoError> {
    let rows = client
        .query(
            "SELECT id, label, hash, display, created_at, expires_at, last_used_at, scope \
             FROM user_access_token WHERE user_id = $1 ORDER BY position",
            &[&user_id],
        )
        .await
        .map_err(repo_error)?;
    rows.into_iter()
        .map(|row| {
            let id: String = row.get(0);
            let id: TokenId = id.parse().map_err(repo_error)?;
            let label: String = row.get(1);
            let hash: String = row.get(2);
            let display: String = row.get(3);
            let created_at: String = row.get(4);
            let expires_at: Option<String> = row.get(5);
            let last_used_at: Option<String> = row.get(6);
            let scope: String = row.get(7);
            let scope = scope_from_json(&scope)?;
            Ok(AccessToken::from_persistence(
                id,
                label,
                hash,
                display,
                created_at,
                expires_at,
                last_used_at,
                scope,
            ))
        })
        .collect()
}
