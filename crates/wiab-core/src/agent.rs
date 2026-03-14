use thiserror::Error;

use crate::meeting::{Meeting, MeetingParticipant, MinutesDocument};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloorRequestCandidate {
    pub floor_request_id: String,
    pub participant_id: String,
    pub score: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechClip {
    pub mime_type: String,
    pub audio_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SpeechSynthesisError {
    #[error("{0}")]
    Message(String),
}

pub trait MeetingIntelligence: Send + Sync {
    fn evaluate_floor_requests(
        &self,
        meeting: &Meeting,
        utterance_text: &str,
        source_utterance_id: &str,
    ) -> Vec<FloorRequestCandidate>;

    fn select_floor_request(
        &self,
        meeting: &Meeting,
        utterance_text: &str,
        floor_requests: &[FloorRequestCandidate],
    ) -> Option<String>;

    fn generate_agent_reply(
        &self,
        meeting: &Meeting,
        agent: &MeetingParticipant,
        utterance_text: &str,
    ) -> String;

    fn generate_minutes(&self, meeting: &Meeting) -> MinutesDocument;
}

pub trait SpeechSynthesizer: Send + Sync {
    fn synthesize(&self, text: &str, voice_id: &str) -> Result<SpeechClip, SpeechSynthesisError>;
}

pub trait Clock: Send + Sync {
    fn now_rfc3339(&self) -> String;
}
