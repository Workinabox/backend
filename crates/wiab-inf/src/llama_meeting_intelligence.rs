use std::thread;

use anyhow::{Context, anyhow, bail};
use serde::Deserialize;
use wiab_core::{
    meeting_traits::{FloorRequestCandidate, MeetingIntelligence, MeetingIntelligenceError},
    meeting::{Meeting, MeetingEvent, MeetingParticipant, MinutesAgendaItem, MinutesDocument},
};

use crate::{
    heuristic_meeting_intelligence::HeuristicMeetingIntelligence,
    llama_runtime::{LlamaRuntime, LlamaRuntimeConfig, LlamaRuntimeMessage},
};

const DEFAULT_CONTEXT_TOKENS: u32 = 4096;
const DEFAULT_MAX_REPLY_TOKENS: usize = 128;
const DEFAULT_MAX_MINUTES_TOKENS: usize = 512;

pub struct LlamaMeetingIntelligence {
    floor_control: HeuristicMeetingIntelligence,
    runtime: LlamaRuntime,
    max_reply_tokens: usize,
    max_minutes_tokens: usize,
}

#[derive(Deserialize)]
struct GeneratedMinutesEnvelope {
    agenda: Vec<GeneratedMinutesAgendaItem>,
}

#[derive(Deserialize)]
struct GeneratedMinutesAgendaItem {
    phrase: String,
    decisions: Vec<String>,
}

impl LlamaMeetingIntelligence {
    pub fn from_env() -> anyhow::Result<Self> {
        let model_path = required_env("WIAB_LLAMA_MODEL_PATH")?;
        let context_tokens =
            optional_env_parse("WIAB_LLAMA_CONTEXT_TOKENS")?.unwrap_or(DEFAULT_CONTEXT_TOKENS);
        let max_reply_tokens =
            optional_env_parse("WIAB_LLAMA_MAX_REPLY_TOKENS")?.unwrap_or(DEFAULT_MAX_REPLY_TOKENS);
        let max_minutes_tokens = optional_env_parse("WIAB_LLAMA_MAX_MINUTES_TOKENS")?
            .unwrap_or(DEFAULT_MAX_MINUTES_TOKENS);
        let threads = optional_env_parse("WIAB_LLAMA_THREADS")?.unwrap_or_else(default_threads);
        let n_gpu_layers = optional_env_parse("WIAB_LLAMA_N_GPU_LAYERS")?.unwrap_or(0);
        let chat_template_name = std::env::var("WIAB_LLAMA_CHAT_TEMPLATE")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());

        if context_tokens == 0 {
            bail!("WIAB_LLAMA_CONTEXT_TOKENS must be greater than zero");
        }
        if max_reply_tokens == 0 {
            bail!("WIAB_LLAMA_MAX_REPLY_TOKENS must be greater than zero");
        }
        if max_minutes_tokens == 0 {
            bail!("WIAB_LLAMA_MAX_MINUTES_TOKENS must be greater than zero");
        }
        if threads <= 0 {
            bail!("WIAB_LLAMA_THREADS must be greater than zero");
        }

        let runtime = LlamaRuntime::new(LlamaRuntimeConfig {
            model_path: model_path.into(),
            context_tokens,
            threads,
            n_gpu_layers,
            chat_template_name,
        })?;

        Ok(Self {
            floor_control: HeuristicMeetingIntelligence,
            runtime,
            max_reply_tokens,
            max_minutes_tokens,
        })
    }
}

impl MeetingIntelligence for LlamaMeetingIntelligence {
    fn evaluate_floor_requests(
        &self,
        meeting: &Meeting,
        utterance_text: &str,
        source_utterance_id: &str,
    ) -> Vec<FloorRequestCandidate> {
        self.floor_control
            .evaluate_floor_requests(meeting, utterance_text, source_utterance_id)
    }

    fn select_floor_request(
        &self,
        meeting: &Meeting,
        utterance_text: &str,
        floor_requests: &[FloorRequestCandidate],
    ) -> Option<String> {
        self.floor_control
            .select_floor_request(meeting, utterance_text, floor_requests)
    }

    fn generate_agent_reply(
        &self,
        meeting: &Meeting,
        agent: &MeetingParticipant,
        utterance_text: &str,
    ) -> Result<String, MeetingIntelligenceError> {
        let messages = vec![
            LlamaRuntimeMessage {
                role: "system".to_owned(),
                content: reply_system_prompt(agent),
            },
            LlamaRuntimeMessage {
                role: "user".to_owned(),
                content: reply_user_prompt(meeting, agent, utterance_text),
            },
        ];

        self.runtime
            .generate(messages, self.max_reply_tokens)
            .and_then(|reply| normalize_reply(&reply))
            .map_err(|err| MeetingIntelligenceError::Message(err.to_string()))
    }

