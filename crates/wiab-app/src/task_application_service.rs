use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::board::{BoardId, BoardRepository};
use wiab_core::repository::{SaveError, Version};
use wiab_core::task::{
    Task, TaskError, TaskId, TaskNumbering, TaskRepository, TaskSnapshot, TaskState,
};
use wiab_core::team::TeamId;
use wiab_core::work::{WorkId, WorkRepository};

use crate::task_requests::CreateTaskRequest;

/// Orchestrates use cases over the `Task` aggregate: queueing work on a board, the pull that
/// hands a task to a team, and the transitions a team drives as it works.
///
/// Holds the board and work repositories to verify both ends of a new task exist — a task
/// pointing at a work that was never written would be picked up and then fail on the team.
///
/// Mutations use optimistic concurrency: load with version, apply, retry on conflict. The one
/// exception is `claim_next`, which the repository performs atomically — see
/// [`TaskRepository::claim_next`] for why a retry loop is the wrong shape there.
pub struct TaskApplicationService<T: TaskRepository, B: BoardRepository, W: WorkRepository> {
    task_repository: T,
    board_repository: B,
    work_repository: W,
    numbering: Arc<dyn TaskNumbering>,
}

impl<T: TaskRepository, B: BoardRepository, W: WorkRepository> TaskApplicationService<T, B, W> {
    pub fn new(
        task_repository: T,
        board_repository: B,
        work_repository: W,
        numbering: Arc<dyn TaskNumbering>,
    ) -> Self {
        Self {
            task_repository,
            board_repository,
            work_repository,
            numbering,
        }
    }

    /// Returns `Ok(None)` when no board with the given id exists.
    pub async fn list_tasks(&self, board_id: &str) -> anyhow::Result<Option<Vec<TaskSnapshot>>> {
        let id: BoardId = board_id.parse()?;
        if self.board_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let mut tasks = self
            .task_repository
            .list()
            .await?
            .into_iter()
            .filter(|task| task.board_id() == id)
            .collect::<Vec<_>>();
        tasks.sort_by_key(|task| task.id().number());
        Ok(Some(tasks.iter().map(Task::snapshot).collect()))
    }

    pub async fn task_snapshot(&self, task_id: &str) -> anyhow::Result<Option<TaskSnapshot>> {
        let id: TaskId = task_id.parse()?;
        Ok(self
            .task_repository
            .get(&id)
            .await?
            .map(|(task, _)| task.snapshot()))
    }

    /// Queue a work item on a board. `Ok(None)` when the board or the work is unknown.
    pub async fn create_task(
        &self,
        board_id: &str,
        request: CreateTaskRequest,
    ) -> anyhow::Result<Option<TaskSnapshot>> {
        let board_id: BoardId = board_id.parse()?;
        let work_id: WorkId = request.work_id.parse()?;
        if self.board_repository.get(&board_id).await?.is_none()
            || self.work_repository.get(&work_id).await?.is_none()
        {
            return Ok(None);
        }
        let task = Task::new(self.numbering.next(), board_id, work_id);
        let snapshot = task.snapshot();
        self.task_repository.save(task, Version::NEW).await?;
        Ok(Some(snapshot))
    }

    /// Take the next task off a board for a team. `Ok(None)` when the board is unknown or has
    /// nothing waiting — the caller cannot tell those apart, and does not need to: a team
    /// polling an empty board and a team polling a board that vanished both simply wait.
    pub async fn claim_next_task(
        &self,
        board_id: &str,
        team_id: &str,
    ) -> anyhow::Result<Option<TaskSnapshot>> {
        let board_id: BoardId = board_id.parse()?;
        let team_id: TeamId = team_id.parse()?;
        Ok(self
            .task_repository
            .claim_next(&board_id, team_id)
            .await?
            .map(|(task, _)| task.snapshot()))
    }

