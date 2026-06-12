use crate::board::{BoardError, BoardId, BoardSnapshot};
use crate::project::ProjectId;

/// A board: a `B-###` id, the project it belongs to, a name, and a description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    id: BoardId,
    project_id: ProjectId,
    name: String,
    description: String,
}

impl Board {
    pub fn new(
        id: BoardId,
        project_id: ProjectId,
        name: String,
        description: String,
    ) -> Result<Self, BoardError> {
        if name.trim().is_empty() {
            return Err(BoardError::EmptyName);
        }
        Ok(Self {
            id,
            project_id,
            name,
            description,
        })
    }

    pub fn id(&self) -> BoardId {
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

    pub fn update(&mut self, name: String, description: String) -> Result<(), BoardError> {
        if name.trim().is_empty() {
            return Err(BoardError::EmptyName);
        }
        self.name = name;
        self.description = description;
        Ok(())
    }

    pub fn snapshot(&self) -> BoardSnapshot {
        BoardSnapshot {
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

    fn board(number: u64, name: &str) -> Board {
        Board::new(
            BoardId::from_number(number),
            ProjectId::from_number(1),
            name.to_owned(),
            String::new(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_empty_name() {
        let error = Board::new(
            BoardId::from_number(1),
            ProjectId::from_number(1),
            "  ".to_owned(),
            String::new(),
        )
        .unwrap_err();
        assert_eq!(error, BoardError::EmptyName);
    }

    #[test]
    fn exposes_getters() {
        let board = Board::new(
            BoardId::from_number(1),
            ProjectId::from_number(2),
            "Backlog".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        assert_eq!(board.id(), BoardId::from_number(1));
        assert_eq!(board.project_id(), ProjectId::from_number(2));
        assert_eq!(board.name(), "Backlog");
        assert_eq!(board.description(), "desc");
    }

    #[test]
    fn update_replaces_name_and_description_but_not_project() {
        let mut board = board(1, "Backlog");
        board
            .update("Sprint".to_owned(), "two weeks".to_owned())
            .unwrap();
        assert_eq!(board.name(), "Sprint");
        assert_eq!(board.description(), "two weeks");
        assert_eq!(board.project_id(), ProjectId::from_number(1));
    }

    #[test]
    fn update_rejects_empty_name() {
        let mut board = board(1, "Backlog");
        let error = board
            .update("  ".to_owned(), "two weeks".to_owned())
            .unwrap_err();
        assert_eq!(error, BoardError::EmptyName);
        assert_eq!(board.name(), "Backlog");
        assert_eq!(board.description(), "");
    }

    #[test]
    fn snapshot_mirrors_fields() {
        let board = Board::new(
            BoardId::from_number(1),
            ProjectId::from_number(2),
            "Backlog".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        let snapshot = board.snapshot();
        assert_eq!(snapshot.id, "B-1");
        assert_eq!(snapshot.project_id, "P-2");
        assert_eq!(snapshot.name, "Backlog");
        assert_eq!(snapshot.description, "desc");
    }
}
