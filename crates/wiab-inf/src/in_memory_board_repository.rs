use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::board::{Board, BoardId, BoardRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryBoardRepository {
    boards: Arc<RwLock<HashMap<BoardId, Board>>>,
}

impl InMemoryBoardRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BoardRepository for InMemoryBoardRepository {
    fn save(&self, board: Board) {
        self.boards
            .write()
            .expect("board repository write lock poisoned")
            .insert(board.id(), board);
    }

    fn get(&self, id: &BoardId) -> Option<Board> {
        self.boards
            .read()
            .expect("board repository read lock poisoned")
            .get(id)
            .cloned()
    }

    fn list(&self) -> Vec<Board> {
        self.boards
            .read()
            .expect("board repository read lock poisoned")
            .values()
            .cloned()
            .collect()
    }
}
