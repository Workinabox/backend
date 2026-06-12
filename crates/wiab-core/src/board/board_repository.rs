use crate::board::{Board, BoardId};

/// Port for persisting board aggregates. One repository per aggregate root.
pub trait BoardRepository: Send + Sync + 'static {
    fn save(&self, board: Board);
    fn get(&self, id: &BoardId) -> Option<Board>;
    fn list(&self) -> Vec<Board>;
}
