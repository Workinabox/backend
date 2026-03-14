use std::{fs, path::Path, process::Command};

use uuid::Uuid;
use wiab_core::agent::{SpeechClip, SpeechSynthesisError, SpeechSynthesizer};

pub struct DefaultSpeechSynthesizer {
    implementation: SpeechSynthesizerImpl,
}

impl DefaultSpeechSynthesizer {
    pub fn from_env() -> Self {
        if cfg!(target_os = "macos") && command_exists("say") && command_exists("afconvert") {
            Self {
                implementation: SpeechSynthesizerImpl::MacOsSay,
            }
        } else {
            Self {
                implementation: SpeechSynthesizerImpl::Unavailable,
            }
        }
    }
}

impl SpeechSynthesizer for DefaultSpeechSynthesizer {
    fn synthesize(&self, text: &str, voice_id: &str) -> Result<SpeechClip, SpeechSynthesisError> {
        match self.implementation {
            SpeechSynthesizerImpl::MacOsSay => synthesize_with_say(text, voice_id),
            SpeechSynthesizerImpl::Unavailable => Err(SpeechSynthesisError::Message(
                "no supported speech synthesizer is configured for this environment".to_owned(),
            )),
        }
    }
}

enum SpeechSynthesizerImpl {
    MacOsSay,
    Unavailable,
}

fn command_exists(binary: &str) -> bool {
    Command::new("which")
        .arg(binary)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn synthesize_with_say(text: &str, voice_id: &str) -> Result<SpeechClip, SpeechSynthesisError> {
    let temp_dir = std::env::temp_dir();
    let unique = Uuid::new_v4().to_string();
    let aiff_path = temp_dir.join(format!("wiab-agent-{unique}.aiff"));
    let wav_path = temp_dir.join(format!("wiab-agent-{unique}.wav"));

    let say_status = Command::new("say")
        .args([
            "-v",
            say_voice_for(voice_id),
            "-o",
            path_str(&aiff_path)?,
            text,
        ])
        .status()
        .map_err(|err| SpeechSynthesisError::Message(format!("failed to execute 'say': {err}")))?;
    if !say_status.success() {
        return Err(SpeechSynthesisError::Message(format!(
            "'say' exited with status {}",
            say_status
        )));
    }

    let afconvert_status = Command::new("afconvert")
        .args([
            "-f",
            "WAVE",
            "-d",
            "LEI16@22050",
            path_str(&aiff_path)?,
            path_str(&wav_path)?,
        ])
        .status()
        .map_err(|err| {
            SpeechSynthesisError::Message(format!("failed to execute 'afconvert': {err}"))
        })?;
    if !afconvert_status.success() {
        return Err(SpeechSynthesisError::Message(format!(
            "'afconvert' exited with status {}",
            afconvert_status
        )));
    }

    let wav_bytes = fs::read(&wav_path).map_err(|err| {
        SpeechSynthesisError::Message(format!("failed to read synthesized wav audio: {err}"))
    })?;

    let _ = fs::remove_file(&aiff_path);
    let _ = fs::remove_file(&wav_path);

    Ok(SpeechClip {
        mime_type: "audio/wav".to_owned(),
        audio_bytes: wav_bytes,
    })
}

fn say_voice_for(voice_id: &str) -> &str {
    match voice_id {
        "alloy" => "Samantha",
        "verse" => "Daniel",
        "aria" => "Karen",
        _ => "Samantha",
    }
}

fn path_str(path: &Path) -> Result<&str, SpeechSynthesisError> {
    path.to_str().ok_or_else(|| {
        SpeechSynthesisError::Message("temporary path contains non-utf8 characters".to_owned())
    })
}
