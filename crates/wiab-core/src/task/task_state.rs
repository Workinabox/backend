use std::fmt;
use std::str::FromStr;

use crate::task::TaskError;

/// Lifecycle of a task, as specified in `docs/AGENT_MODEL.md`:
///
/// ```text
/// Created → Assigned → InProgress → Completed
///                   ↘             ↗
///                    → Blocked ──
///                   ↘ Escalated (returned to the board with context)
///                   ↘ Failed
/// ```
///
/// `Created` and `Escalated` are the two states in which a task sits on the board waiting to
/// be picked up — an escalated task is one a team gave back, so it becomes available again
/// rather than being terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Created,
    Assigned,
    InProgress,
    Blocked,
    Escalated,
    Completed,
    Failed,
}

impl TaskState {
    /// Whether a team may pick this task up. Both an untouched task and one handed back after
    /// an escalation are fair game; everything else is either held or settled.
    pub fn is_available(&self) -> bool {
        matches!(self, TaskState::Created | TaskState::Escalated)
    }
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            TaskState::Created => "created",
            TaskState::Assigned => "assigned",
            TaskState::InProgress => "in_progress",
            TaskState::Blocked => "blocked",
            TaskState::Escalated => "escalated",
            TaskState::Completed => "completed",
            TaskState::Failed => "failed",
        };
        f.write_str(text)
    }
}

impl FromStr for TaskState {
    type Err = TaskError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "created" => Ok(TaskState::Created),
            "assigned" => Ok(TaskState::Assigned),
            "in_progress" => Ok(TaskState::InProgress),
            "blocked" => Ok(TaskState::Blocked),
            "escalated" => Ok(TaskState::Escalated),
            "completed" => Ok(TaskState::Completed),
            "failed" => Ok(TaskState::Failed),
            other => Err(TaskError::InvalidTaskState(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [TaskState; 7] = [
        TaskState::Created,
        TaskState::Assigned,
        TaskState::InProgress,
        TaskState::Blocked,
        TaskState::Escalated,
        TaskState::Completed,
        TaskState::Failed,
    ];

    #[test]
    fn round_trips_through_string() {
        for state in ALL {
            assert_eq!(state.to_string().parse::<TaskState>().unwrap(), state);
        }
    }

    #[test]
    fn an_escalated_task_goes_back_on_the_board() {
        // Escalation hands work back, so it must be pickable again — not terminal.
        assert!(TaskState::Escalated.is_available());
    }

    #[test]
    fn only_untouched_and_escalated_tasks_are_available() {
        for state in ALL {
            let expected = matches!(state, TaskState::Created | TaskState::Escalated);
            assert_eq!(state.is_available(), expected, "{state}");
        }
    }

    #[test]
    fn rejects_unknown_state() {
        assert_eq!(
            "doing".parse::<TaskState>().unwrap_err(),
            TaskError::InvalidTaskState("doing".to_owned())
        );
    }
}
