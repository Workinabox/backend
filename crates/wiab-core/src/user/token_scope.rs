use crate::organization::OrganizationId;
use crate::repo::RepoId;

/// What a token is allowed to do, narrowing the user's own permissions.
///
/// `None` on a list means "no restriction on that axis". Effective permission is the
/// user's role capped by this scope — computed by the authorization service, which calls
/// the accessors here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenScope {
    read_only: bool,
    repos: Option<Vec<RepoId>>,
    orgs: Option<Vec<OrganizationId>>,
}

impl TokenScope {
    pub fn new(
        read_only: bool,
        repos: Option<Vec<RepoId>>,
        orgs: Option<Vec<OrganizationId>>,
    ) -> Self {
        Self {
            read_only,
            repos,
            orgs,
        }
    }

    /// An unrestricted scope — the token carries the user's full permissions.
    pub fn unrestricted() -> Self {
        Self {
            read_only: false,
            repos: None,
            orgs: None,
        }
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn allows_repo(&self, repo: RepoId) -> bool {
        self.repos
            .as_ref()
            .is_none_or(|repos| repos.contains(&repo))
    }

    pub fn allows_org(&self, org: OrganizationId) -> bool {
        self.orgs.as_ref().is_none_or(|orgs| orgs.contains(&org))
    }

    pub fn repos(&self) -> Option<&[RepoId]> {
        self.repos.as_deref()
    }

    pub fn orgs(&self) -> Option<&[OrganizationId]> {
        self.orgs.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrestricted_allows_everything() {
        let scope = TokenScope::unrestricted();
        assert!(!scope.is_read_only());
        assert!(scope.allows_repo(RepoId::from_number(5)));
        assert!(scope.allows_org(OrganizationId::from_number(2)));
    }

    #[test]
    fn restricted_lists_gate_membership() {
        let scope = TokenScope::new(
            true,
            Some(vec![RepoId::from_number(1)]),
            Some(vec![OrganizationId::from_number(1)]),
        );
        assert!(scope.is_read_only());
        assert!(scope.allows_repo(RepoId::from_number(1)));
        assert!(!scope.allows_repo(RepoId::from_number(2)));
        assert!(scope.allows_org(OrganizationId::from_number(1)));
        assert!(!scope.allows_org(OrganizationId::from_number(9)));
    }
}
