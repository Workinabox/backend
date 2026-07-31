use deadpool_postgres::Pool;
use wiab_core::pull_request::{
    PullRequest, PullRequestId, PullRequestRepository, PullRequestState,
};
use wiab_core::repo::{BranchName, CommitHash, RepoId};
use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::user::UserId;

/// PostgreSQL-backed pull request repository. One row per aggregate in `pull_request`,
/// guarded by an optimistic-concurrency `version` column.
#[derive(Clone)]
pub struct PostgresPullRequestRepository {
    pool: Pool,
}

impl PostgresPullRequestRepository {
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

/// Rebuild an aggregate from a row. Uses `from_persistence`, not `open`, because a stored
/// request may be merged or closed — states `open` cannot produce.
///
/// The argument count mirrors the table's columns; grouping them into a struct would add a
/// type whose only purpose is to be immediately destructured.
#[allow(clippy::too_many_arguments)]
fn from_row(
    id: PullRequestId,
    repo_id: &str,
    author_id: &str,
    title: String,
    description: String,
    source_branch: String,
    target_branch: String,
    state: &str,
    merge_commit: Option<String>,
    opened_at: String,
) -> Result<PullRequest, RepoError> {
    let repo_id: RepoId = repo_id.parse().map_err(repo_error)?;
    let author_id: UserId = author_id.parse().map_err(repo_error)?;
    let source_branch = BranchName::new(source_branch).map_err(repo_error)?;
    let target_branch = BranchName::new(target_branch).map_err(repo_error)?;
    let state: PullRequestState = state.parse().map_err(repo_error)?;
    let merge_commit = merge_commit
        .map(CommitHash::new)
        .transpose()
        .map_err(repo_error)?;
    Ok(PullRequest::from_persistence(
        id,
        repo_id,
        author_id,
        title,
        description,
        source_branch,
        target_branch,
        state,
        merge_commit,
        opened_at,
    ))
}

impl PullRequestRepository for PostgresPullRequestRepository {
    async fn save(
        &self,
        pull_request: PullRequest,
        expected: Version,
    ) -> Result<Version, SaveError> {
        let client = self.pool.get().await.map_err(save_error)?;
        let id = pull_request.id().to_string();
        let next = expected.next();
        let next_version = next.value() as i64;
        let repo_id = pull_request.repo_id().to_string();
        let author_id = pull_request.author_id().to_string();
        let source_branch = pull_request.source_branch().to_string();
        let target_branch = pull_request.target_branch().to_string();
        let state = pull_request.state().to_string();
        let merge_commit = pull_request.merge_commit().map(CommitHash::to_string);
        let rows = if expected == Version::NEW {
            client
                .execute(
                    "INSERT INTO pull_request (id, version, repo_id, author_id, title, \
                     description, source_branch, target_branch, state, merge_commit, opened_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
                     ON CONFLICT (id) DO NOTHING",
                    &[
                        &id,
                        &next_version,
                        &repo_id,
                        &author_id,
                        &pull_request.title(),
                        &pull_request.description(),
                        &source_branch,
                        &target_branch,
                        &state,
                        &merge_commit,
                        &pull_request.opened_at(),
                    ],
                )
                .await
                .map_err(save_error)?
        } else {
            client
                .execute(
                    "UPDATE pull_request SET version = $2, repo_id = $3, author_id = $4, \
                     title = $5, description = $6, source_branch = $7, target_branch = $8, \
                     state = $9, merge_commit = $10, opened_at = $11 \
                     WHERE id = $1 AND version = $12",
                    &[
                        &id,
                        &next_version,
                        &repo_id,
                        &author_id,
                        &pull_request.title(),
                        &pull_request.description(),
                        &source_branch,
                        &target_branch,
                        &state,
                        &merge_commit,
                        &pull_request.opened_at(),
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

    async fn get(&self, id: &PullRequestId) -> Result<Option<(PullRequest, Version)>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let row = client
            .query_opt(
                "SELECT version, repo_id, author_id, title, description, source_branch, \
                 target_branch, state, merge_commit, opened_at FROM pull_request WHERE id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(repo_error)?;
        match row {
            None => Ok(None),
            Some(row) => {
                let version: i64 = row.get(0);
                let repo_id: String = row.get(1);
                let author_id: String = row.get(2);
                let state: String = row.get(7);
                let pull_request = from_row(
                    *id,
                    &repo_id,
                    &author_id,
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                    &state,
                    row.get(8),
                    row.get(9),
                )?;
                Ok(Some((pull_request, Version::from_value(version as u64))))
            }
        }
    }

    async fn list(&self) -> Result<Vec<PullRequest>, RepoError> {
        let client = self.pool.get().await.map_err(repo_error)?;
        let rows = client
            .query(
                "SELECT id, repo_id, author_id, title, description, source_branch, \
                 target_branch, state, merge_commit, opened_at FROM pull_request",
                &[],
            )
            .await
            .map_err(repo_error)?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get(0);
                let id: PullRequestId = id.parse().map_err(repo_error)?;
                let repo_id: String = row.get(1);
                let author_id: String = row.get(2);
                let state: String = row.get(7);
                from_row(
                    id,
                    &repo_id,
                    &author_id,
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                    &state,
                    row.get(8),
                    row.get(9),
                )
            })
            .collect()
    }
}
