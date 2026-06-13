use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::board::{Board, BoardId, BoardNumbering, BoardRepository, BoardSnapshot};
use wiab_core::project::{ProjectId, ProjectRepository};
use wiab_core::repository::{SaveError, Version};

use crate::board_requests::{CreateBoardRequest, UpdateBoardRequest};

/// Orchestrates use cases over the `Board` aggregate.
///
/// Holds the project repository to verify the parent project exists.
pub struct BoardApplicationService<B: BoardRepository, P: ProjectRepository> {
    board_repository: B,
    project_repository: P,
    numbering: Arc<dyn BoardNumbering>,
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
        }
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub async fn list_boards(
        &self,
        project_id: &str,
    ) -> anyhow::Result<Option<Vec<BoardSnapshot>>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let mut boards = self
            .board_repository
            .list()
            .await?
            .into_iter()
            .filter(|board| board.project_id() == id)
            .collect::<Vec<_>>();
        boards.sort_by_key(|board| board.id().number());
        Ok(Some(
            boards.into_iter().map(|board| board.snapshot()).collect(),
        ))
    }

    pub async fn board_snapshot(&self, board_id: &str) -> anyhow::Result<Option<BoardSnapshot>> {
        let id: BoardId = board_id.parse()?;
        Ok(self
            .board_repository
            .get(&id)
            .await?
            .map(|(board, _)| board.snapshot()))
    }

    /// Returns `Ok(None)` when no project with the given id exists.
    pub async fn create_board(
        &self,
        project_id: &str,
        request: CreateBoardRequest,
    ) -> anyhow::Result<Option<BoardSnapshot>> {
        let id: ProjectId = project_id.parse()?;
        if self.project_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let board = Board::new(self.numbering.next(), id, request.name, request.description)?;
        let snapshot = board.snapshot();
        self.board_repository.save(board, Version::NEW).await?;
        Ok(Some(snapshot))
    }

    /// Returns `Ok(None)` when no board with the given id exists.
    pub async fn update_board(
        &self,
        board_id: &str,
        request: UpdateBoardRequest,
    ) -> anyhow::Result<Option<BoardSnapshot>> {
        let id: BoardId = board_id.parse()?;
        loop {
            let Some((mut board, version)) = self.board_repository.get(&id).await? else {
                return Ok(None);
            };
            board.update(request.name.clone(), request.description.clone())?;
            let snapshot = board.snapshot();
            match self.board_repository.save(board, version).await {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::organization::OrganizationId;
    use wiab_core::project::Project;
    use wiab_core::repository::{RepoError, SaveError, Version};

    use super::*;

    #[derive(Default)]
    struct TestBoardRepository {
        boards: RwLock<HashMap<BoardId, (Board, u64)>>,
    }

    impl BoardRepository for TestBoardRepository {
        async fn save(&self, board: Board, expected: Version) -> Result<Version, SaveError> {
            let mut boards = self
                .boards
                .write()
                .expect("test repository write lock poisoned");
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
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(board, version)| (board.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Board>, RepoError> {
            Ok(self
                .boards
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(board, _)| board.clone())
                .collect())
        }
    }

    #[derive(Default)]
    struct TestProjectRepository {
        projects: RwLock<HashMap<ProjectId, (Project, u64)>>,
    }

    impl ProjectRepository for TestProjectRepository {
        async fn save(&self, project: Project, expected: Version) -> Result<Version, SaveError> {
            let mut projects = self
                .projects
                .write()
                .expect("test repository write lock poisoned");
            let current = projects
                .get(&project.id())
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            projects.insert(project.id(), (project, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &ProjectId) -> Result<Option<(Project, Version)>, RepoError> {
            Ok(self
                .projects
                .read()
                .expect("test repository read lock poisoned")
                .get(id)
                .map(|(project, version)| (project.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Project>, RepoError> {
            Ok(self
                .projects
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(project, _)| project.clone())
                .collect())
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

    async fn seed_project(
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
        service
            .project_repository
            .save(project, Version::NEW)
            .await
            .unwrap();
        id
    }

    async fn create(
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
            .await
            .expect("project id should be valid")
            .expect("project should exist")
    }

    #[tokio::test]
    async fn create_board_assigns_incrementing_ids() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        assert_eq!(create(&service, &project_id, "First").await.id, "B-1");
        assert_eq!(create(&service, &project_id, "Second").await.id, "B-2");
    }

    #[tokio::test]
    async fn create_board_records_project_id() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let board = create(&service, &project_id, "Backlog").await;
        assert_eq!(board.project_id, project_id);
    }

    #[tokio::test]
    async fn create_board_under_missing_project_returns_none() {
        let service = service();
        let result = service
            .create_board(
                "P-9",
                CreateBoardRequest {
                    name: "Backlog".to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn create_board_rejects_malformed_project_id() {
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
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn create_board_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        assert!(
            service
                .create_board(
                    &project_id,
                    CreateBoardRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn list_boards_partitions_by_project() {
        let service = service();
        let first_project = seed_project(&service, 1).await;
        let second_project = seed_project(&service, 2).await;
        create(&service, &first_project, "First").await;
        create(&service, &second_project, "Second").await;
        create(&service, &first_project, "Third").await;
        service
            .board_repository
            .save(
                Board::new(
                    BoardId::from_number(10),
                    ProjectId::from_number(1),
                    "Tenth".to_owned(),
                    String::new(),
                )
                .unwrap(),
                Version::NEW,
            )
            .await
            .unwrap();

        let first_ids = service
            .list_boards(&first_project)
            .await
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|board| board.id)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, vec!["B-1", "B-3", "B-10"]);

        let second_ids = service
            .list_boards(&second_project)
            .await
            .unwrap()
            .expect("project should exist")
            .into_iter()
            .map(|board| board.id)
            .collect::<Vec<_>>();
        assert_eq!(second_ids, vec!["B-2"]);
    }

    #[tokio::test]
    async fn list_boards_for_missing_project_returns_none() {
        let service = service();
        assert!(service.list_boards("P-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_boards_rejects_malformed_project_id() {
        let service = service();
        assert!(service.list_boards("bogus").await.is_err());
    }

    #[tokio::test]
    async fn board_snapshot_returns_none_for_missing() {
        let service = service();
        assert!(service.board_snapshot("B-9").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn board_snapshot_rejects_malformed_id() {
        let service = service();
        assert!(service.board_snapshot("bogus").await.is_err());
    }

    #[tokio::test]
    async fn update_board_replaces_fields_but_not_project() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let board = create(&service, &project_id, "Backlog").await;
        let updated = service
            .update_board(
                &board.id,
                UpdateBoardRequest {
                    name: "Sprint".to_owned(),
                    description: "two weeks".to_owned(),
                },
            )
            .await
            .unwrap()
            .expect("board should exist");
        assert_eq!(updated.name, "Sprint");
        assert_eq!(updated.description, "two weeks");
        assert_eq!(updated.project_id, project_id);

        let reloaded = service
            .board_snapshot(&board.id)
            .await
            .unwrap()
            .expect("board should exist");
        assert_eq!(reloaded.name, "Sprint");
    }

    #[tokio::test]
    async fn update_missing_board_returns_none() {
        let service = service();
        let result = service
            .update_board(
                "B-9",
                UpdateBoardRequest {
                    name: "Sprint".to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_board_rejects_empty_name() {
        let service = service();
        let project_id = seed_project(&service, 1).await;
        let board = create(&service, &project_id, "Backlog").await;
        assert!(
            service
                .update_board(
                    &board.id,
                    UpdateBoardRequest {
                        name: "  ".to_owned(),
                        description: String::new(),
                    },
                )
                .await
                .is_err()
        );
    }
}
