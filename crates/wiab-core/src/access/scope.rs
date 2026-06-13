use crate::access::AccessError;
use crate::organization::OrganizationId;
use crate::project::ProjectId;
use crate::repo::RepoId;

/// The resource a role assignment applies to. An Org-scoped grant covers all the org's
/// projects and repos; a Project-scoped grant covers that project's repos; a Repo-scoped
/// grant covers just that repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Org(OrganizationId),
    Project(ProjectId),
    Repo(RepoId),
}

impl Scope {
    /// Whether this scope covers the given repo, identified by its full org/project/repo
    /// chain.
    pub fn covers(&self, org: OrganizationId, project: ProjectId, repo: RepoId) -> bool {
        match self {
            Scope::Org(o) => *o == org,
            Scope::Project(p) => *p == project,
            Scope::Repo(r) => *r == repo,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Scope::Org(_) => "org",
            Scope::Project(_) => "project",
            Scope::Repo(_) => "repo",
        }
    }

    pub fn id_string(&self) -> String {
        match self {
            Scope::Org(o) => o.to_string(),
            Scope::Project(p) => p.to_string(),
            Scope::Repo(r) => r.to_string(),
        }
    }

    /// Builds a scope from a `(kind, id)` pair as it arrives over HTTP.
    pub fn parse(kind: &str, id: &str) -> Result<Self, AccessError> {
        match kind {
            "org" => id
                .parse::<OrganizationId>()
                .map(Scope::Org)
                .map_err(|_| AccessError::InvalidScope(format!("{kind}:{id}"))),
            "project" => id
                .parse::<ProjectId>()
                .map(Scope::Project)
                .map_err(|_| AccessError::InvalidScope(format!("{kind}:{id}"))),
            "repo" => id
                .parse::<RepoId>()
                .map(Scope::Repo)
                .map_err(|_| AccessError::InvalidScope(format!("{kind}:{id}"))),
            other => Err(AccessError::InvalidScope(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (OrganizationId, ProjectId, RepoId) {
        (
            OrganizationId::from_number(1),
            ProjectId::from_number(2),
            RepoId::from_number(3),
        )
    }

    #[test]
    fn org_scope_covers_its_repos() {
        let (org, project, repo) = ids();
        assert!(Scope::Org(org).covers(org, project, repo));
        assert!(!Scope::Org(OrganizationId::from_number(9)).covers(org, project, repo));
    }

    #[test]
    fn narrower_scopes_match_their_level() {
        let (org, project, repo) = ids();
        assert!(Scope::Project(project).covers(org, project, repo));
        assert!(!Scope::Project(ProjectId::from_number(9)).covers(org, project, repo));
        assert!(Scope::Repo(repo).covers(org, project, repo));
        assert!(!Scope::Repo(RepoId::from_number(9)).covers(org, project, repo));
    }

    #[test]
    fn parses_from_kind_and_id() {
        assert_eq!(
            Scope::parse("org", "O-1").unwrap(),
            Scope::Org(OrganizationId::from_number(1))
        );
        assert_eq!(
            Scope::parse("repo", "R-5").unwrap(),
            Scope::Repo(RepoId::from_number(5))
        );
        assert!(Scope::parse("org", "P-1").is_err());
        assert!(Scope::parse("bogus", "X-1").is_err());
    }
}
