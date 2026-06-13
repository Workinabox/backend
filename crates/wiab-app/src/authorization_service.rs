use anyhow::Result;
use wiab_core::access::{Operation, Role, RoleAssignmentRepository, Scope, effective_role};
use wiab_core::organization::OrganizationId;
use wiab_core::project::{ProjectId, ProjectRepository};
use wiab_core::repo::{RepoId, RepoRepository};
use wiab_core::user::{TokenScope, UserId};

/// Decides whether a user may perform an operation on a repo. Gathers the repo's
/// org/project chain and the user's grants, applies the core `effective_role` policy, and
/// caps the result by the presented token's scope (HTTPS only).
pub struct AuthorizationService<
    A: RoleAssignmentRepository,
    R: RepoRepository,
    P: ProjectRepository,
> {
    assignment_repository: A,
    repo_repository: R,
    project_repository: P,
}

impl<A: RoleAssignmentRepository, R: RepoRepository, P: ProjectRepository>
    AuthorizationService<A, R, P>
{
    pub fn new(assignment_repository: A, repo_repository: R, project_repository: P) -> Self {
        Self {
            assignment_repository,
            repo_repository,
            project_repository,
        }
    }

    async fn repo_chain(
        &self,
        repo: RepoId,
    ) -> Result<Option<(OrganizationId, ProjectId, RepoId)>> {
        let Some((repo, _)) = self.repo_repository.get(&repo).await? else {
            return Ok(None);
        };
        let Some((project, _)) = self.project_repository.get(&repo.project_id()).await? else {
            return Ok(None);
        };
        Ok(Some((
            project.organization_id(),
            repo.project_id(),
            repo.id(),
        )))
    }

    /// Whether `user` may perform `operation` at the organization level (e.g. creating a
    /// repo). Considers only Org-scoped grants for that org.
    pub async fn authorize_org(
        &self,
        user: UserId,
        org: OrganizationId,
        operation: Operation,
    ) -> Result<bool> {
        let role = self
            .assignment_repository
            .list()
            .await?
            .into_iter()
            .filter(|assignment| {
                assignment.user_id() == user && assignment.scope() == Scope::Org(org)
            })
            .map(|assignment| assignment.role())
            .max();
        Ok(role.is_some_and(|role| role.allows(operation)))
    }

    /// The user's effective role on the repo (ignoring any token scope), or `None`.
    pub async fn effective_role(&self, user: UserId, repo: RepoId) -> Result<Option<Role>> {
        let Some((org, project, repo)) = self.repo_chain(repo).await? else {
            return Ok(None);
        };
        let assignments = self.assignment_repository.list().await?;
        Ok(effective_role(&assignments, user, org, project, repo))
    }

    /// Whether `user` may perform `operation` on `repo`. `token_scope` is `Some` when the
    /// request authenticated with an access token (HTTPS) and `None` for SSH key auth.
    pub async fn authorize(
        &self,
        user: UserId,
        repo: RepoId,
        operation: Operation,
        token_scope: Option<&TokenScope>,
    ) -> Result<bool> {
        let Some((org, project, repo_id)) = self.repo_chain(repo).await? else {
            return Ok(false);
        };
        let assignments = self.assignment_repository.list().await?;
        let Some(base) = effective_role(&assignments, user, org, project, repo_id) else {
            return Ok(false);
        };
        let role = match token_scope {
            None => base,
            Some(scope) => {
                if !scope.allows_repo(repo_id) || !scope.allows_org(org) {
                    return Ok(false);
                }
                if scope.is_read_only() {
                    base.min(Role::Read)
                } else {
                    base
                }
            }
        };
        Ok(role.allows(operation))
    }
}
