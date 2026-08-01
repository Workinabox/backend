#[allow(clippy::module_inception)]
mod team;
mod team_error;
mod team_id;
mod team_numbering;
mod team_repository;
mod team_snapshot;
mod team_state;

pub use team::Team;
pub use team_error::TeamError;
pub use team_id::TeamId;
pub use team_numbering::TeamNumbering;
pub use team_repository::TeamRepository;
pub use team_snapshot::TeamSnapshot;
pub use team_state::TeamState;
