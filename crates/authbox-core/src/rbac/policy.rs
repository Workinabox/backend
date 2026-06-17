use crate::rbac::{Grant, ResourceHierarchy, ResourceRef, Role};

/// The principal's effective role on a target: the highest role among the principal's
/// grants whose resource covers `target_chain`, per the supplied `hierarchy`. `None` means
/// no access.
///
/// This is the single source of truth for "what can this principal do here"; the caller
/// gathers the data (the principal's grants and the target's scope chain) and supplies the
/// product's containment rule.
pub fn effective_role(
    grants: &[Grant],
    principal: &str,
    target_chain: &[ResourceRef],
    hierarchy: &dyn ResourceHierarchy,
) -> Option<Role> {
    grants
        .iter()
        .filter(|grant| {
            grant.principal() == principal && hierarchy.covers(grant.resource(), target_chain)
        })
        .map(|grant| grant.role())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial hierarchy where a grant covers the target only if it is one of the
    /// resources in the chain (exact match by `(kind, id)`). This mirrors how a layered
    /// `[broad, …, narrow]` chain behaves when each level has a distinct kind.
    struct ChainMatch;

    impl ResourceHierarchy for ChainMatch {
        fn covers(&self, granted: &ResourceRef, target_chain: &[ResourceRef]) -> bool {
            target_chain.iter().any(|target| target == granted)
        }
    }

    fn grant(principal: &str, kind: &str, id: &str, role: Role) -> Grant {
        Grant::new(principal, ResourceRef::new(kind, id), role)
    }

    #[test]
    fn takes_the_highest_covering_role() {
        let chain = [
            ResourceRef::new("org", "O-1"),
            ResourceRef::new("project", "P-2"),
            ResourceRef::new("repo", "R-3"),
        ];
        let grants = vec![
            grant("U-1", "org", "O-1", Role::Read),
            grant("U-1", "repo", "R-3", Role::Write),
            // belongs to a different principal — ignored
            grant("U-2", "org", "O-1", Role::Owner),
        ];
        assert_eq!(
            effective_role(&grants, "U-1", &chain, &ChainMatch),
            Some(Role::Write)
        );
    }

    #[test]
    fn no_covering_grant_means_no_access() {
        let chain = [ResourceRef::new("org", "O-1")];
        let grants = vec![grant("U-1", "org", "O-9", Role::Owner)];
        assert_eq!(effective_role(&grants, "U-1", &chain, &ChainMatch), None);
    }
}
