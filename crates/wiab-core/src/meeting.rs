use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::agent::FloorRequestCandidate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantKind {
    Human,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingRole {
    Owner,
    Moderator,
    Participant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingState {
    Active,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MeetingError {
    #[error("meeting title must be a non-empty trimmed string")]
    EmptyTitle,
    #[error("agenda must contain at least one item")]
    EmptyAgenda,
    #[error("meeting owner '{0}' not found in participants")]
    OwnerNotFound(String),
    #[error("owner '{0}' must have owner role")]
    OwnerRoleMismatch(String),
    #[error("meeting moderator '{0}' not found in participants")]
    ModeratorNotFound(String),
    #[error("moderator '{0}' must have moderator role")]
    ModeratorRoleMismatch(String),
    #[error("moderator '{0}' must be an agent participant")]
    ModeratorNotAgent(String),
    #[error("participant ids must be unique")]
    DuplicateParticipantIds,
    #[error("duplicate agent name '{0}'")]
    DuplicateAgentName(String),
    #[error("participant '{0}' does not belong to meeting")]
    ParticipantNotFound(String),
    #[error("participant '{0}' is not a human participant")]
    ParticipantNotHuman(String),
    #[error("participant '{0}' is not an agent participant")]
    ParticipantNotAgent(String),
    #[error("participant '{0}' is not the meeting owner")]
    ParticipantNotOwner(String),
    #[error("meeting '{0}' has already ended")]
    Inactive(String),
    #[error("utterance text must be a non-empty trimmed string")]
    EmptyUtterance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingParticipant {
    pub participant_id: String,
    pub kind: ParticipantKind,
    pub meeting_role: MeetingRole,
    pub name: String,
    pub instructions: Option<String>,
    pub voice_id: Option<String>,
}

impl MeetingParticipant {
    pub fn view(&self) -> ParticipantView {
        ParticipantView {
            participant_id: self.participant_id.clone(),
            kind: self.kind,
            meeting_role: self.meeting_role,
            name: self.name.clone(),
        }
    }

    pub fn is_human(&self) -> bool {
        self.kind == ParticipantKind::Human
    }

    pub fn is_agent(&self) -> bool {
        self.kind == ParticipantKind::Agent
    }

    pub fn is_moderator(&self) -> bool {
        self.meeting_role == MeetingRole::Moderator
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantView {
    pub participant_id: String,
    pub kind: ParticipantKind,
    pub meeting_role: MeetingRole,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaItem {
    pub agenda_item_id: String,
    pub phrase: String,
}

impl AgendaItem {
    pub fn view(&self) -> AgendaItemView {
        AgendaItemView {
            agenda_item_id: self.agenda_item_id.clone(),
            phrase: self.phrase.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgendaItemView {
    pub agenda_item_id: String,
    pub phrase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinutesAgendaItem {
    pub agenda_item_id: String,
    pub phrase: String,
    pub decisions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinutesDocument {
    pub meeting_id: String,
    pub title: String,
    pub owner_name: String,
    pub moderator_name: String,
    pub participants: Vec<ParticipantView>,
    pub started_at: String,
    pub ended_at: String,
    pub agenda: Vec<MinutesAgendaItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingSnapshot {
    pub meeting_id: String,
    pub title: String,
    pub state: MeetingState,
    pub owner_participant_id: String,
    pub moderator_participant_id: String,
    pub participants: Vec<ParticipantView>,
    pub agenda: Vec<AgendaItemView>,
    pub started_at: String,
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingEventLogEntry {
    pub event_id: String,
    pub meeting_id: String,
    pub sequence_number: u64,
    pub recorded_at: String,
    pub event: MeetingEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MeetingEvent {
    MeetingCreated {
        meeting: MeetingSnapshot,
    },
    HumanUtteranceRecorded {
        utterance_id: String,
        participant_id: String,
        text: String,
        directly_addressed_agent_participant_ids: Vec<String>,
    },
    AgentFloorRequested {
        floor_request_id: String,
        participant_id: String,
        source_utterance_id: String,
    },
    AgentFloorDecision {
        floor_request_id: String,
        participant_id: String,
        granted: bool,
    },
    AgentTurnSelected {
        participant_id: String,
        source_utterance_id: String,
        reason: String,
    },
    AgentUtteranceRecorded {
        utterance_id: String,
        participant_id: String,
        text: String,
        source_utterance_id: String,
    },
    MeetingEnded {
        ended_by_participant_id: String,
    },
    MinutesGenerated {
        minutes: MinutesDocument,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meeting {
    pub meeting_id: String,
    pub title: String,
    pub state: MeetingState,
    pub owner_participant_id: String,
    pub moderator_participant_id: String,
    pub participants: Vec<MeetingParticipant>,
    pub agenda: Vec<AgendaItem>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub event_log: Vec<MeetingEventLogEntry>,
    pub next_sequence_number: u64,
}

impl Meeting {
    pub fn new(
        title: String,
        owner_participant_id: String,
        moderator_participant_id: String,
        participants: Vec<MeetingParticipant>,
        agenda: Vec<AgendaItem>,
        started_at: String,
    ) -> Result<Self, MeetingError> {
        if title.trim().is_empty() {
            return Err(MeetingError::EmptyTitle);
        }
        if agenda.is_empty() {
            return Err(MeetingError::EmptyAgenda);
        }

        let mut participant_ids = HashSet::new();
        let mut agent_names = HashSet::new();
        let mut owner_seen = false;
        let mut moderator_seen = false;

        for participant in &participants {
            if !participant_ids.insert(participant.participant_id.clone()) {
                return Err(MeetingError::DuplicateParticipantIds);
            }

            if participant.participant_id == owner_participant_id {
                owner_seen = true;
                if participant.meeting_role != MeetingRole::Owner {
                    return Err(MeetingError::OwnerRoleMismatch(
                        owner_participant_id.clone(),
                    ));
                }
            }

            if participant.participant_id == moderator_participant_id {
                moderator_seen = true;
                if participant.meeting_role != MeetingRole::Moderator {
                    return Err(MeetingError::ModeratorRoleMismatch(
                        moderator_participant_id.clone(),
                    ));
                }
                if !participant.is_agent() {
                    return Err(MeetingError::ModeratorNotAgent(
                        moderator_participant_id.clone(),
                    ));
                }
            }

            if participant.is_agent() {
                let normalized_name = participant.name.to_ascii_lowercase();
                if !agent_names.insert(normalized_name) {
                    return Err(MeetingError::DuplicateAgentName(participant.name.clone()));
                }
            }
        }

        if !owner_seen {
            return Err(MeetingError::OwnerNotFound(owner_participant_id));
        }
        if !moderator_seen {
            return Err(MeetingError::ModeratorNotFound(moderator_participant_id));
        }

        Ok(Self {
            meeting_id: Uuid::new_v4().to_string(),
            title,
            state: MeetingState::Active,
            owner_participant_id,
            moderator_participant_id,
            participants,
            agenda,
            started_at,
            ended_at: None,
            event_log: Vec::new(),
            next_sequence_number: 1,
        })
    }

    pub fn snapshot(&self) -> MeetingSnapshot {
        MeetingSnapshot {
            meeting_id: self.meeting_id.clone(),
            title: self.title.clone(),
            state: self.state,
            owner_participant_id: self.owner_participant_id.clone(),
            moderator_participant_id: self.moderator_participant_id.clone(),
            participants: self
                .participants
                .iter()
                .map(MeetingParticipant::view)
                .collect(),
            agenda: self.agenda.iter().map(AgendaItem::view).collect(),
            started_at: self.started_at.clone(),
            ended_at: self.ended_at.clone(),
        }
    }

    pub fn participant(&self, participant_id: &str) -> Option<&MeetingParticipant> {
        self.participants
            .iter()
            .find(|participant| participant.participant_id == participant_id)
    }

    pub fn agent_participants(&self) -> impl Iterator<Item = &MeetingParticipant> {
        self.participants
            .iter()
            .filter(|participant| participant.is_agent())
    }

    pub fn non_moderator_agent_participants(&self) -> impl Iterator<Item = &MeetingParticipant> {
        self.participants
            .iter()
            .filter(|participant| participant.is_agent() && !participant.is_moderator())
    }

    pub fn owner(&self) -> Option<&MeetingParticipant> {
        self.participant(&self.owner_participant_id)
    }

    pub fn moderator(&self) -> Option<&MeetingParticipant> {
        self.participant(&self.moderator_participant_id)
    }

    pub fn recent_agent_utterance_texts(&self, limit: usize) -> Vec<&str> {
        self.event_log
            .iter()
            .rev()
            .filter_map(|entry| match &entry.event {
                MeetingEvent::AgentUtteranceRecorded { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .take(limit)
            .collect()
    }

    pub fn require_active(&self) -> Result<(), MeetingError> {
        if self.state != MeetingState::Active {
            return Err(MeetingError::Inactive(self.meeting_id.clone()));
        }
        Ok(())
    }

    pub fn require_participant(
        &self,
        participant_id: &str,
    ) -> Result<&MeetingParticipant, MeetingError> {
        self.participant(participant_id)
            .ok_or_else(|| MeetingError::ParticipantNotFound(participant_id.to_owned()))
    }

    pub fn require_human_participant(
        &self,
        participant_id: &str,
    ) -> Result<&MeetingParticipant, MeetingError> {
        let participant = self.require_participant(participant_id)?;
        if !participant.is_human() {
            return Err(MeetingError::ParticipantNotHuman(participant_id.to_owned()));
        }
        Ok(participant)
    }

    pub fn require_agent_participant(
        &self,
        participant_id: &str,
    ) -> Result<&MeetingParticipant, MeetingError> {
        let participant = self.require_participant(participant_id)?;
        if !participant.is_agent() {
            return Err(MeetingError::ParticipantNotAgent(participant_id.to_owned()));
        }
        Ok(participant)
    }

    pub fn require_owner(&self, participant_id: &str) -> Result<(), MeetingError> {
        self.require_participant(participant_id)?;
        if self.owner_participant_id != participant_id {
            return Err(MeetingError::ParticipantNotOwner(participant_id.to_owned()));
        }
        Ok(())
    }

    pub fn directly_addressed_agent_participant_ids(&self, utterance_text: &str) -> Vec<String> {
        let tokens = normalized_tokens(utterance_text);
        self.non_moderator_agent_participants()
            .filter(|agent| {
                tokens
                    .iter()
                    .any(|token| token == &agent.name.to_ascii_lowercase())
            })
            .map(|agent| agent.participant_id.clone())
            .collect()
    }

    pub fn choose_directly_addressed_agent(
        &self,
        utterance_text: &str,
        candidate_ids: &[String],
    ) -> Option<MeetingParticipant> {
        let candidates = candidate_ids.iter().cloned().collect::<HashSet<_>>();
        let tokens = normalized_tokens(utterance_text);

        for token in tokens {
            if let Some(agent) = self
                .non_moderator_agent_participants()
                .find(|agent| {
                    candidates.contains(&agent.participant_id)
                        && token == agent.name.to_ascii_lowercase()
                })
                .cloned()
            {
                return Some(agent);
            }
        }

        self.non_moderator_agent_participants()
            .find(|agent| candidates.contains(&agent.participant_id))
            .cloned()
    }

    pub fn record_created(&mut self, recorded_at: String) {
        let snapshot = self.snapshot();
        self.append_event(
            recorded_at,
            MeetingEvent::MeetingCreated { meeting: snapshot },
        );
    }

    pub fn record_human_utterance(
        &mut self,
        recorded_at: String,
        participant_id: &str,
        text: &str,
        directly_addressed_agent_participant_ids: Vec<String>,
    ) -> Result<String, MeetingError> {
        self.require_active()?;
        let participant_id = self
            .require_human_participant(participant_id)?
            .participant_id
            .clone();
        let text = normalize_non_empty(text)?;
        let utterance_id = Uuid::new_v4().to_string();
        self.append_event(
            recorded_at,
            MeetingEvent::HumanUtteranceRecorded {
                utterance_id: utterance_id.clone(),
                participant_id,
                text,
                directly_addressed_agent_participant_ids,
            },
        );
        Ok(utterance_id)
    }

    pub fn record_floor_requests(
        &mut self,
        recorded_at: String,
        source_utterance_id: &str,
        floor_requests: &[FloorRequestCandidate],
    ) -> Result<(), MeetingError> {
        for floor_request in floor_requests {
            let participant_id = self
                .require_agent_participant(&floor_request.participant_id)?
                .participant_id
                .clone();
            self.append_event(
                recorded_at.clone(),
                MeetingEvent::AgentFloorRequested {
                    floor_request_id: floor_request.floor_request_id.clone(),
                    participant_id,
                    source_utterance_id: source_utterance_id.to_owned(),
                },
            );
        }
        Ok(())
    }

    pub fn record_floor_decisions(
        &mut self,
        recorded_at: String,
        floor_requests: &[FloorRequestCandidate],
        granted_participant_id: Option<&str>,
    ) -> Result<(), MeetingError> {
        for floor_request in floor_requests {
            let participant_id = self
                .require_agent_participant(&floor_request.participant_id)?
                .participant_id
                .clone();
            self.append_event(
                recorded_at.clone(),
                MeetingEvent::AgentFloorDecision {
                    floor_request_id: floor_request.floor_request_id.clone(),
                    participant_id: participant_id.clone(),
                    granted: granted_participant_id == Some(participant_id.as_str()),
                },
            );
        }
        Ok(())
    }

    pub fn record_agent_turn_selected(
        &mut self,
        recorded_at: String,
        participant_id: &str,
        source_utterance_id: &str,
        reason: &str,
    ) -> Result<(), MeetingError> {
        let participant_id = self
            .require_agent_participant(participant_id)?
            .participant_id
            .clone();
        self.append_event(
            recorded_at,
            MeetingEvent::AgentTurnSelected {
                participant_id,
                source_utterance_id: source_utterance_id.to_owned(),
                reason: reason.to_owned(),
            },
        );
        Ok(())
    }

    pub fn record_agent_utterance(
        &mut self,
        recorded_at: String,
        participant_id: &str,
        text: &str,
        source_utterance_id: &str,
    ) -> Result<String, MeetingError> {
        let participant_id = self
            .require_agent_participant(participant_id)?
            .participant_id
            .clone();
        let text = normalize_non_empty(text)?;
        let utterance_id = Uuid::new_v4().to_string();
        self.append_event(
            recorded_at,
            MeetingEvent::AgentUtteranceRecorded {
                utterance_id: utterance_id.clone(),
                participant_id,
                text,
                source_utterance_id: source_utterance_id.to_owned(),
            },
        );
        Ok(utterance_id)
    }

    pub fn end(
        &mut self,
        recorded_at: String,
        ended_by_participant_id: &str,
    ) -> Result<(), MeetingError> {
        self.require_active()?;
        self.require_owner(ended_by_participant_id)?;
        self.state = MeetingState::Ended;
        self.ended_at = Some(recorded_at.clone());
        self.append_event(
            recorded_at,
            MeetingEvent::MeetingEnded {
                ended_by_participant_id: ended_by_participant_id.to_owned(),
            },
        );
        Ok(())
    }

    pub fn record_minutes_generated(&mut self, recorded_at: String, minutes: MinutesDocument) {
        self.append_event(recorded_at, MeetingEvent::MinutesGenerated { minutes });
    }

    pub fn append_event(&mut self, recorded_at: String, event: MeetingEvent) {
        self.event_log.push(MeetingEventLogEntry {
            event_id: Uuid::new_v4().to_string(),
            meeting_id: self.meeting_id.clone(),
            sequence_number: self.next_sequence_number,
            recorded_at,
            event,
        });
        self.next_sequence_number += 1;
    }
}

fn normalize_non_empty(raw: &str) -> Result<String, MeetingError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(MeetingError::EmptyUtterance);
    }
    Ok(normalized.to_owned())
}

fn normalized_tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(trim_ascii_punctuation)
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn trim_ascii_punctuation(token: &str) -> &str {
    token.trim_matches(|character: char| character.is_ascii_punctuation())
}

pub trait MeetingRepository: Send + Sync + 'static {
    fn save(&self, meeting: Meeting);
    fn get(&self, meeting_id: &str) -> Option<Meeting>;
    fn list(&self) -> Vec<Meeting>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_rejects_duplicate_agent_names() {
        let owner = human_participant("owner-1", MeetingRole::Owner, "Frederic");
        let moderator = agent_participant("agent-1", MeetingRole::Moderator, "Moderator");
        let invited = agent_participant("agent-2", MeetingRole::Participant, "CTO");
        let duplicate = agent_participant("agent-3", MeetingRole::Participant, "cto");

        let error = Meeting::new(
            "Leadership".to_owned(),
            owner.participant_id.clone(),
            moderator.participant_id.clone(),
            vec![owner, moderator, invited, duplicate],
            vec![agenda_item("agenda-1", "review launch")],
            "2026-03-14T08:00:00Z".to_owned(),
        )
        .expect_err("duplicate agent names should be rejected");

        assert_eq!(error, MeetingError::DuplicateAgentName("cto".to_owned()));
    }

    #[test]
    fn directly_addressed_agent_uses_exact_name_match_order() {
        let meeting = sample_meeting();
        let direct_ids = meeting.directly_addressed_agent_participant_ids("CTO and PM, respond");

        let selected = meeting
            .choose_directly_addressed_agent("CTO and PM, respond", &direct_ids)
            .expect("a directly addressed agent should be selected");

        assert_eq!(selected.name, "CTO");
    }

    #[test]
    fn ending_meeting_requires_owner() {
        let mut meeting = sample_meeting();
        let pm_id = meeting
            .participants
            .iter()
            .find(|participant| participant.name == "PM")
            .map(|participant| participant.participant_id.clone())
            .expect("pm should exist");

        let error = meeting
            .end("2026-03-14T09:00:00Z".to_owned(), &pm_id)
            .expect_err("non-owner should not be allowed to end meeting");

        assert_eq!(error, MeetingError::ParticipantNotOwner(pm_id));
    }

    #[test]
    fn recording_agent_utterance_requires_agent_participant() {
        let mut meeting = sample_meeting();

        let error = meeting
            .record_agent_utterance(
                "2026-03-14T08:05:00Z".to_owned(),
                &meeting.owner_participant_id.clone(),
                "I have thoughts.",
                "utterance-1",
            )
            .expect_err("human owner should not be treated as an agent speaker");

        assert_eq!(
            error,
            MeetingError::ParticipantNotAgent(meeting.owner_participant_id.clone())
        );
    }

    fn sample_meeting() -> Meeting {
        let owner = human_participant("owner-1", MeetingRole::Owner, "Frederic");
        let moderator = agent_participant("agent-1", MeetingRole::Moderator, "Moderator");
        let cto = agent_participant("agent-2", MeetingRole::Participant, "CTO");
        let pm = agent_participant("agent-3", MeetingRole::Participant, "PM");

        Meeting::new(
            "Leadership".to_owned(),
            owner.participant_id.clone(),
            moderator.participant_id.clone(),
            vec![owner, moderator, cto, pm],
            vec![agenda_item("agenda-1", "review launch")],
            "2026-03-14T08:00:00Z".to_owned(),
        )
        .expect("sample meeting should be valid")
    }

    fn human_participant(id: &str, role: MeetingRole, name: &str) -> MeetingParticipant {
        MeetingParticipant {
            participant_id: id.to_owned(),
            kind: ParticipantKind::Human,
            meeting_role: role,
            name: name.to_owned(),
            instructions: None,
            voice_id: None,
        }
    }

    fn agent_participant(id: &str, role: MeetingRole, name: &str) -> MeetingParticipant {
        MeetingParticipant {
            participant_id: id.to_owned(),
            kind: ParticipantKind::Agent,
            meeting_role: role,
            name: name.to_owned(),
            instructions: Some(format!("{name} instructions")),
            voice_id: Some("alloy".to_owned()),
        }
    }

    fn agenda_item(id: &str, phrase: &str) -> AgendaItem {
        AgendaItem {
            agenda_item_id: id.to_owned(),
            phrase: phrase.to_owned(),
        }
    }
}
