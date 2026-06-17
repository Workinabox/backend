use crate::rbac::ResourceRef;

/// Strategy a product implements to express how a *granted* resource confers authority
/// over a *target*.
///
/// The [`effective_role`](crate::rbac::effective_role) policy is hierarchy-agnostic: it
/// asks this trait whether each of a principal's grants covers the resource being checked.
/// WIAB, for example, implements an `Org ⊇ Project ⊇ Repo` containment; another product
/// supplies its own. `target_chain` is the resource being checked plus its enclosing
/// scopes, broadest-or-narrowest as the product chooses (WIAB passes `[org, project,
/// repo]`).
pub trait ResourceHierarchy: Send + Sync {
    fn covers(&self, granted: &ResourceRef, target_chain: &[ResourceRef]) -> bool;
}
