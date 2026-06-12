use std::sync::{Arc, Mutex};

use wiab_core::board::{Board, BoardId, BoardNumbering, BoardRepository, BoardSnapshot};
use wiab_core::project::{ProjectId, ProjectRepository};

use crate::board_requests::{CreateBoardRequest, UpdateBoardRequest};

/// Orchestrates use cases over the `Board` aggregate.
///
/// Methods are synchronous: `Board` has no external/async collaborators, so a plain
/// `std::sync::Mutex` guard held across each load-mutate-save is enough to prevent lost
/// updates. Holds the project repository to verify the parent project exists.
pub struct BoardApplicationService<B: BoardRepository, P: ProjectRepository> {
    board_repository: B,
    project_repository: P,
    numbering: Arc<dyn BoardNumbering>,
    mutation_guard: Mutex<()>,
}

impl<B: BoardRepository, P: ProjectRepository> BoardApplicationService<B, P> {
    pub fn new(
        board_repository: B,
        project_repository: P,
        numbering: Arc<dyn BoardNumbering>,
    ) -> Self {
        Self {
            board_repository,
            project_repository,
            numbering,
            mutation_guard: Mutex::new(()),
        }
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub fn list_boards(&self, project_id: &str) -> anyhow::Result<Option<Vec<BoardSnapshot>>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).is_none() {
            return Ok(None);
        }
        let mut boards = self
            .board_repository
            .list()
            .into_iter()
            .filter(|board| board.project_id() == id)
            .collect::<Vec<_>>();
        boards.sort_by_key(|board| board.id().number());
        Ok(Some(
            boards.into_iter().map(|board| board.snapshot()).collect(),
        ))
    }

    pub fn board_snapshot(&self, board_id: &str) -> anyhow::Result<Option<BoardSnapshot>> {
        let id: BoardId = board_id.parse()?;
        Ok(self.board_repository.get(&id).map(|board| board.snapshot()))
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub fn create_board(
        &self,
        project_id: &str,
        request: CreateBoardRequest,
    ) -> anyhow::Result<Option<BoardSnapshot>> {
        let _guard = self.lock();
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).is_none() {
            return Ok(None);
        }
        let board = Board::new(self.numbering.next(), id, request.name, request.description)?;
        let snapshot = board.snapshot();
        self.board_repository.save(board);
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no board with the given id exists.
    pub fn update_board(
        &self,
        board_id: &str,
        request: UpdateBoardRequest,
    ) -> anyhow::Result<Option<BoardSnapshot>> {
        let _guard = self.lock();
        let id: BoardId = board_id.parse()?;
        let Some(mut board) = self.board_repository.get(&id) else {
            return Ok(None);
        };
        board.update(request.name, request.description)?;
        let snapshot = board.snapshot();
        self.board_repository.save(board);
        Ok(Some(snapshot))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_guard
            .lock()
            .expect("board mutation guard poisoned")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::organization::OrganizationId;
    use wiab_core::project::Project;

    use super::*;

    #[derive(Default)]
    struct TestBoardRepository {
        boards: RwLock<HashMap<BoardId, Board>>,
    }

    impl BoardRepository for TestBoardRepository {
        fn save(&self, board: Board) {
            self.boards
                .write()
                .expect("test repository write lock poisoned")
                .insert(board.id(), board);
        }

        fn get(&self, id: &BoardId) -> Option<Board> {
            self.boards
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .cloned()
        }

        fn list(&self) -> Vec<Board> {
            self.boards
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .cloned()
                .collect()
        }
    }

    #[derive(Default)]
    struct TestProjectRepository {
        projects: RwLock<HashMap<ProjectId, Project>>,
    }

    impl ProjectRepository for TestProjectRepository {
        fn save(&self, project: Project) {
            self.projects
                .write()
                .expect("test repository write lock poisoned")
                .insert(project.id(), project);
        }

        fn get(&self, id: &ProjectId) -> Option<Project> {
            self.projects
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .cloned()
        }

        fn list(&self) -> Vec<Project> {
            self.projects
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .cloned()
                .collect()
        }
    }

    #[derive(Default)]
    struct TestBoardNumbering {
        counter: AtomicU64,
    }

    impl BoardNumbering for TestBoardNumbering {
        fn next(&self) -> BoardId {
            BoardId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    fn service() -> BoardApplicationService<TestBoardRepository, TestProjectRepository> {
        BoardApplicationService::new(
            TestBoardRepository::default(),
            TestProjectRepository::default(),
            Arc::new(TestBoardNumbering::default()),
        )
    }

    fn seed_project(
        service: &BoardApplicationService<TestBoardRepository, TestProjectRepository>,
        number: u64,
    ) -> String {
        let project = Project::new(
            ProjectId::from_number(number),
            OrganizationId::from_number(1),
            format!("Project {number}"),
            String::new(),
        )
        .unwrap();
        let id = project.id().to_string();
        service.project_repository.save(project);
        id
    }

    fn create(
        service: &BoardApplicationService<TestBoardRepository, TestProjectRepository>,
        project_id: &str,
        name: &str,
    ) -> BoardSnapshot {
        service
            .create_board(
                project_id,
                CreateBoardRequest {
                    name: name.to_owned(),
                    description: String::new(),
                },
            )
            .expect("project id should be valid")
            .expect("project should exist")
    }

    #[test]
    fn create_board_assigns_incrementing_ids() {
        let service = service();
        let project_id = seed_project(&service, 1);
        assert_eq!(create(&service, &project_id, "First").id, "B-1");
        assert_eq!(create(&service, &project_id, "Second").id, "B-2");
    }

    #[test]
    fn create_board_records_project_id() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let board = create(&service, &project_id, "Backlog");
        assert_eq!(board.project_id, project_id);
    }

    #[test]
    fn create_board_under_missing_project_returns_none() {
        let service = service();
        let result = service
            .create_board(
                "P-9",
                CreateBoardRequest {
                    name: "Backlog".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn create_board_rejects_malformed_project_id() {
        let service = service();
        assert!(
            service
                .create_board(
                    "bogus",
                    CreateBoardRequest {
                        name: "Backlog".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn create_board_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1);
        assert!(
            service
                .create_board(
                    &project_id,
                    CreateBoardRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn list_boards_partitions_by_project() {
        let service = service();
        let first_project = seed_project(&service, 1);
        let second_project = seed_project(&service, 2);
        create(&service, &first_project, "First");
        create(&service, &second_project, "Second");
        create(&service, &first_project, "Third");
        service.board_repository.save(
            Board::new(
                BoardId::from_number(10),
                ProjectId::from_number(1),
                "Tenth".to_owned(),
                String::new(),
            )
            .unwrap(),
        );

        let first_ids = service
            .list_boards(&first_project)
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|board| board.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["B-1", "B-3", "B-10"]);

        let second_ids = service
            .list_boards(&second_project)
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|board| board.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["B-2"]);
    }

    #[test]
    fn list_boards_for_missing_project_returns_none() {
        let service = service();
        assert!(service.list_boards("P-9").unwrap().is_none());
    }

    #[test]
    fn list_boards_rejects_malformed_project_id() {
        let service = service();
        assert!(service.list_boards("bogus").is_err());
    }

    #[test]
    fn board_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.board_snapshot("B-9").unwrap().is_none());
    }

    #[test]
    fn board_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.board_snapshot("bogus").is_err());
    }

    #[test]
    fn update_board_replaces_fields_but_not_project() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let board = create(&service, &project_id, "Backlog");
        let updated = service
            .update_board(
                &board.id,
                UpdateBoardRequest {
                    name: "Sprint".to_owned(),
                    description: "two weeks".to_owned(),
                },
            )
            .unwrap()
            .expect("board should exist");
        assert_eq!(updated.name, "Sprint");
        assert_eq!(updated.description, "two weeks");
        assert_eq!(updated.project_id, project_id);

        let reloaded = service
            .board_snapshot(&board.id)
            .unwrap()
            .expect("board should exist");
        assert_eq!(reloaded.name, "Sprint");
    }

    #[test]
    fn update_missing_board_returns_none() {
        let service = service();
        let result = service
            .update_board(
                "B-9",
                UpdateBoardRequest {
                    name: "Sprint".to_owned(),
                    description: String::new(),
                },
            )
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_board_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1);
        let board = create(&service, &project_id, "Backlog");
        assert!(
            service
                .update_board(
                    &board.id,
                    UpdateBoardRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .is_err()
        );
    }
}
