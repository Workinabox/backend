use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMeetingRequest {
    pub title: String,
    pub owner: CreateMeetingParticipant,
    #[serde(default)]
    pub invited_participants: Vec<CreateMeetingParticipant>,
    pub agenda: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreateMeetingParticipant {
    Human {
        name: String,
    },
    Agent {
        name: String,
        instructions: String,
        voice_id: String,
    },
}
