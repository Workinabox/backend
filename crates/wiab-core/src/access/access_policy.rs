use crate::access::{Role, RoleAssignment};
use crate::organization::OrganizationId;
use crate::project::ProjectId;
use crate::repo::RepoId;
use crate::user::UserId;

/// The user's effective role on a repo: the highest role among the user's assignments
/// whose scope covers that repo (its repo, its project, or its org). `None` = no access.
///
/// This is the single source of truth for "what can this user do here"; the application
/// layer gathers the data (the user's grants and the repo's org/project chain) and calls it.
pub fn effective_role(
    assignments: &[RoleAssignment],
    user: UserId,
    org: OrganizationId,
    project: ProjectId,
    repo: RepoId,
) -> Option<Role> {
    assignments
        .iter()
        .filter(|assignment| {
            assignment.user_id() == user && assignment.scope().covers(org, project, repo)
        })
        .map(|assignment| assignment.role())
        .max()
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
