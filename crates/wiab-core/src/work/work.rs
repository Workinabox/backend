use crate::project::ProjectId;
use crate::work::{Done, DoneId, WorkError, WorkId, WorkSnapshot};

/// A unit of work: a `W-###` id, the project it belongs to, a title, a description, a
/// list of `Done`s (acceptance criteria), and child works. `Work` is both the aggregate
/// root and a node in the composite tree — a leaf is simply a `Work` with no children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    id: WorkId,
    project_id: ProjectId,
    title: String,
    description: String,
    dones: Vec<Done>,
    children: Vec<Work>,
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
            children: Vec::new(),
        })
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

    pub fn children(&self) -> &[Work] {
        &self.children
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

    pub fn add_child(&mut self, child: Work) -> Result<(), WorkError> {
        if self.find(&child.id).is_some() {
            return Err(WorkError::DuplicateWorkId(child.id));
        }
        self.children.push(child);
        Ok(())
    }

    /// Recursively locate a work by id anywhere in this subtree (including `self`).
    pub fn find(&self, id: &WorkId) -> Option<&Work> {
        if self.id == *id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }

    /// Recursive mutable lookup — the entry point for mutating a nested sub-work.
    pub fn find_mut(&mut self, id: &WorkId) -> Option<&mut Work> {
        if self.id == *id {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.find_mut(id))
    }

    /// A work is done when all of its own dones are fulfilled and all of its children are
    /// done. A node with no dones and no children is vacuously done.
    pub fn is_done(&self) -> bool {
        self.dones.iter().all(Done::is_fulfilled) && self.children.iter().all(Work::is_done)
    }

    pub fn snapshot(&self) -> WorkSnapshot {
        WorkSnapshot {
            id: self.id.to_string(),
            project_id: self.project_id.to_string(),
            title: self.title.clone(),
            description: self.description.clone(),
            dones: self.dones.iter().map(Done::view).collect(),
            children: self.children.iter().map(Work::snapshot).collect(),
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
    fn parent_not_done_until_children_done() {
        let mut parent = work(1, "Epic");
        let parent_done = parent.add_done("review complete".to_owned()).unwrap();
        parent.fulfill_done(&parent_done).unwrap();

        let mut child = work(2, "Subtask");
        let child_done = child.add_done("implemented".to_owned()).unwrap();
        parent.add_child(child).unwrap();

        // Parent's own dones are fulfilled, but the child is not done yet.
        assert!(!parent.is_done());

        parent
            .find_mut(&WorkId::from_number(2))
            .unwrap()
            .fulfill_done(&child_done)
            .unwrap();
        assert!(parent.is_done());
    }

    #[test]
    fn find_locates_nested_work() {
        let mut parent = work(1, "Epic");
        let mut child = work(2, "Subtask");
        child.add_child(work(3, "Leaf")).unwrap();
        parent.add_child(child).unwrap();

        assert!(parent.find(&WorkId::from_number(3)).is_some());
        assert!(parent.find(&WorkId::from_number(9)).is_none());
    }

    #[test]
    fn add_child_rejects_duplicate_id() {
        let mut parent = work(1, "Epic");
        parent.add_child(work(2, "Subtask")).unwrap();
        let error = parent.add_child(work(2, "Clash")).unwrap_err();
        assert_eq!(error, WorkError::DuplicateWorkId(WorkId::from_number(2)));
    }

    #[test]
    fn empty_work_is_vacuously_done() {
        assert!(work(1, "Empty").is_done());
    }

    #[test]
    fn exposes_getters() {
        let mut root = Work::new(
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
        root.add_child(work(2, "Child")).unwrap();
        assert_eq!(root.children().len(), 1);
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
    fn find_mut_returns_none_for_missing() {
        let mut root = work(1, "Ship");
        assert!(root.find_mut(&WorkId::from_number(42)).is_none());
    }

    #[test]
    fn snapshot_mirrors_tree_with_completion() {
        let mut parent = work(1, "Epic");
        let parent_done = parent.add_done("review".to_owned()).unwrap();
        parent.add_child(work(2, "Subtask")).unwrap();

        let snapshot = parent.snapshot();
        assert_eq!(snapshot.id, "W-1");
        assert_eq!(snapshot.children.len(), 1);
        assert_eq!(snapshot.children[0].id, "W-2");
        assert!(snapshot.children[0].is_done); // empty child, vacuously done
        assert!(!snapshot.is_done); // parent's own done outstanding

        parent.fulfill_done(&parent_done).unwrap();
        assert!(parent.snapshot().is_done);
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