    /// The task a team is currently holding, if any.
    ///
    /// A team that was stopped mid-issue still owns its task. On restart it asks this so it
    /// can carry on rather than claim something new and strand the old one in progress.
    ///
    /// `Assigned` counts as held: the team took the task and may not have started it yet.
    /// `Blocked` counts too — the team still owns it. Escalated and settled tasks do not.
    pub async fn held_task(&self, team_id: &str) -> anyhow::Result<Option<TaskSnapshot>> {
        let team_id: TeamId = team_id.parse()?;
        let mut held = self
            .task_repository
            .list()
            .await?
            .into_iter()
            .filter(|task| {
                task.assignee() == Some(team_id)
                    && matches!(
                        task.state(),
                        TaskState::Assigned | TaskState::InProgress | TaskState::Blocked
                    )
            })
            .collect::<Vec<_>>();
        // A team works one issue at a time, so more than one would be a bug elsewhere;
        // answering with the oldest keeps it deterministic rather than arbitrary.
        held.sort_by_key(|task| task.id().number());
        Ok(held.first().map(Task::snapshot))
    }

    pub async fn start_task(&self, task_id: &str) -> anyhow::Result<Option<TaskSnapshot>> {
        self.mutate(task_id, |task| task.start()).await
    }

    pub async fn block_task(
        &self,
        task_id: &str,
        reason: String,
    ) -> anyhow::Result<Option<TaskSnapshot>> {
        self.mutate(task_id, |task| task.block(reason.clone()))
            .await
    }

    pub async fn resume_task(&self, task_id: &str) -> anyhow::Result<Option<TaskSnapshot>> {
        self.mutate(task_id, |task| task.resume()).await
    }

    pub async fn escalate_task(
        &self,
        task_id: &str,
        reason: String,
    ) -> anyhow::Result<Option<TaskSnapshot>> {
        self.mutate(task_id, |task| task.escalate(reason.clone()))
            .await
    }

    pub async fn complete_task(&self, task_id: &str) -> anyhow::Result<Option<TaskSnapshot>> {
        self.mutate(task_id, |task| task.complete()).await
    }

    pub async fn fail_task(
        &self,
        task_id: &str,
        reason: String,
    ) -> anyhow::Result<Option<TaskSnapshot>> {
        self.mutate(task_id, |task| task.fail(reason.clone())).await
    }