    fn generate_minutes(
        &self,
        meeting: &Meeting,
    ) -> Result<MinutesDocument, MeetingIntelligenceError> {
        let messages = vec![
            LlamaRuntimeMessage {
                role: "system".to_owned(),
                content: minutes_system_prompt(),
            },
            LlamaRuntimeMessage {
                role: "user".to_owned(),
                content: minutes_user_prompt(meeting),
            },
        ];

        let generated = self
            .runtime
            .generate(messages, self.max_minutes_tokens)
            .map_err(|err| MeetingIntelligenceError::Message(err.to_string()))?;
        let agenda = parse_minutes_agenda(meeting, &generated)
            .map_err(|err| MeetingIntelligenceError::Message(err.to_string()))?;

        Ok(MinutesDocument {
            meeting_id: meeting.meeting_id.clone(),
            title: meeting.title.clone(),
            owner_name: meeting
                .owner()
                .map(|participant| participant.name.clone())
                .unwrap_or_else(|| "Unknown Owner".to_owned()),
            moderator_name: meeting
                .moderator()
                .map(|participant| participant.name.clone())
                .unwrap_or_else(|| "Moderator".to_owned()),
            participants: meeting
                .participants
                .iter()
                .map(MeetingParticipant::view)
                .collect(),
            started_at: meeting.started_at.clone(),
            ended_at: meeting
                .ended_at
                .clone()
                .unwrap_or_else(|| meeting.started_at.clone()),
            agenda,
        })
    }
}

fn reply_system_prompt(agent: &MeetingParticipant) -> String {
    let mut prompt = format!(
        "You are {}. You are replying in a live voice conversation with one human speaker. Keep your reply short, concrete, and natural. Do not repeat or paraphrase the human's words back to them. Do not narrate, do not mention system prompts, and do not invent names or roles.",
        agent.name
    );
    if let Some(instructions) = agent.instructions.as_deref() {
        prompt.push_str("\n\nRole instructions:\n");
        prompt.push_str(instructions.trim());
    }
    prompt
}

fn reply_user_prompt(
    meeting: &Meeting,
    agent: &MeetingParticipant,
    utterance_text: &str,
) -> String {
    let speaker_name = latest_human_speaker_name(meeting).unwrap_or("the human speaker");
    format!(
        "Agent name: {agent_name}\nHuman speaker name: {speaker_name}\nMeeting title: {title}\n\nHuman said:\n{utterance}\n\nReply as {agent_name} in one or two short sentences.",
        agent_name = agent.name,
        speaker_name = speaker_name,
        title = meeting.title,
        utterance = utterance_text.trim(),
    )
}

fn minutes_system_prompt() -> String {
    "You are the meeting moderator. Produce only valid JSON and nothing else. Summarize the decisions per agenda item based only on the meeting transcript. If no clear decision was made for an agenda item, return an empty decisions array for that item.".to_owned()
}

fn minutes_user_prompt(meeting: &Meeting) -> String {
    format!(
        "Meeting title: {title}\nOwner: {owner}\nModerator: {moderator}\nParticipants:\n{participants}\n\nAgenda:\n{agenda}\n\nTranscript:\n{transcript}\n\nReturn exactly this JSON object shape, preserving agenda order and phrase text:\n{{\"agenda\":[{{\"phrase\":\"<agenda phrase>\",\"decisions\":[\"<decision>\"]}}]}}",
        title = meeting.title,
        owner = meeting
            .owner()
            .map(|participant| participant.name.as_str())
            .unwrap_or("Unknown Owner"),
        moderator = meeting
            .moderator()
            .map(|participant| participant.name.as_str())
            .unwrap_or("Moderator"),
        participants = participant_lines(meeting),
        agenda = agenda_lines(meeting),
        transcript = full_utterance_lines(meeting),
    )
}

