use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::board::{Board, BoardId, BoardRepository};
use wiab_core::repository::{RepoError, SaveError, Version};

#[derive(Debug, Clone, Default)]
pub struct InMemoryBoardRepository {
    boards: Arc<RwLock<HashMap<BoardId, (Board, u64)>>>,
}

impl InMemoryBoardRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BoardRepository for InMemoryBoardRepository {
    async fn save(&self, board: Board, expected: Version) -> Result<Version, SaveError> {
        let mut boards = self
            .boards
            .write()
            .expect("board repository write lock poisoned");
        let current = boards
            .get(&board.id())
            .map(|(_, version)| *version)
            .unwrap_or(0);
        if current != expected.value() {
            return Err(SaveError::Conflict);
        }
        let next = expected.next();
        boards.insert(board.id(), (board, next.value()));
        Ok(next)
    }

    async fn get(&self, id: &BoardId) -> Result<Option<(Board, Version)>, RepoError> {
        Ok(self
            .boards
            .read()
            .expect("board repository read lock poisoned")
            .get(id)
            .map(|(board, version)| (board.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Board>, RepoError> {
        Ok(self
            .boards
            .read()
            .expect("board repository read lock poisoned")
            .values()
            .map(|(board, _)| board.clone())
            .collect())
    }
}
