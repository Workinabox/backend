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

    fn repo_chain(&self, repo: RepoId) -> Option<(OrganizationId, ProjectId, RepoId)> {
        let repo = self.repo_repository.get(&repo)?;
        let project = self.project_repository.get(&repo.project_id())?;
        Some((project.organization_id(), repo.project_id(), repo.id()))
    }

    /// Whether `user` may perform `operation` at the organization level (e.g. creating a
    /// repo). Considers only Org-scoped grants for that org.
    pub fn authorize_org(&self, user: UserId, org: OrganizationId, operation: Operation) -> bool {
        let role = self
            .assignment_repository
            .list()
            .into_iter()
            .filter(|assignment| {
                assignment.user_id() == user && assignment.scope() == Scope::Org(org)
            })
            .map(|assignment| assignment.role())
            .max();
        role.is_some_and(|role| role.allows(operation))
    }

    /// The user's effective role on the repo (ignoring any token scope), or `None`.
    pub fn effective_role(&self, user: UserId, repo: RepoId) -> Option<Role> {
        let (org, project, repo) = self.repo_chain(repo)?;
        let assignments = self.assignment_repository.list();
        effective_role(&assignments, user, org, project, repo)
    }

    /// Whether `user` may perform `operation` on `repo`. `token_scope` is `Some` when the
    /// request authenticated with an access token (HTTPS) and `None` for SSH key auth.
    pub fn authorize(
        &self,
        user: UserId,
        repo: RepoId,
        operation: Operation,
        token_scope: Option<&TokenScope>,
    ) -> bool {
        let Some((org, project, repo_id)) = self.repo_chain(repo) else {
            return false;
        };
        let assignments = self.assignment_repository.list();
        let Some(base) = effective_role(&assignments, user, org, project, repo_id) else {
            return false;
        };
        let role = match token_scope {
            None => base,
            Some(scope) => {
                if !scope.allows_repo(repo_id) || !scope.allows_org(org) {
                    return false;
                }
                if scope.is_read_only() {
                    base.min(Role::Read)
                } else {
                    base
                }
            }
        };
        role.allows(operation)
    }
}
