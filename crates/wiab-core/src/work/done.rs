use crate::work::{DoneId, DoneView, WorkError};

/// An acceptance criterion that must be fulfilled. Internal entity of the `Work` aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    id: DoneId,
    criterion: String,
    fulfilled: bool,
}

impl Done {
    pub fn new(criterion: String) -> Result<Self, WorkError> {
        if criterion.trim().is_empty() {
            return Err(WorkError::EmptyCriterion);
        }
        Ok(Self {
            id: DoneId::new(),
            criterion,
            fulfilled: false,
        })
    }

    /// Rebuild a `Done` from persisted state (used by repository implementations).
    pub fn from_persistence(id: DoneId, criterion: String, fulfilled: bool) -> Self {
        Self {
            id,
            criterion,
            fulfilled,
        }
    }

    pub fn id(&self) -> DoneId {
        self.id
    }

    pub fn criterion(&self) -> &str {
        &self.criterion
    }

    pub fn is_fulfilled(&self) -> bool {
        self.fulfilled
    }

    pub fn fulfill(&mut self) {
        self.fulfilled = true;
    }

    pub fn unfulfill(&mut self) {
        self.fulfilled = false;
    }

    pub fn view(&self) -> DoneView {
        DoneView {
            id: self.id.to_string(),
            criterion: self.criterion.clone(),
            fulfilled: self.fulfilled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_criterion() {
        assert_eq!(
            Done::new("   ".to_owned()).unwrap_err(),
            WorkError::EmptyCriterion
        );
    }

    #[test]
    fn starts_unfulfilled_and_toggles() {
        let mut done = Done::new("tests pass".to_owned()).unwrap();
        assert!(!done.is_fulfilled());
        done.fulfill();
        assert!(done.is_fulfilled());
        done.unfulfill();
        assert!(!done.is_fulfilled());
    }

    #[test]
    fn exposes_accessors_and_view() {
        let mut done = Done::new("ship it".to_owned()).unwrap();
        assert_eq!(done.criterion(), "ship it");
        done.fulfill();
        let view = done.view();
        assert_eq!(view.id, done.id().to_string());
        assert_eq!(view.criterion, "ship it");
        assert!(view.fulfilled);
    }
}
