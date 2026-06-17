/// A reference to this user in an external system — e.g. WIAB's agent identity
/// (`("agent", "A-9")`) or a SCIM `externalId`.
///
/// Lets a product link its own concepts to a user without the user model depending on
/// those types. Replaces the previously WIAB-specific `agent_id` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalRef {
    system: String,
    id: String,
}

impl ExternalRef {
    pub fn new(system: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            id: id.into(),
        }
    }

    pub fn system(&self) -> &str {
        &self.system
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}
