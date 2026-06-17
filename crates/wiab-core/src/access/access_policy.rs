use authbox_core::rbac::{
    Grant, ResourceHierarchy, ResourceRef, effective_role as core_effective_role,
};

use crate::access::{Role, RoleAssignment};
use crate::organization::OrganizationId;
use crate::project::ProjectId;
use crate::repo::RepoId;
use crate::user::UserId;

/// WIAB's resource containment, supplied to the generic RBAC policy: an Org-scoped grant
/// covers the org's projects and repos, a Project-scoped grant covers that project's repos,
/// and a Repo-scoped grant covers just that repo. Expressed against the `[org, project,
/// repo]` chain a check builds: a grant covers the target iff its `(kind, id)` matches the
/// chain entry at its level (each level has a distinct kind).
struct WiabHierarchy;

impl ResourceHierarchy for WiabHierarchy {
    fn covers(&self, granted: &ResourceRef, target_chain: &[ResourceRef]) -> bool {
        target_chain.iter().any(|target| target == granted)
    }
}

/// The user's effective role on a repo: the highest role among the user's assignments
/// whose scope covers that repo (its repo, its project, or its org). `None` = no access.
///
/// This is the single source of truth for "what can this user do here"; the application
/// layer gathers the data (the user's grants and the repo's org/project chain) and calls it.
/// The actual policy is the product-neutral [`authbox_core::rbac::effective_role`]; this
/// adapter maps WIAB's typed `RoleAssignment`/`Scope` onto its generic inputs and supplies
/// [`WiabHierarchy`].
pub fn effective_role(
    assignments: &[RoleAssignment],
    user: UserId,
    org: OrganizationId,
    project: ProjectId,
    repo: RepoId,
) -> Option<Role> {
    let grants: Vec<Grant> = assignments
        .iter()
        .map(|assignment| {
            Grant::new(
                assignment.user_id().to_string(),
                assignment.scope().into(),
                assignment.role(),
            )
        })
        .collect();
    let target_chain = [
        ResourceRef::new("org", org.to_string()),
        ResourceRef::new("project", project.to_string()),
        ResourceRef::new("repo", repo.to_string()),
    ];
    core_effective_role(&grants, &user.to_string(), &target_chain, &WiabHierarchy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{RoleAssignmentId, Scope};

    fn assignment(number: u64, user: u64, scope: Scope, role: Role) -> RoleAssignment {
        RoleAssignment::new(
            RoleAssignmentId::from_number(number),
            UserId::from_number(user),
            scope,
            role,
        )
    }

    #[test]
    fn takes_the_highest_covering_role() {
        let org = OrganizationId::from_number(1);
        let project = ProjectId::from_number(2);
        let repo = RepoId::from_number(3);
        let user = UserId::from_number(1);

        let assignments = vec![
            assignment(1, 1, Scope::Org(org), Role::Read),
            assignment(2, 1, Scope::Repo(repo), Role::Write),
            // belongs to a different user — ignored
            assignment(3, 2, Scope::Org(org), Role::Owner),
        ];
        assert_eq!(
            effective_role(&assignments, user, org, project, repo),
            Some(Role::Write)
        );
    }

    #[test]
    fn no_covering_assignment_means_no_access() {
        let user = UserId::from_number(1);
        let assignments = vec![assignment(
            1,
            1,
            Scope::Org(OrganizationId::from_number(9)),
            Role::Owner,
        )];
        assert_eq!(
            effective_role(
                &assignments,
                user,
                OrganizationId::from_number(1),
                ProjectId::from_number(2),
                RepoId::from_number(3),
            ),
            None
        );
    }
}
