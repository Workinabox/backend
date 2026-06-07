use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

use crate::work::WorkError;

/// Stable identity for an acceptance criterion within a `Work`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoneId(Uuid);

impl DoneId {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for DoneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DoneId {
    type Err = WorkError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value)
            .map(DoneId)
            .map_err(|_| WorkError::InvalidDoneId(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_string() {
        let id = DoneId::new();
        assert_eq!(id.to_string().parse::<DoneId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_id() {
        assert_eq!(
            "not-a-uuid".parse::<DoneId>().unwrap_err(),
            WorkError::InvalidDoneId("not-a-uuid".to_owned())
        );
    }
}
