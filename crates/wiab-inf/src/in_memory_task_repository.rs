use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::board::BoardId;
use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::task::{Task, TaskId, TaskRepository};
use wiab_core::team::TeamId;

#[derive(Debug, Clone, Default)]
pub struct InMemoryTaskRepository {
    tasks: Arc<RwLock<HashMap<TaskId, (Task, u64)>>>,
}

impl InMemoryTaskRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TaskRepository for InMemoryTaskRepository {
    async fn save(&self, task: Task, expected: Version) -> Result<Version, SaveError> {
        let mut tasks = self
            .tasks
            .write()
            .expect("task repository write lock poisoned");
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
            .expect("task repository read lock poisoned")
            .get(id)
            .map(|(task, version)| (task.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Task>, RepoError> {
        Ok(self
            .tasks
            .read()
            .expect("task repository read lock poisoned")
            .values()
            .map(|(task, _)| task.clone())
            .collect())
    }

    /// Claims under the write lock, so a concurrent claimer cannot see the same task
    /// available. This is the in-memory stand-in for the Postgres `FOR UPDATE SKIP LOCKED`.
    async fn claim_next(
        &self,
        board_id: &BoardId,
        team_id: TeamId,
    ) -> Result<Option<(Task, Version)>, RepoError> {
        let mut tasks = self
            .tasks
            .write()
            .expect("task repository write lock poisoned");
        // Oldest first: the id is the arrival order.
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