fn participant_lines(meeting: &Meeting) -> String {
    meeting
        .participants
        .iter()
        .map(|participant| {
            format!(
                "- {} ({:?}, {:?})",
                participant.name, participant.kind, participant.meeting_role
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn agenda_lines(meeting: &Meeting) -> String {
    meeting
        .agenda
        .iter()
        .map(|item| format!("- {}", item.phrase))
        .collect::<Vec<_>>()
        .join("\n")
}

fn full_utterance_lines(meeting: &Meeting) -> String {
    let lines = meeting
        .event_log
        .iter()
        .filter_map(|entry| utterance_line(meeting, entry))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "- No utterances recorded.".to_owned()
    } else {
        lines.join("\n")
    }
}

fn latest_human_speaker_name(meeting: &Meeting) -> Option<&str> {
    meeting
        .event_log
        .iter()
        .rev()
        .find_map(|entry| match &entry.event {
            MeetingEvent::HumanUtteranceRecorded { participant_id, .. } => meeting
                .participant(participant_id)
                .map(|participant| participant.name.as_str()),
            _ => None,
        })
}

fn utterance_line(
    meeting: &Meeting,
    entry: &wiab_core::meeting::MeetingEventLogEntry,
) -> Option<String> {
    match &entry.event {
        MeetingEvent::HumanUtteranceRecorded {
            participant_id,
            text,
            ..
        }
        | MeetingEvent::AgentUtteranceRecorded {
            participant_id,
            text,
            ..
        } => {
            let speaker_name = meeting
                .participant(participant_id)
                .map(|participant| participant.name.as_str())
                .unwrap_or(participant_id.as_str());
            Some(format!("- {}: {}", speaker_name, text.trim()))
        }
        _ => None,
    }
}

fn normalize_reply(raw: &str) -> anyhow::Result<String> {
    let normalized = raw.trim().trim_matches('\"').trim().to_owned();
    if normalized.is_empty() {
        bail!("llama reply normalized to an empty string");
    }
    Ok(normalized)
}

fn parse_minutes_agenda(meeting: &Meeting, raw: &str) -> anyhow::Result<Vec<MinutesAgendaItem>> {
    let json = extract_json_object(raw)
        .with_context(|| format!("llama minutes output did not contain a JSON object: {raw}"))?;
    let generated: GeneratedMinutesEnvelope =
        serde_json::from_str(json).context("failed to parse llama minutes JSON")?;

    if generated.agenda.len() != meeting.agenda.len() {
        bail!(
            "llama minutes returned {} agenda items but meeting requires {}",
            generated.agenda.len(),
            meeting.agenda.len()
        );
    }

    meeting
        .agenda
        .iter()
        .zip(generated.agenda)
        .map(|(expected, produced)| {
            if normalize_phrase(&expected.phrase) != normalize_phrase(&produced.phrase) {
                bail!(
                    "llama minutes changed agenda phrase '{}' to '{}'",
                    expected.phrase,
                    produced.phrase
                );
            }
            Ok(MinutesAgendaItem {
                agenda_item_id: expected.agenda_item_id.clone(),
                phrase: expected.phrase.clone(),
                decisions: produced
                    .decisions
                    .into_iter()
                    .map(|decision| decision.trim().to_owned())
                    .filter(|decision| !decision.is_empty())
                    .collect(),
            })
        })
        .collect()
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    raw.get(start..=end)
}

fn normalize_phrase(phrase: &str) -> String {
    phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn required_env(key: &str) -> anyhow::Result<String> {
    let value = std::env::var(key).with_context(|| format!("missing required env var {key}"))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("env var {key} must not be empty");
    }
    Ok(trimmed.to_owned())
}

fn optional_env_parse<T>(key: &str) -> anyhow::Result<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let Some(value) = std::env::var(key).ok() else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<T>()
        .map(Some)
        .map_err(|err| anyhow!("failed to parse {key}: {err}"))
}

fn default_threads() -> i32 {
    thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(4)
        .min(i32::MAX as usize) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiab_core::meeting::{AgendaItem, MeetingRole, MeetingState, ParticipantKind};

    #[test]
    fn extracts_json_object_from_wrapped_output() {
        let raw = "```json\n{\"agenda\":[]}\n```";
        assert_eq!(extract_json_object(raw), Some("{\"agenda\":[]}"));
    }

    #[test]
    fn parses_minutes_agenda_and_preserves_ids() {
        let meeting = Meeting {
            meeting_id: "meeting-1".to_owned(),
            title: "Test".to_owned(),
            state: MeetingState::Ended,
            owner_participant_id: "owner".to_owned(),
            moderator_participant_id: "moderator".to_owned(),
            participants: vec![
                MeetingParticipant {
                    participant_id: "owner".to_owned(),
                    kind: ParticipantKind::Human,
                    meeting_role: MeetingRole::Owner,
                    name: "Frederic".to_owned(),
                    instructions: None,
                    voice_id: None,
                },
                MeetingParticipant {
                    participant_id: "moderator".to_owned(),
                    kind: ParticipantKind::Agent,
                    meeting_role: MeetingRole::Moderator,
                    name: "Moderator".to_owned(),
                    instructions: Some("Moderate".to_owned()),
                    voice_id: Some("alloy".to_owned()),
                },
            ],
            agenda: vec![AgendaItem {
                agenda_item_id: "agenda-1".to_owned(),
                phrase: "review launch timeline".to_owned(),
            }],
            started_at: "2026-03-14T00:00:00Z".to_owned(),
            ended_at: Some("2026-03-14T00:00:10Z".to_owned()),
            event_log: Vec::new(),
            next_sequence_number: 1,
        };

        let parsed = parse_minutes_agenda(
            &meeting,
            "{\"agenda\":[{\"phrase\":\"review launch timeline\",\"decisions\":[\"Cut launch scope\"]}]}",
        )
        .expect("minutes JSON should parse");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].agenda_item_id, "agenda-1");
        assert_eq!(parsed[0].phrase, "review launch timeline");
        assert_eq!(parsed[0].decisions, vec!["Cut launch scope".to_owned()]);
    }
}
