use crate::board::BoardId;
use crate::event::DomainEvent;
use crate::task::{TaskError, TaskId, TaskSnapshot, TaskState};
use crate::team::TeamId;
use crate::work::WorkId;

/// A unit of work queued on a board for a team to pick up.
///
/// A `Task` is the *scheduling* of a `Work`: the work says what to do and what "done" means,
/// the task says where it is queued, who holds it, and how far along it is. The two are
/// separate because the same work can be attempted more than once — an escalated task goes
/// back on the board and is picked up again — while the work itself does not change.
///
/// `Task` is an aggregate root; it references its board, its work and its assignee by
/// identity only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    id: TaskId,
    board_id: BoardId,
    work_id: WorkId,
    state: TaskState,
    assignee: Option<TeamId>,
    /// Why the task is blocked, escalated or failed. Cleared whenever it moves on.
    reason: Option<String>,
    /// What has happened to this task since it was loaded, drained on save.
    events: Vec<DomainEvent>,
}

impl Task {
    /// Queue a work item on a board. A new task is unassigned and waiting.
    pub fn new(id: TaskId, board_id: BoardId, work_id: WorkId) -> Self {
        Self {
            id,
            board_id,
            work_id,
            state: TaskState::Created,
            assignee: None,
            reason: None,
            events: Vec::new(),
        }
    }

    /// Rebuild from persisted state, including states `new` cannot produce.
    pub fn from_persistence(
        id: TaskId,
        board_id: BoardId,
        work_id: WorkId,
        state: TaskState,
        assignee: Option<TeamId>,
        reason: Option<String>,
    ) -> Self {
        Self {
            id,
            board_id,
            work_id,
            state,
            assignee,
            reason,
            events: Vec::new(),
        }
    }

    pub fn id(&self) -> TaskId {
        self.id
    }

    pub fn board_id(&self) -> BoardId {
        self.board_id
    }

    pub fn work_id(&self) -> WorkId {
        self.work_id
    }

    pub fn state(&self) -> TaskState {
        self.state
    }

    pub fn assignee(&self) -> Option<TeamId> {
        self.assignee
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// Hand the task to a team. Legal from the two waiting states — an escalated task is one
    /// that came back to the board, so it can be handed out again.
    ///
    /// Push (a human or parent agent assigns) and pull (a team takes the next task off the
    /// board) are the same transition; which one happens is policy, not domain.
    pub fn assign(&mut self, team_id: TeamId) -> Result<(), TaskError> {
        if !self.state.is_available() {
            return Err(TaskError::NotAvailable(self.state));
        }
        self.state = TaskState::Assigned;
        self.assignee = Some(team_id);
        self.reason = None;
        self.record(
            "task.assigned",
            serde_json::json!({"team_id": team_id.to_string()}),
        );
        Ok(())
    }

    /// The team has begun. Only legal from `Assigned`, so nothing is worked on unheld.
    pub fn start(&mut self) -> Result<(), TaskError> {
        if self.state != TaskState::Assigned {
            return Err(TaskError::NotAssigned(self.state));
        }
        self.state = TaskState::InProgress;
        self.record("task.started", serde_json::json!({}));
        Ok(())
    }

    /// Work has stalled on something external. The team keeps the task.
    pub fn block(&mut self, reason: String) -> Result<(), TaskError> {
        if self.state != TaskState::InProgress {
            return Err(TaskError::NotInProgress(self.state));
        }
        let reason = non_empty(reason)?;
        self.state = TaskState::Blocked;
        self.record("task.blocked", serde_json::json!({"reason": reason}));
        self.reason = Some(reason);
        Ok(())
    }

    /// The blocker is gone. Back to work, on the same team.
    pub fn resume(&mut self) -> Result<(), TaskError> {
        if self.state != TaskState::Blocked {
            return Err(TaskError::NotBlocked(self.state));
        }
        self.state = TaskState::InProgress;
        self.reason = None;
        self.record("task.resumed", serde_json::json!({}));
        Ok(())
    }

    /// Give the task back to the board with context, releasing the team. Legal while working
    /// or blocked — a team that cannot proceed hands the work on rather than failing it.
    pub fn escalate(&mut self, reason: String) -> Result<(), TaskError> {
        if !matches!(self.state, TaskState::InProgress | TaskState::Blocked) {
            return Err(TaskError::NotInProgress(self.state));
        }
        let reason = non_empty(reason)?;
        self.state = TaskState::Escalated;
        self.assignee = None;
        self.record("task.escalated", serde_json::json!({"reason": reason}));
        self.reason = Some(reason);
        Ok(())
    }

    /// The work is done. Terminal.
    pub fn complete(&mut self) -> Result<(), TaskError> {
        if self.state != TaskState::InProgress {
            return Err(TaskError::NotInProgress(self.state));
        }
        self.state = TaskState::Completed;
        self.reason = None;
        self.record("task.completed", serde_json::json!({}));
        Ok(())
    }

    /// The work could not be done. Terminal, and keeps the team on it as the record of who
    /// tried.
    pub fn fail(&mut self, reason: String) -> Result<(), TaskError> {
        if !matches!(self.state, TaskState::InProgress | TaskState::Blocked) {
            return Err(TaskError::NotInProgress(self.state));
        }
        let reason = non_empty(reason)?;
        self.state = TaskState::Failed;
        self.record("task.failed", serde_json::json!({"reason": reason}));
        self.reason = Some(reason);
        Ok(())
    }

    /// Hand over what has happened, clearing it. Called by the repository, which writes
    /// these in the same transaction as the row — see `DomainEvent`.
    pub fn take_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.events)
    }

    fn record(&mut self, name: &str, payload: serde_json::Value) {
        self.events
            .push(DomainEvent::new(name, self.id.to_string(), payload));
    }

    pub fn snapshot(&self) -> TaskSnapshot {
        TaskSnapshot {
            id: self.id.to_string(),
            board_id: self.board_id.to_string(),
            work_id: self.work_id.to_string(),
            state: self.state.to_string(),
            assignee: self.assignee.map(|id| id.to_string()),
            reason: self.reason.clone(),
        }
    }
}

