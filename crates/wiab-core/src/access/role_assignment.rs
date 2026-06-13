use crate::access::{Role, RoleAssignmentId, RoleAssignmentSnapshot, Scope};
use crate::user::UserId;

/// A grant: a user holds a role at a scope. Its own aggregate so grants are listed,
/// granted, and revoked independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleAssignment {
    id: RoleAssignmentId,
    user_id: UserId,
    scope: Scope,
    role: Role,
}

impl RoleAssignment {
    pub fn new(id: RoleAssignmentId, user_id: UserId, scope: Scope, role: Role) -> Self {
        Self {
            id,
            user_id,
            scope,
            role,
        }
    }

    pub fn id(&self) -> RoleAssignmentId {
        self.id
    }

    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn snapshot(&self) -> RoleAssignmentSnapshot {
        RoleAssignmentSnapshot {
            id: self.id.to_string(),
            user_id: self.user_id.to_string(),
            scope_kind: self.scope.kind().to_owned(),
            scope_id: self.scope.id_string(),
            role: self.role.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organization::OrganizationId;

    #[test]
    fn exposes_fields_and_snapshot() {
        let assignment = RoleAssignment::new(
            RoleAssignmentId::from_number(3),
            UserId::from_number(7),
            Scope::Org(OrganizationId::from_number(1)),
            Role::Admin,
        );
        assert_eq!(assignment.id(), RoleAssignmentId::from_number(3));
        assert_eq!(assignment.user_id(), UserId::from_number(7));
        assert_eq!(
            assignment.scope(),
            Scope::Org(OrganizationId::from_number(1))
        );
        assert_eq!(assignment.role(), Role::Admin);

        let snapshot = assignment.snapshot();
        assert_eq!(snapshot.id, "G-3");
        assert_eq!(snapshot.user_id, "U-7");
        assert_eq!(snapshot.scope_kind, "org");
        assert_eq!(snapshot.scope_id, "O-1");
        assert_eq!(snapshot.role, "admin");
    }
}
