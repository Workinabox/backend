use serde::{Deserialize, Serialize};

/// Something that happened, recorded by the aggregate it happened to.
///
/// Aggregates collect these as they change and hand them over on save; the repository
/// writes them alongside the row in one transaction, so an event cannot be lost after the
/// change it describes has been committed, nor published for a change that was rolled back.
///
/// `payload` is JSON rather than a typed body: consumers live outside this process (and
/// outside this language), so the wire shape is the contract, not a Rust enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainEvent {
    /// Dotted name, e.g. `team.started`. Doubles as the broker subject.
    pub name: String,
    /// The aggregate this happened to, e.g. `TM-1`.
    pub aggregate_id: String,
    pub payload: serde_json::Value,
}

impl DomainEvent {
    pub fn new(
        name: impl Into<String>,
        aggregate_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            aggregate_id: aggregate_id.into(),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let event = DomainEvent::new("team.started", "TM-1", serde_json::json!({"vm_id": "VM-2"}));
        let encoded = serde_json::to_vec(&event).unwrap();
        let decoded: DomainEvent = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn the_name_is_the_subject_consumers_subscribe_to() {
        let event = DomainEvent::new("task.completed", "T-3", serde_json::Value::Null);
        assert_eq!(event.name, "task.completed");
        assert_eq!(event.aggregate_id, "T-3");
    }
}
