use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMeetingRequest {
    pub title: String,
    /// Display name for the owner's seat. The owner is always the authenticated caller, so the
    /// request supplies only what to call them — letting the body name the owning *user* would
    /// make creating a meeting a way to hand someone else's identity a seat.
    pub owner_name: String,
    #[serde(default)]
    pub invited_participants: Vec<CreateMeetingParticipant>,
    pub agenda: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreateMeetingParticipant {
    Human {
        name: String,
        /// The platform user this seat belongs to; only they can occupy it.
        user_id: String,
    },
    Agent {
        name: String,
        instructions: String,
        voice_id: String,
    },
}
