use wiab_core::meeting::MinutesDocument;

#[derive(Debug, Clone)]
pub struct AgentAudioClip {
    pub mime_type: String,
    pub audio_base64: String,
}

#[derive(Debug, Clone)]
pub enum MeetingClientEvent {
    AgentText {
        meeting_id: String,
        participant_id: String,
        participant_name: String,
        utterance_id: String,
        text: String,
    },
    AgentAudio {
        meeting_id: String,
        participant_id: String,
        participant_name: String,
        utterance_id: String,
        clip: AgentAudioClip,
    },
    MeetingEnded {
        meeting_id: String,
        ended_by_participant_id: String,
        ended_at: String,
    },
    MinutesReady {
        meeting_id: String,
        minutes: MinutesDocument,
    },
}
