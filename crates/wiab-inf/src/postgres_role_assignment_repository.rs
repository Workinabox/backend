use deadpool_postgres::Pool;
use wiab_core::access::{Role, RoleAssignment, RoleAssignmentId, RoleAssignmentRepository, Scope};
use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::user::UserId;

/// PostgreSQL-backed role-assignment repository. One row per aggregate in `role_assignment`,
/// guarded by an optimistic-concurrency `version` column. The grant's `scope` is stored as a
/// `(scope_kind, scope_id)` pair.
#[derive(Clone)]
pub struct PostgresRoleAssignmentRepository {
    pool: Pool,
}

impl PostgresRoleAssignmentRepository {
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

impl RoleAssignmentRepository for PostgresRoleAssignmentRepository {
    async fn save(
        &self,
        assignment: RoleAssignment,
        expected: Version,
    ) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = assignment.id().to_string();
        let user_id = assignment.user_id().to_string();
        let scope = assignment.scope();
        let scope_kind = scope.kind();
        let scope_id = scope.id_string();
        let role = assignment.role().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO role_assignment (id, version, user_id, scope_kind, scope_id, role) \
                     VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (id) DO NOTHING",
                    &[&id, &next_version, &user_id, &scope_kind, &scope_id, &role],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE role_assignment SET version = $2, user_id = $3, scope_kind = $4, \
                     scope_id = $5, role = $6 WHERE id = $1 AND version = $7",
                    &[
                        &id,
                        &next_version,
                        &user_id,
                        &scope_kind,
                        &scope_id,
                        &role,
                        &(expected.value() as i64),
                    ],
                )
                .await
                .map_err(save_error)?
        };
        if rows == 0 {
            return Err(SaveError::Conflict);
        }
        Ok(next)
    }

    async fn get(
        &self,
        id: &RoleAssignmentId,
    ) -> Result<Option<(RoleAssignment, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                "SELECT version, user_id, scope_kind, scope_id, role FROM role_assignment \
                 WHERE id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        match row {
            None => Ok(None),
            Some(row) => {
                let version: i64 = row.get(0);
                let user_id: String = row.get(1);
                let user_id: UserId = user_id.parse().map_err(repo_error)?;
                let scope_kind: String = row.get(2);
                let scope_id: String = row.get(3);
                let scope = Scope::parse(&scope_kind, &scope_id).map_err(repo_error)?;
                let role: String = row.get(4);
                let role: Role = role.parse().map_err(repo_error)?;
                let assignment = RoleAssignment::new(*id, user_id, scope, role);
                Ok(Some((assignment, Version::from_value(version as u64))))
            }
        }
    }

    async fn remove(&self, id: &RoleAssignmentId) -> Result<bool, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .execute(
                "DELETE FROM role_assignment WHERE id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        Ok(rows > 0)
    }

    async fn list(&self) -> Result<Vec<RoleAssignment>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(
                "SELECT id, user_id, scope_kind, scope_id, role FROM role_assignment",
                &[],
            )
            .await
            .map_err(repo_error)?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get(0);
                let id: RoleAssignmentId = id.parse().map_err(repo_error)?;
                let user_id: String = row.get(1);
                let user_id: UserId = user_id.parse().map_err(repo_error)?;
                let scope_kind: String = row.get(2);
                let scope_id: String = row.get(3);
                let scope = Scope::parse(&scope_kind, &scope_id).map_err(repo_error)?;
                let role: String = row.get(4);
                let role: Role = role.parse().map_err(repo_error)?;
                Ok(RoleAssignment::new(id, user_id, scope, role))
            })
            .collect()
    }
}
