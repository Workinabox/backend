use tracing::warn;
use wiab_core::meeting_traits::{SpeechClip, SpeechSynthesisError, SpeechSynthesizer};

/// Agent voice synthesis is not implemented yet. Fails loudly — warns once at
/// startup, returns an error per call — instead of silently doing nothing.
pub struct DefaultSpeechSynthesizer;

impl DefaultSpeechSynthesizer {
    pub fn from_env() -> Self {
        warn!("speech synthesis is not implemented yet — agent voice is disabled");
        Self
    }
}

impl SpeechSynthesizer for DefaultSpeechSynthesizer {
    fn synthesize(&self, _text: &str, _voice_id: &str) -> Result<SpeechClip, SpeechSynthesisError> {
        Err(SpeechSynthesisError::Message(
            "speech synthesis is not implemented yet".to_owned(),
        ))
    }
}
