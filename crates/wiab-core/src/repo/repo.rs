use crate::project::ProjectId;
use crate::repo::{RepoError, RepoId, RepoSnapshot};

/// A repo: an `R-###` id, the project it belongs to, a name, and a description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    id: RepoId,
    project_id: ProjectId,
    name: String,
    description: String,
}

impl Repo {
    pub fn new(
        id: RepoId,
        project_id: ProjectId,
        name: String,
        description: String,
    ) -> Result<Self, RepoError> {
        if name.trim().is_empty() {
            return Err(RepoError::EmptyName);
        }
        Ok(Self {
            id,
            project_id,
            name,
            description,
        })
    }

    pub fn id(&self) -> RepoId {
        self.id
    }

    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn update(&mut self, name: String, description: String) -> Result<(), RepoError> {
        if name.trim().is_empty() {
            return Err(RepoError::EmptyName);
        }
        self.name = name;
        self.description = description;
        Ok(())
    }

    pub fn snapshot(&self) -> RepoSnapshot {
        RepoSnapshot {
            id: self.id.to_string(),
            project_id: self.project_id.to_string(),
            name: self.name.clone(),
            description: self.description.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(number: u64, name: &str) -> Repo {
        Repo::new(
            RepoId::from_number(number),
            ProjectId::from_number(1),
            name.to_owned(),
            String::new(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_empty_name() {
        let error = Repo::new(
            RepoId::from_number(1),
            ProjectId::from_number(1),
            "  ".to_owned(),
            String::new(),
        )
        .unwrap_err();
        assert_eq!(error, RepoError::EmptyName);
    }

    #[test]
    fn exposes_getters() {
        let repo = Repo::new(
            RepoId::from_number(1),
            ProjectId::from_number(2),
            "backend".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        assert_eq!(repo.id(), RepoId::from_number(1));
        assert_eq!(repo.project_id(), ProjectId::from_number(2));
        assert_eq!(repo.name(), "backend");
        assert_eq!(repo.description(), "desc");
    }

    #[test]
    fn update_replaces_name_and_description_but_not_project() {
        let mut repo = repo(1, "backend");
        repo.update("frontend".to_owned(), "react app".to_owned())
            .unwrap();
        assert_eq!(repo.name(), "frontend");
        assert_eq!(repo.description(), "react app");
        assert_eq!(repo.project_id(), ProjectId::from_number(1));
    }

    #[test]
    fn update_rejects_empty_name() {
        let mut repo = repo(1, "backend");
        let error = repo
            .update("  ".to_owned(), "react app".to_owned())
            .unwrap_err();
        assert_eq!(error, RepoError::EmptyName);
        assert_eq!(repo.name(), "backend");
        assert_eq!(repo.description(), "");
    }

    #[test]
    fn snapshot_mirrors_fields() {
        let repo = Repo::new(
            RepoId::from_number(1),
            ProjectId::from_number(2),
            "backend".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        let snapshot = repo.snapshot();
        assert_eq!(snapshot.id, "R-1");
        assert_eq!(snapshot.project_id, "P-2");
        assert_eq!(snapshot.name, "backend");
        assert_eq!(snapshot.description, "desc");
    }
}
