use std::fmt;
use std::str::FromStr;

use crate::task::TaskError;

/// Human-readable task identifier, rendered "T-7".
///
/// The number is minted by the `TaskNumbering` seam at the application layer and passed into
/// `Task::new`; the domain never invents its own sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(u64);

impl TaskId {
    pub fn from_number(number: u64) -> Self {
        Self(number)
    }

    pub fn number(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "T-{}", self.0)
    }
}

impl FromStr for TaskId {
    type Err = TaskError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .strip_prefix("T-")
            .and_then(|number| number.parse::<u64>().ok())
            .map(TaskId)
            .ok_or_else(|| TaskError::InvalidTaskId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_t_prefix() {
        assert_eq!(TaskId::from_number(7).to_string(), "T-7");
    }

    #[test]
    fn exposes_number() {
        assert_eq!(TaskId::from_number(9).number(), 9);
    }

    #[test]
    fn round_trips_through_string() {
        let id = TaskId::from_number(123);
        assert_eq!(id.to_string().parse::<TaskId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "42".parse::<TaskId>().unwrap_err(),
            TaskError::InvalidTaskId("42".to_owned())
        );
        assert!("T-abc".parse::<TaskId>().is_err());
    }

    /// `TM-` (team) is the prefix `T-` could plausibly be confused with.
    #[test]
    fn rejects_a_team_id() {
        assert!("TM-1".parse::<TaskId>().is_err());
    }
}
