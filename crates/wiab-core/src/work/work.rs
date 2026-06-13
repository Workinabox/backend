use crate::project::ProjectId;
use crate::work::{Done, DoneId, WorkError, WorkId, WorkSnapshot};

/// A unit of work: a `W-###` id, the project it belongs to, a title, a description, and a
/// list of `Done`s (acceptance criteria). `Work` is an aggregate root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    id: WorkId,
    project_id: ProjectId,
    title: String,
    description: String,
    dones: Vec<Done>,
}

impl Work {
    pub fn new(
        id: WorkId,
        project_id: ProjectId,
        title: String,
        description: String,
    ) -> Result<Self, WorkError> {
        if title.trim().is_empty() {
            return Err(WorkError::EmptyTitle);
        }
        Ok(Self {
            id,
            project_id,
            title,
            description,
            dones: Vec::new(),
        })
    }

    /// Rebuild a `Work` from persisted state, including its `dones` (used by repository
    /// implementations). Bypasses the invariants enforced by `new`/`add_done`.
    pub fn from_persistence(
        id: WorkId,
        project_id: ProjectId,
        title: String,
        description: String,
        dones: Vec<Done>,
    ) -> Self {
        Self {
            id,
            project_id,
            title,
            description,
            dones,
        }
    }

    pub fn id(&self) -> WorkId {
        self.id
    }

    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn dones(&self) -> &[Done] {
        &self.dones
    }

    pub fn update(&mut self, title: String, description: String) -> Result<(), WorkError> {
        if title.trim().is_empty() {
            return Err(WorkError::EmptyTitle);
        }
        self.title = title;
        self.description = description;
        Ok(())
    }

    pub fn add_done(&mut self, criterion: String) -> Result<DoneId, WorkError> {
        let done = Done::new(criterion)?;
        let id = done.id();
        self.dones.push(done);
        Ok(id)
    }

    pub fn fulfill_done(&mut self, done_id: &DoneId) -> Result<(), WorkError> {
        self.done_mut(done_id)?.fulfill();
        Ok(())
    }

    pub fn unfulfill_done(&mut self, done_id: &DoneId) -> Result<(), WorkError> {
        self.done_mut(done_id)?.unfulfill();
        Ok(())
    }

    /// A work is done when all of its own dones are fulfilled. A work with no dones is
    /// vacuously done.
    pub fn is_done(&self) -> bool {
        self.dones.iter().all(Done::is_fulfilled)
    }

    pub fn snapshot(&self) -> WorkSnapshot {
        WorkSnapshot {
            id: self.id.to_string(),
            project_id: self.project_id.to_string(),
            title: self.title.clone(),
            description: self.description.clone(),
            dones: self.dones.iter().map(Done::view).collect(),
            is_done: self.is_done(),
        }
    }

    fn done_mut(&mut self, done_id: &DoneId) -> Result<&mut Done, WorkError> {
        self.dones
            .iter_mut()
            .find(|done| done.id() == *done_id)
            .ok_or(WorkError::DoneNotFound(*done_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(number: u64, title: &str) -> Work {
        Work::new(
            WorkId::from_number(number),
            ProjectId::from_number(1),
            title.to_owned(),
            String::new(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_empty_title() {
        let error = Work::new(
            WorkId::from_number(1),
            ProjectId::from_number(1),
            "  ".to_owned(),
            String::new(),
        )
        .unwrap_err();
        assert_eq!(error, WorkError::EmptyTitle);
    }

    #[test]
    fn update_replaces_title_and_description() {
        let mut work = work(1, "Ship v1");
        work.update("Ship v2".to_owned(), "the sequel".to_owned())
            .unwrap();
        assert_eq!(work.title(), "Ship v2");
        assert_eq!(work.description(), "the sequel");
    }

    #[test]
    fn update_rejects_empty_title() {
        let mut work = work(1, "Ship v1");
        let error = work
            .update("  ".to_owned(), "the sequel".to_owned())
            .unwrap_err();
        assert_eq!(error, WorkError::EmptyTitle);
        assert_eq!(work.title(), "Ship v1");
        assert_eq!(work.description(), "");
    }

    #[test]
    fn added_done_appears_in_dones() {
        let mut work = work(1, "Ship v1");
        let done_id = work.add_done("tests pass".to_owned()).unwrap();
        assert_eq!(work.dones().len(), 1);
        assert_eq!(work.dones()[0].id(), done_id);
    }

    #[test]
    fn fulfilling_unknown_done_errors() {
        let mut work = work(1, "Ship v1");
        let missing = DoneId::new();
        assert_eq!(
            work.fulfill_done(&missing).unwrap_err(),
            WorkError::DoneNotFound(missing)
        );
    }

    #[test]
    fn leaf_is_done_only_when_all_dones_fulfilled() {
        let mut work = work(1, "Ship v1");
        let first = work.add_done("tests pass".to_owned()).unwrap();
        let second = work.add_done("docs written".to_owned()).unwrap();
        assert!(!work.is_done());
        work.fulfill_done(&first).unwrap();
        assert!(!work.is_done());
        work.fulfill_done(&second).unwrap();
        assert!(work.is_done());
    }

    #[test]
    fn empty_work_is_vacuously_done() {
        assert!(work(1, "Empty").is_done());
    }

    #[test]
    fn exposes_getters() {
        let root = Work::new(
            WorkId::from_number(1),
            ProjectId::from_number(2),
            "Epic".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        assert_eq!(root.id(), WorkId::from_number(1));
        assert_eq!(root.project_id(), ProjectId::from_number(2));
        assert_eq!(root.title(), "Epic");
        assert_eq!(root.description(), "desc");
        assert!(root.dones().is_empty());
    }

    #[test]
    fn unfulfill_done_reverts_completion() {
        let mut root = work(1, "Ship");
        let done = root.add_done("tests pass".to_owned()).unwrap();
        root.fulfill_done(&done).unwrap();
        assert!(root.is_done());
        root.unfulfill_done(&done).unwrap();
        assert!(!root.is_done());
    }

    #[test]
    fn snapshot_includes_project_id() {
        let work = Work::new(
            WorkId::from_number(1),
            ProjectId::from_number(7),
            "Ship".to_owned(),
            String::new(),
        )
        .unwrap();
        assert_eq!(work.snapshot().project_id, "P-7");
    }
}