/// A reason that is blank tells a human nothing, which defeats the point of recording it.
fn non_empty(reason: String) -> Result<String, TaskError> {
    if reason.trim().is_empty() {
        return Err(TaskError::EmptyReason);
    }
    Ok(reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Task {
        Task::new(
            TaskId::from_number(1),
            BoardId::from_number(2),
            WorkId::from_number(3),
        )
    }

    /// Drive a task to `InProgress`, the state most transitions start from.
    fn working_task() -> Task {
        let mut task = task();
        task.assign(TeamId::from_number(4)).unwrap();
        task.start().unwrap();
        task
    }

    #[test]
    fn a_new_task_waits_on_the_board_for_no_one() {
        let task = task();
        assert_eq!(task.state(), TaskState::Created);
        assert_eq!(task.assignee(), None);
        assert_eq!(task.reason(), None);
    }

    #[test]
    fn exposes_getters() {
        let task = task();
        assert_eq!(task.id(), TaskId::from_number(1));
        assert_eq!(task.board_id(), BoardId::from_number(2));
        assert_eq!(task.work_id(), WorkId::from_number(3));
    }

    #[test]
    fn assigning_records_the_team() {
        let mut task = task();
        task.assign(TeamId::from_number(4)).unwrap();
        assert_eq!(task.state(), TaskState::Assigned);
        assert_eq!(task.assignee(), Some(TeamId::from_number(4)));
    }

    #[test]
    fn a_held_task_cannot_be_assigned_again() {
        // Otherwise two teams could believe they hold the same task.
        let mut task = working_task();
        assert_eq!(
            task.assign(TeamId::from_number(5)).unwrap_err(),
            TaskError::NotAvailable(TaskState::InProgress)
        );
    }

    #[test]
    fn starting_before_being_assigned_is_rejected() {
        let mut task = task();
        assert_eq!(
            task.start().unwrap_err(),
            TaskError::NotAssigned(TaskState::Created)
        );
    }

    #[test]
    fn blocking_keeps_the_team_and_records_why() {
        let mut task = working_task();
        task.block("waiting on a review".to_owned()).unwrap();
        assert_eq!(task.state(), TaskState::Blocked);
        assert_eq!(task.assignee(), Some(TeamId::from_number(4)));
        assert_eq!(task.reason(), Some("waiting on a review"));
    }

    #[test]
    fn resuming_clears_the_blocker() {
        let mut task = working_task();
        task.block("waiting on a review".to_owned()).unwrap();
        task.resume().unwrap();
        assert_eq!(task.state(), TaskState::InProgress);
        assert_eq!(task.reason(), None);
    }

    #[test]
    fn resuming_a_task_that_is_not_blocked_is_rejected() {
        let mut task = working_task();
        assert_eq!(
            task.resume().unwrap_err(),
            TaskError::NotBlocked(TaskState::InProgress)
        );
    }

    #[test]
    fn escalating_returns_the_task_to_the_board_with_context() {
        let mut task = working_task();
        task.escalate("needs a decision on the schema".to_owned())
            .unwrap();
        assert_eq!(task.state(), TaskState::Escalated);
        assert_eq!(task.assignee(), None, "the team is released");
        assert_eq!(task.reason(), Some("needs a decision on the schema"));
        assert!(task.state().is_available());
    }

    #[test]
    fn an_escalated_task_can_be_picked_up_again_and_loses_the_stale_reason() {
        let mut task = working_task();
        task.escalate("needs a decision".to_owned()).unwrap();
        task.assign(TeamId::from_number(9)).unwrap();
        assert_eq!(task.assignee(), Some(TeamId::from_number(9)));
        assert_eq!(task.reason(), None);
    }

    #[test]
    fn a_blocked_task_can_be_escalated_or_failed_without_resuming_first() {
        for outcome in [
            (|t: &mut Task| t.escalate("stuck".to_owned())) as fn(&mut Task) -> _,
            |t: &mut Task| t.fail("stuck".to_owned()),
        ] {
            let mut task = working_task();
            task.block("waiting".to_owned()).unwrap();
            outcome(&mut task).unwrap();
            assert_ne!(task.state(), TaskState::Blocked);
        }
    }

    #[test]
    fn completing_settles_the_task() {
        let mut task = working_task();
        task.complete().unwrap();
        assert_eq!(task.state(), TaskState::Completed);
        assert_eq!(task.reason(), None);
    }

    #[test]
    fn completing_a_task_nobody_started_is_rejected() {
        let mut task = task();
        assert_eq!(
            task.complete().unwrap_err(),
            TaskError::NotInProgress(TaskState::Created)
        );
    }

    #[test]
    fn failing_keeps_the_team_as_the_record_of_who_tried() {
        let mut task = working_task();
        task.fail("the build never went green".to_owned()).unwrap();
        assert_eq!(task.state(), TaskState::Failed);
        assert_eq!(task.assignee(), Some(TeamId::from_number(4)));
        assert_eq!(task.reason(), Some("the build never went green"));
    }

    #[test]
    fn a_settled_task_stays_settled() {
        for settle in [
            (|t: &mut Task| t.complete()) as fn(&mut Task) -> _,
            |t: &mut Task| t.fail("no".to_owned()),
        ] {
            let mut task = working_task();
            settle(&mut task).unwrap();
            let state = task.state();
            assert!(task.assign(TeamId::from_number(5)).is_err());
            assert!(task.start().is_err());
            assert!(task.block("x".to_owned()).is_err());
            assert!(task.escalate("x".to_owned()).is_err());
            assert_eq!(task.state(), state);
        }
    }

    #[test]
    fn a_blank_reason_is_rejected() {
        // A reason nobody can read is worse than none — it looks recorded.
        let mut task = working_task();
        assert_eq!(
            task.block("   ".to_owned()).unwrap_err(),
            TaskError::EmptyReason
        );
        assert_eq!(
            task.escalate(String::new()).unwrap_err(),
            TaskError::EmptyReason
        );
        assert_eq!(
            task.fail(" ".to_owned()).unwrap_err(),
            TaskError::EmptyReason
        );
        assert_eq!(task.state(), TaskState::InProgress, "no partial transition");
    }

    #[test]
    fn snapshot_mirrors_the_task() {
        let mut task = working_task();
        task.block("waiting on a review".to_owned()).unwrap();
        let snapshot = task.snapshot();
        assert_eq!(snapshot.id, "T-1");
        assert_eq!(snapshot.board_id, "B-2");
        assert_eq!(snapshot.work_id, "W-3");
        assert_eq!(snapshot.state, "blocked");
        assert_eq!(snapshot.assignee.as_deref(), Some("TM-4"));
        assert_eq!(snapshot.reason.as_deref(), Some("waiting on a review"));
    }

    #[test]
    fn from_persistence_round_trips_an_escalated_task() {
        let task = Task::from_persistence(
            TaskId::from_number(1),
            BoardId::from_number(2),
            WorkId::from_number(3),
            TaskState::Escalated,
            None,
            Some("needs a decision".to_owned()),
        );
        assert_eq!(task.state(), TaskState::Escalated);
        assert_eq!(task.reason(), Some("needs a decision"));
    }

    #[test]
    fn a_task_records_what_happened_to_it() {
        let mut task = working_task();
        task.complete().unwrap();

        let names: Vec<String> = task.take_events().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["task.assigned", "task.started", "task.completed"]);
    }

    #[test]
    fn taking_events_clears_them_so_a_save_cannot_publish_twice() {
        let mut task = working_task();
        assert!(!task.take_events().is_empty());
        assert!(task.take_events().is_empty());
    }

    #[test]
    fn an_event_carries_the_reason_a_human_needs() {
        let mut task = working_task();
        task.fail("the build never went green".to_owned()).unwrap();

        let failed = task
            .take_events()
            .into_iter()
            .find(|e| e.name == "task.failed")
            .expect("failing records an event");
        assert_eq!(failed.aggregate_id, "T-1");
        assert_eq!(failed.payload["reason"], "the build never went green");
    }

    #[test]
    fn a_rejected_transition_records_nothing() {
        // Otherwise consumers would hear about changes that never happened.
        let mut task = task();
        assert!(task.complete().is_err());
        assert!(task.take_events().is_empty());
    }
}