    /// Load, apply a transition, save — retrying the whole cycle if another writer got there
    /// first. `Ok(None)` when no task with the given id exists.
    async fn mutate(
        &self,
        task_id: &str,
        transition: impl Fn(&mut Task) -> Result<(), TaskError>,
    ) -> anyhow::Result<Option<TaskSnapshot>> {
        let id: TaskId = task_id.parse()?;
        loop {
            let Some((mut task, version)) = self.task_repository.get(&id).await? else {
                return Ok(None);
            };
            transition(&mut task)?;
            let snapshot = task.snapshot();
            match self.task_repository.save(task, version).await {
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

    use wiab_core::board::Board;
    use wiab_core::project::ProjectId;
    use wiab_core::repository::RepoError;
    use wiab_core::task::TaskState;
    use wiab_core::work::Work;

    use super::*;

    /// Shares the in-memory task repository so both the service and the test can see it, and
    /// claims under the write lock as the real adapters do.
    #[derive(Default)]
    struct TestTaskRepository {
        tasks: RwLock<HashMap<TaskId, (Task, u64)>>,
    }

    impl TaskRepository for TestTaskRepository {
        async fn save(&self, task: Task, expected: Version) -> Result<Version, SaveError> {
            let mut tasks = self.tasks.write().expect("test write lock poisoned");
            let current = tasks
                .get(&task.id())
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            tasks.insert(task.id(), (task, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &TaskId) -> Result<Option<(Task, Version)>, RepoError> {
            Ok(self
                .tasks
                .read()
                .expect("test read lock poisoned")
                .get(id)
                .map(|(task, version)| (task.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Task>, RepoError> {
            Ok(self
                .tasks
                .read()
                .expect("test read lock poisoned")
                .values()
                .map(|(task, _)| task.clone())
                .collect())
        }

        async fn claim_next(
            &self,
            board_id: &BoardId,
            team_id: TeamId,
        ) -> Result<Option<(Task, Version)>, RepoError> {
            let mut tasks = self.tasks.write().expect("test write lock poisoned");
            let Some(id) = tasks
                .values()
                .filter(|(task, _)| task.board_id() == *board_id && task.state().is_available())
                .map(|(task, _)| task.id())
                .min_by_key(TaskId::number)
            else {
                return Ok(None);
            };
            let (task, version) = tasks.get_mut(&id).expect("just selected");
            task.assign(team_id)
                .map_err(|error| RepoError::Backend(error.to_string()))?;
            *version += 1;
            Ok(Some((task.clone(), Version::from_value(*version))))
        }
    }

    #[derive(Default)]
    struct TestBoardRepository {
        boards: RwLock<HashMap<BoardId, (Board, u64)>>,
    }

    impl BoardRepository for TestBoardRepository {
        async fn save(&self, board: Board, expected: Version) -> Result<Version, SaveError> {
            let next = expected.next();
            self.boards
                .write()
                .expect("test write lock poisoned")
                .insert(board.id(), (board, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &BoardId) -> Result<Option<(Board, Version)>, RepoError> {
            Ok(self
                .boards
                .read()
                .expect("test read lock poisoned")
                .get(id)
                .map(|(board, version)| (board.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Board>, RepoError> {
            Ok(self
                .boards
                .read()
                .expect("test read lock poisoned")
                .values()
                .map(|(board, _)| board.clone())
                .collect())
        }
    }

    #[derive(Default)]
    struct TestWorkRepository {
        works: RwLock<HashMap<WorkId, (Work, u64)>>,
    }

    impl WorkRepository for TestWorkRepository {
        async fn save(&self, work: Work, expected: Version) -> Result<Version, SaveError> {
            let next = expected.next();
            self.works
                .write()
                .expect("test write lock poisoned")
                .insert(work.id(), (work, next.value()));
            Ok(next)
        }

        async fn get(&self, id: &WorkId) -> Result<Option<(Work, Version)>, RepoError> {
            Ok(self
                .works
                .read()
                .expect("test read lock poisoned")
                .get(id)
                .map(|(work, version)| (work.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Work>, RepoError> {
            Ok(self
                .works
                .read()
                .expect("test read lock poisoned")
                .values()
                .map(|(work, _)| work.clone())
                .collect())
        }
    }

    #[derive(Default)]
    struct TestTaskNumbering {
        counter: AtomicU64,
    }

    impl TaskNumbering for TestTaskNumbering {
        fn next(&self) -> TaskId {
            TaskId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    type Svc = TaskApplicationService<TestTaskRepository, TestBoardRepository, TestWorkRepository>;

    fn service() -> Svc {
        TaskApplicationService::new(
            TestTaskRepository::default(),
            TestBoardRepository::default(),
            TestWorkRepository::default(),
            Arc::new(TestTaskNumbering::default()),
        )
    }

    async fn seed_board(service: &Svc, number: u64) -> String {
        let board = Board::new(
            BoardId::from_number(number),
            ProjectId::from_number(1),
            format!("Board {number}"),
            String::new(),
        )
        .unwrap();
        let id = board.id().to_string();
        service
            .board_repository
            .save(board, Version::NEW)
            .await
            .unwrap();
        id
    }

    async fn seed_work(service: &Svc, number: u64) -> String {
        let work = Work::new(
            WorkId::from_number(number),
            ProjectId::from_number(1),
            format!("Work {number}"),
            String::new(),
        )
        .unwrap();
        let id = work.id().to_string();
        service
            .work_repository
            .save(work, Version::NEW)
            .await
            .unwrap();
        id
    }

    async fn queue(service: &Svc, board_id: &str, work_id: &str) -> TaskSnapshot {
        service
            .create_task(
                board_id,
                CreateTaskRequest {
                    work_id: work_id.to_owned(),
                },
            )
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn queues_a_task_waiting_for_no_one() {
        let service = service();
        let board = seed_board(&service, 1).await;
        let work = seed_work(&service, 1).await;

        let created = queue(&service, &board, &work).await;
        assert_eq!(created.id, "T-1");
        assert_eq!(created.state, "created");
        assert_eq!(created.assignee, None);
        assert_eq!(
            service.task_snapshot("T-1").await.unwrap().unwrap(),
            created
        );
    }

    #[tokio::test]
    async fn queueing_needs_both_a_real_board_and_a_real_work() {
        // A task pointing at nothing would be claimed and only then fail, on the team.
        let service = service();
        let board = seed_board(&service, 1).await;
        let work = seed_work(&service, 1).await;

        assert!(
            service
                .create_task("B-404", CreateTaskRequest { work_id: work })
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            service
                .create_task(
                    &board,
                    CreateTaskRequest {
                        work_id: "W-404".to_owned()
                    }
                )
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn lists_only_the_board_s_own_tasks_in_arrival_order() {
        let service = service();
        let first = seed_board(&service, 1).await;
        let second = seed_board(&service, 2).await;
        let work = seed_work(&service, 1).await;
        queue(&service, &first, &work).await;
        queue(&service, &second, &work).await;
        queue(&service, &first, &work).await;

        let listed = service.list_tasks(&first).await.unwrap().unwrap();
        let ids: Vec<&str> = listed.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(ids, ["T-1", "T-3"]);
    }

    #[tokio::test]
    async fn listing_an_unknown_board_is_not_found() {
        let service = service();
        assert!(service.list_tasks("B-404").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn claiming_takes_the_oldest_task_first_then_runs_dry() {
        let service = service();
        let board = seed_board(&service, 1).await;
        let work = seed_work(&service, 1).await;
        queue(&service, &board, &work).await;
        queue(&service, &board, &work).await;

        let first = service
            .claim_next_task(&board, "TM-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.id, "T-1");
        assert_eq!(first.state, "assigned");
        assert_eq!(first.assignee.as_deref(), Some("TM-1"));

        let second = service
            .claim_next_task(&board, "TM-2")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.id, "T-2");

        assert!(
            service
                .claim_next_task(&board, "TM-3")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn claiming_an_empty_or_unknown_board_yields_nothing() {
        let service = service();
        let board = seed_board(&service, 1).await;
        assert!(
            service
                .claim_next_task(&board, "TM-1")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            service
                .claim_next_task("B-404", "TM-1")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn drives_a_task_from_claim_to_completion() {
        let service = service();
        let board = seed_board(&service, 1).await;
        let work = seed_work(&service, 1).await;
        queue(&service, &board, &work).await;
        service.claim_next_task(&board, "TM-1").await.unwrap();

        assert_eq!(
            service.start_task("T-1").await.unwrap().unwrap().state,
            "in_progress"
        );
        let blocked = service
            .block_task("T-1", "waiting on a review".to_owned())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(blocked.state, "blocked");
        assert_eq!(blocked.reason.as_deref(), Some("waiting on a review"));

        assert_eq!(
            service.resume_task("T-1").await.unwrap().unwrap().state,
            "in_progress"
        );
        let done = service.complete_task("T-1").await.unwrap().unwrap();
        assert_eq!(done.state, "completed");
        assert_eq!(done.reason, None);
    }

    #[tokio::test]
    async fn escalating_puts_the_task_back_on_the_board_for_another_team() {
        let service = service();
        let board = seed_board(&service, 1).await;
        let work = seed_work(&service, 1).await;
        queue(&service, &board, &work).await;
        service.claim_next_task(&board, "TM-1").await.unwrap();
        service.start_task("T-1").await.unwrap();

        let escalated = service
            .escalate_task("T-1", "needs a decision".to_owned())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(escalated.state, "escalated");
        assert_eq!(escalated.assignee, None);

        let reclaimed = service
            .claim_next_task(&board, "TM-2")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reclaimed.id, "T-1");
        assert_eq!(reclaimed.assignee.as_deref(), Some("TM-2"));
        assert_eq!(reclaimed.reason, None);
    }

    #[tokio::test]
    async fn failing_keeps_the_team_and_leaves_the_board_empty() {
        let service = service();
        let board = seed_board(&service, 1).await;
        let work = seed_work(&service, 1).await;
        queue(&service, &board, &work).await;
        service.claim_next_task(&board, "TM-1").await.unwrap();
        service.start_task("T-1").await.unwrap();

        let failed = service
            .fail_task("T-1", "the build never went green".to_owned())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(failed.state, "failed");
        assert_eq!(failed.assignee.as_deref(), Some("TM-1"));
        assert!(
            service
                .claim_next_task(&board, "TM-2")
                .await
                .unwrap()
                .is_none(),
            "a failed task must not go back on the board"
        );
    }

    #[tokio::test]
    async fn an_illegal_transition_is_an_error_not_a_silent_no_op() {
        let service = service();
        let board = seed_board(&service, 1).await;
        let work = seed_work(&service, 1).await;
        queue(&service, &board, &work).await;

        assert!(service.start_task("T-1").await.is_err());
        assert!(service.complete_task("T-1").await.is_err());
        assert_eq!(
            service.task_snapshot("T-1").await.unwrap().unwrap().state,
            TaskState::Created.to_string()
        );
    }

    #[tokio::test]
    async fn unknown_tasks_are_not_found() {
        let service = service();
        assert!(service.task_snapshot("T-404").await.unwrap().is_none());
        assert!(service.start_task("T-404").await.unwrap().is_none());
        assert!(service.complete_task("T-404").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_team_can_ask_which_task_it_is_holding() {
        let service = service();
        let board = seed_board(&service, 1).await;
        let work = seed_work(&service, 1).await;
        queue(&service, &board, &work).await;
        service.claim_next_task(&board, "TM-1").await.unwrap();

        // Assigned counts: the team took it and may not have started yet.
        let held = service.held_task("TM-1").await.unwrap().unwrap();
        assert_eq!(held.id, "T-1");

        service.start_task("T-1").await.unwrap();
        assert_eq!(service.held_task("TM-1").await.unwrap().unwrap().id, "T-1");

        // Blocked still counts — the team owns it.
        service
            .block_task("T-1", "waiting".to_owned())
            .await
            .unwrap();
        assert_eq!(service.held_task("TM-1").await.unwrap().unwrap().id, "T-1");
    }

    #[tokio::test]
    async fn a_team_holding_nothing_gets_nothing() {
        let service = service();
        let board = seed_board(&service, 1).await;
        let work = seed_work(&service, 1).await;
        queue(&service, &board, &work).await;

        // Unclaimed work belongs to no one.
        assert!(service.held_task("TM-1").await.unwrap().is_none());

        // Another team's task is not this team's.
        service.claim_next_task(&board, "TM-2").await.unwrap();
        assert!(service.held_task("TM-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn settled_and_escalated_tasks_are_not_held() {
        // Escalation released the team; completing and failing ended the work.
        let service = service();
        let board = seed_board(&service, 1).await;
        let work = seed_work(&service, 1).await;
        for _ in 0..2 {
            queue(&service, &board, &work).await;
        }

        service.claim_next_task(&board, "TM-1").await.unwrap();
        service.start_task("T-1").await.unwrap();
        service
            .escalate_task("T-1", "needs a decision".to_owned())
            .await
            .unwrap();
        assert!(service.held_task("TM-1").await.unwrap().is_none());

        // The escalated task went back to the front of the board, so this claims it again.
        let again = service
            .claim_next_task(&board, "TM-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(again.id, "T-1");
        service.start_task(&again.id).await.unwrap();
        service.complete_task(&again.id).await.unwrap();
        assert!(service.held_task("TM-1").await.unwrap().is_none());
    }
}
