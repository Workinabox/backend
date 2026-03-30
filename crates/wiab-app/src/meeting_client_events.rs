use wiab_core::{meeting::MinutesDocument, meeting_traits::SpeechClip};

#[derive(Debug, Clone)]
pub enum MeetingClientEvent {
    AgentText {
        meeting_id: String,
        participant_id: String,
        participant_name: String,
        utterance_id: String,
        text: String,
    },
    AgentSpeech {
        meeting_id: String,
        participant_id: String,
        participant_name: String,
        utterance_id: String,
        clip: SpeechClip,
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
