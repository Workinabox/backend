use std::{collections::HashSet, sync::Arc};

use anyhow::{anyhow, bail};
use tokio::{sync::Mutex, task};
use tracing::{info, warn};
use uuid::Uuid;
use wiab_core::{
    meeting::{
        AgendaItem, Meeting, MeetingParticipant, MeetingRepository, MeetingRole, MeetingSnapshot,
        ParticipantKind,
    },
    meeting_traits::{Clock, MeetingIntelligence, MeetingIntelligenceError, SpeechSynthesizer},
    repository::{SaveError, Version},
};

use crate::{
    create_meeting_request::{CreateMeetingParticipant, CreateMeetingRequest},
    meeting_client_events::MeetingClientEvent,
};

const MODERATOR_NAME: &str = "Moderator";
const MODERATOR_INSTRUCTIONS: &str =
    "You are the meeting moderator. Control agent turn-taking and generate minutes.";
const MODERATOR_VOICE_ID: &str = "alloy";

#[derive(Clone)]
pub struct MeetingApplicationService<R: MeetingRepository> {
    meeting_repository: R,
    mutation_guard: Arc<Mutex<()>>,
    intelligence: Option<Arc<dyn MeetingIntelligence>>,
    speech: Arc<dyn SpeechSynthesizer>,
    clock: Arc<dyn Clock>,
}

impl<R: MeetingRepository> MeetingApplicationService<R> {
    pub fn new(
        meeting_repository: R,
        intelligence: Option<Arc<dyn MeetingIntelligence>>,
        speech: Arc<dyn SpeechSynthesizer>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            meeting_repository,
            mutation_guard: Arc::new(Mutex::new(())),
            intelligence,
            speech,
            clock,
        }
    }

    pub async fn list_meetings(&self) -> anyhow::Result<Vec<MeetingSnapshot>> {
        let mut meetings = self
            .meeting_repository
            .list()
            .await?
            .into_iter()
            .map(|meeting| meeting.snapshot())
            .collect::<Vec<_>>();
        meetings.sort_by(|left, right| left.title.cmp(&right.title));
        Ok(meetings)
    }

    pub async fn create_meeting(
        &self,
        request: CreateMeetingRequest,
    ) -> anyhow::Result<MeetingSnapshot> {
        let _guard = self.mutation_guard.lock().await;
        let meeting = build_meeting_from_request(request, self.clock.as_ref())?;
        let snapshot = meeting.snapshot();
        self.meeting_repository.save(meeting, Version::NEW).await?;
        Ok(snapshot)
    }

    pub async fn meeting_snapshot(
        &self,
        meeting_id: &str,
    ) -> anyhow::Result<Option<MeetingSnapshot>> {
        Ok(self
            .meeting_repository
            .get(meeting_id)
            .await?
            .map(|(meeting, _)| meeting.snapshot()))
    }

    pub async fn validate_join(
        &self,
        meeting_id: &str,
        participant_id: &str,
    ) -> anyhow::Result<MeetingSnapshot> {
        let (meeting, _) = self
            .meeting_repository
            .get(meeting_id)
            .await?
            .ok_or_else(|| anyhow!("meeting '{}' not found", meeting_id))?;
        meeting.require_active()?;
        meeting.require_participant(participant_id)?;
        Ok(meeting.snapshot())
    }

    pub async fn record_human_utterance(
        &self,
        meeting_id: &str,
        participant_id: &str,
        text: &str,
    ) -> anyhow::Result<Vec<MeetingClientEvent>> {
        let text = normalize_required(text, "transcript text")?;
        let _guard = self.mutation_guard.lock().await;
        let (mut meeting, version) = self.require_active_meeting(meeting_id).await?;
        meeting.require_human_participant(participant_id)?;
        if is_recent_agent_echo(&meeting, &text) {
            info!(
                "ignored transcript echo meeting='{}' participant='{}' text={}",
                meeting.meeting_id, participant_id, text
            );
            return Ok(Vec::new());
        }

        let directly_addressed_agent_ids = meeting.directly_addressed_agent_participant_ids(&text);
        let utterance_id = meeting.record_human_utterance(
            self.now(),
            participant_id,
            &text,
            directly_addressed_agent_ids.clone(),
        )?;

        let selected_agent = if directly_addressed_agent_ids.is_empty() {
            None
        } else {
            meeting
                .choose_directly_addressed_agent(&text, &directly_addressed_agent_ids)
                .map(|agent| (agent, "direct_address".to_owned()))
        };

        let mut events = Vec::new();
        if let Some((agent, reason)) = selected_agent {
            meeting.record_agent_turn_selected(
                self.now(),
                &agent.participant_id,
                &utterance_id,
                &reason,
            )?;

            let reply_text = match self
                .generate_agent_reply_blocking(meeting.clone(), agent.clone(), text.clone())
                .await
            {
                Ok(reply_text) => reply_text,
                Err(err) => {
                    warn!(
                        "agent reply generation failed meeting='{}' participant='{}' err={}",
                        meeting.meeting_id, agent.participant_id, err
                    );
                    self.persist(meeting, version).await?;
                    return Ok(events);
                }
            };
            let reply_utterance_id = meeting.record_agent_utterance(
                self.now(),
                &agent.participant_id,
                &reply_text,
                &utterance_id,
            )?;

            events.push(MeetingClientEvent::AgentText {
                meeting_id: meeting.meeting_id.clone(),
                participant_id: agent.participant_id.clone(),
                participant_name: agent.name.clone(),
                utterance_id: reply_utterance_id.clone(),
                text: reply_text.clone(),
            });

            match self.speech.synthesize(
                &reply_text,
                agent.voice_id.as_deref().unwrap_or(MODERATOR_VOICE_ID),
            ) {
                Ok(synthesized_clip) => {
                    events.push(MeetingClientEvent::AgentSpeech {
                        meeting_id: meeting.meeting_id.clone(),
                        participant_id: agent.participant_id.clone(),
                        participant_name: agent.name.clone(),
                        utterance_id: reply_utterance_id,
                        clip: synthesized_clip,
                    });
                }
                Err(err) => {
                    warn!(
                        "speech synthesis failed meeting='{}' participant='{}' err={}",
                        meeting.meeting_id, agent.participant_id, err
                    );
                }
            }
        }

        self.persist(meeting, version).await?;
        Ok(events)
    }

    pub async fn end_meeting(
        &self,
        meeting_id: &str,
        participant_id: &str,
    ) -> anyhow::Result<Vec<MeetingClientEvent>> {
        let _guard = self.mutation_guard.lock().await;
        let (mut meeting, version) = self.require_active_meeting(meeting_id).await?;
        let ended_at = self.now();
        meeting.end(ended_at.clone(), participant_id)?;

        let minutes = match self.generate_minutes_blocking(meeting.clone()).await {
            Ok(minutes) => {
                meeting.record_minutes_generated(self.now(), minutes.clone());
                Some(minutes)
            }
            Err(err) => {
                warn!(
                    "minutes generation failed meeting='{}' err={}",
                    meeting.meeting_id, err
                );
                None
            }
        };

        self.persist(meeting.clone(), version).await?;

        let mut events = vec![MeetingClientEvent::MeetingEnded {
            meeting_id: meeting.meeting_id.clone(),
            ended_by_participant_id: participant_id.to_owned(),
            ended_at,
        }];
        if let Some(minutes) = minutes {
            events.push(MeetingClientEvent::MinutesReady {
                meeting_id: meeting.meeting_id.clone(),
                minutes,
            });
        }

        Ok(events)
    }

    async fn require_active_meeting(&self, meeting_id: &str) -> anyhow::Result<(Meeting, Version)> {
        let (meeting, version) = self
            .meeting_repository
            .get(meeting_id)
            .await?
            .ok_or_else(|| anyhow!("meeting '{}' not found", meeting_id))?;
        meeting.require_active()?;
        Ok((meeting, version))
    }

    /// Persist a meeting under its loaded version. All meeting mutations are serialized by
    /// `mutation_guard`, so a conflict only arises against another process; surface it as an
    /// error rather than retrying, since the mutation has side effects (speech, minutes).
    async fn persist(&self, meeting: Meeting, expected: Version) -> anyhow::Result<()> {
        match self.meeting_repository.save(meeting, expected).await {
            Ok(_) => Ok(()),
            Err(SaveError::Conflict) => Err(anyhow!("meeting was modified concurrently")),
            Err(SaveError::Backend(error)) => Err(anyhow!(error)),
        }
    }

    fn now(&self) -> String {
        self.clock.now_rfc3339()
    }

    async fn generate_agent_reply_blocking(
        &self,
        meeting: Meeting,
        agent: MeetingParticipant,
        text: String,
    ) -> Result<String, MeetingIntelligenceError> {
        let Some(intelligence) = self.intelligence.clone() else {
            return Err(MeetingIntelligenceError::Message(
                "meeting intelligence disabled".to_owned(),
            ));
        };
        task::spawn_blocking(move || intelligence.generate_agent_reply(&meeting, &agent, &text))
            .await
            .map_err(|err| MeetingIntelligenceError::Message(format!("join error: {err}")))?
    }

    async fn generate_minutes_blocking(
        &self,
        meeting: Meeting,
    ) -> Result<wiab_core::meeting::MinutesDocument, MeetingIntelligenceError> {
        let Some(intelligence) = self.intelligence.clone() else {
            return Err(MeetingIntelligenceError::Message(
                "meeting intelligence disabled".to_owned(),
            ));
        };
        task::spawn_blocking(move || intelligence.generate_minutes(&meeting))
            .await
            .map_err(|err| MeetingIntelligenceError::Message(format!("join error: {err}")))?
    }
}

fn build_meeting_from_request(
    request: CreateMeetingRequest,
    clock: &dyn Clock,
) -> anyhow::Result<Meeting> {
    let title = normalize_required(&request.title, "meeting title")?;
    if request.agenda.is_empty() {
        bail!("agenda must contain at least one item");
    }

    let mut participants = Vec::new();

    let owner = build_participant_from_request(request.owner, MeetingRole::Owner)?;
    participants.push(owner.clone());

    let moderator = MeetingParticipant {
        participant_id: Uuid::new_v4().to_string(),
        kind: ParticipantKind::Agent,
        meeting_role: MeetingRole::Moderator,
        name: MODERATOR_NAME.to_owned(),
        instructions: Some(MODERATOR_INSTRUCTIONS.to_owned()),
        voice_id: Some(MODERATOR_VOICE_ID.to_owned()),
    };
    participants.push(moderator.clone());

    let owner_key = participant_key(&owner.kind, &owner.name);
    let mut seen_participant_keys = HashSet::from([owner_key]);
    for invited in request.invited_participants {
        let participant = build_participant_from_request(invited, MeetingRole::Participant)?;
        let participant_key = participant_key(&participant.kind, &participant.name);
        if !seen_participant_keys.insert(participant_key) {
            bail!("duplicate invited participant '{}'", participant.name);
        }
        participants.push(participant);
    }

    let agenda = request
        .agenda
        .into_iter()
        .map(|phrase| {
            Ok(AgendaItem {
                agenda_item_id: Uuid::new_v4().to_string(),
                phrase: normalize_required(&phrase, "agenda item")?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut meeting = Meeting::new(
        title,
        owner.participant_id.clone(),
        moderator.participant_id.clone(),
        participants,
        agenda,
        clock.now_rfc3339(),
    )?;
    meeting.record_created(clock.now_rfc3339());
    Ok(meeting)
}

fn build_participant_from_request(
    request: CreateMeetingParticipant,
    meeting_role: MeetingRole,
) -> anyhow::Result<MeetingParticipant> {
    match request {
        CreateMeetingParticipant::Human { name } => Ok(MeetingParticipant {
            participant_id: Uuid::new_v4().to_string(),
            kind: ParticipantKind::Human,
            meeting_role,
            name: normalize_required(&name, "participant name")?,
            instructions: None,
            voice_id: None,
        }),
        CreateMeetingParticipant::Agent {
            name,
            instructions,
            voice_id,
        } => {
            let name = normalize_required(&name, "agent name")?;
            if name.split_whitespace().count() != 1 {
                bail!("agent name '{}' must be a single word", name);
            }
            Ok(MeetingParticipant {
                participant_id: Uuid::new_v4().to_string(),
                kind: ParticipantKind::Agent,
                meeting_role,
                name,
                instructions: Some(normalize_required(&instructions, "agent instructions")?),
                voice_id: Some(normalize_required(&voice_id, "agent voice_id")?),
            })
        }
    }
}

fn participant_key(kind: &ParticipantKind, name: &str) -> String {
    format!("{kind:?}:{}", name.to_ascii_lowercase())
}

fn normalize_required(raw: &str, label: &str) -> anyhow::Result<String> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        bail!("{label} must be a non-empty trimmed string");
    }
    Ok(normalized.to_owned())
}

fn is_recent_agent_echo(meeting: &Meeting, transcript_text: &str) -> bool {
    let transcript_tokens = normalized_tokens(transcript_text);
    let normalized_transcript = transcript_tokens.join(" ");
    if normalized_transcript.is_empty() {
        return false;
    }

    for recent_agent_text in meeting.recent_agent_utterance_texts(3) {
        let agent_tokens = normalized_tokens(recent_agent_text);
        if agent_tokens.is_empty() {
            continue;
        }

        let normalized_agent = agent_tokens.join(" ");
        if normalized_transcript == normalized_agent {
            return true;
        }

        if transcript_tokens.len() < 4 || agent_tokens.len() < 4 {
            continue;
        }

        let overlap = transcript_tokens
            .iter()
            .filter(|token| agent_tokens.contains(*token))
            .count();
        let minimum_len = transcript_tokens.len().min(agent_tokens.len());
        if overlap * 5 >= minimum_len * 4 {
            return true;
        }
    }

    false
}

fn normalized_tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| token.trim_matches(|character: char| character.is_ascii_punctuation()))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wiab_core::{
        meeting::{MeetingRepository, MinutesAgendaItem, MinutesDocument, ParticipantKind},
        meeting_traits::{
            Clock, FloorRequestCandidate, MeetingIntelligence, SpeechClip, SpeechSynthesisError,
        },
        repository::RepoError,
    };

    #[derive(Default, Clone)]
    struct TestMeetingRepository {
        meetings: Arc<std::sync::RwLock<HashMap<String, (Meeting, u64)>>>,
    }

    impl MeetingRepository for TestMeetingRepository {
        async fn save(&self, meeting: Meeting, expected: Version) -> Result<Version, SaveError> {
            let mut meetings = self
                .meetings
                .write()
                .expect("test repository write lock poisoned");
            let current = meetings
                .get(&meeting.meeting_id)
                .map(|(_, version)| *version)
                .unwrap_or(0);
            if current != expected.value() {
                return Err(SaveError::Conflict);
            }
            let next = expected.next();
            meetings.insert(meeting.meeting_id.clone(), (meeting, next.value()));
            Ok(next)
        }

        async fn get(&self, meeting_id: &str) -> Result<Option<(Meeting, Version)>, RepoError> {
            Ok(self
                .meetings
                .read()
                .expect("test repository read lock poisoned")
                .get(meeting_id)
                .map(|(meeting, version)| (meeting.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Meeting>, RepoError> {
            Ok(self
                .meetings
                .read()
                .expect("test repository read lock poisoned")
                .values()
                .map(|(meeting, _)| meeting.clone())
                .collect())
        }
    }

    struct TestClock;

    impl Clock for TestClock {
        fn now_rfc3339(&self) -> String {
            "2026-03-14T00:00:00Z".to_owned()
        }
    }

    struct TestIntelligence;

    impl MeetingIntelligence for TestIntelligence {
        fn evaluate_floor_requests(
            &self,
            _meeting: &Meeting,
            _utterance_text: &str,
            _source_utterance_id: &str,
        ) -> Vec<FloorRequestCandidate> {
            Vec::new()
        }

        fn select_floor_request(
            &self,
            _meeting: &Meeting,
            _utterance_text: &str,
            _floor_requests: &[FloorRequestCandidate],
        ) -> Option<String> {
            None
        }

        fn generate_agent_reply(
            &self,
            _meeting: &Meeting,
            agent: &MeetingParticipant,
            _utterance_text: &str,
        ) -> Result<String, MeetingIntelligenceError> {
            Ok(format!("{} recommends reducing launch scope", agent.name))
        }

        fn generate_minutes(
            &self,
            meeting: &Meeting,
        ) -> Result<wiab_core::meeting::MinutesDocument, MeetingIntelligenceError> {
            Ok(MinutesDocument {
                meeting_id: meeting.meeting_id.clone(),
                title: meeting.title.clone(),
                owner_name: "Frederic".to_owned(),
                moderator_name: "Moderator".to_owned(),
                participants: meeting
                    .participants
                    .iter()
                    .map(MeetingParticipant::view)
                    .collect(),
                started_at: meeting.started_at.clone(),
                ended_at: meeting
                    .ended_at
                    .clone()
                    .unwrap_or_else(|| "2026-03-14T00:00:01Z".to_owned()),
                agenda: meeting
                    .agenda
                    .iter()
                    .map(|item| MinutesAgendaItem {
                        agenda_item_id: item.agenda_item_id.clone(),
                        phrase: item.phrase.clone(),
                        decisions: Vec::new(),
                    })
                    .collect(),
            })
        }
    }

    struct TestSpeechSynthesizer;

    impl SpeechSynthesizer for TestSpeechSynthesizer {
        fn synthesize(
            &self,
            _text: &str,
            _voice_id: &str,
        ) -> Result<SpeechClip, SpeechSynthesisError> {
            Ok(SpeechClip {
                mime_type: "audio/wav".to_owned(),
                audio_bytes: vec![1, 2, 3],
            })
        }
    }

    struct FailingSpeechSynthesizer;

    impl SpeechSynthesizer for FailingSpeechSynthesizer {
        fn synthesize(
            &self,
            _text: &str,
            _voice_id: &str,
        ) -> Result<SpeechClip, SpeechSynthesisError> {
            Err(SpeechSynthesisError::Message(
                "speech backend unavailable".to_owned(),
            ))
        }
    }

    #[tokio::test]
    async fn directly_addressed_agent_responds() {
        let repository = TestMeetingRepository::default();
        let service = MeetingApplicationService::new(
            repository.clone(),
            Some(Arc::new(TestIntelligence)),
            Arc::new(TestSpeechSynthesizer),
            Arc::new(TestClock),
        );
        let meeting = service
            .create_meeting(CreateMeetingRequest {
                title: "Test".to_owned(),
                owner: CreateMeetingParticipant::Human {
                    name: "Frederic".to_owned(),
                },
                invited_participants: vec![CreateMeetingParticipant::Agent {
                    name: "CTO".to_owned(),
                    instructions: "You are the CTO".to_owned(),
                    voice_id: "alloy".to_owned(),
                }],
                agenda: vec!["review launch timeline".to_owned()],
            })
            .await
            .expect("meeting should be created");

        let owner_id = meeting.owner_participant_id.clone();
        let events = service
            .record_human_utterance(
                &meeting.meeting_id,
                &owner_id,
                "CTO, what is the biggest risk here?",
            )
            .await
            .expect("transcript should be recorded");

        assert!(events.iter().any(|event| matches!(
            event,
            MeetingClientEvent::AgentText { participant_name, .. } if participant_name == "CTO"
        )));
    }

    #[tokio::test]
    async fn unaddressed_agent_does_not_respond() {
        let repository = TestMeetingRepository::default();
        let service = MeetingApplicationService::new(
            repository,
            Some(Arc::new(TestIntelligence)),
            Arc::new(TestSpeechSynthesizer),
            Arc::new(TestClock),
        );
        let meeting = service
            .create_meeting(CreateMeetingRequest {
                title: "Test".to_owned(),
                owner: CreateMeetingParticipant::Human {
                    name: "Frederic".to_owned(),
                },
                invited_participants: vec![CreateMeetingParticipant::Agent {
                    name: "Angela".to_owned(),
                    instructions: "You are Angela".to_owned(),
                    voice_id: "alloy".to_owned(),
                }],
                agenda: vec!["review launch timeline".to_owned()],
            })
            .await
            .expect("meeting should be created");

        let owner_id = meeting.owner_participant_id.clone();
        let events = service
            .record_human_utterance(&meeting.meeting_id, &owner_id, "What should we do next?")
            .await
            .expect("transcript should be recorded");

        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn only_owner_may_end_meeting() {
        let repository = TestMeetingRepository::default();
        let service = MeetingApplicationService::new(
            repository.clone(),
            Some(Arc::new(TestIntelligence)),
            Arc::new(TestSpeechSynthesizer),
            Arc::new(TestClock),
        );
        let meeting = service
            .create_meeting(CreateMeetingRequest {
                title: "Test".to_owned(),
                owner: CreateMeetingParticipant::Human {
                    name: "Frederic".to_owned(),
                },
                invited_participants: vec![CreateMeetingParticipant::Human {
                    name: "Alice".to_owned(),
                }],
                agenda: vec!["review".to_owned()],
            })
            .await
            .expect("meeting should be created");
        let alice_id = meeting
            .participants
            .iter()
            .find(|participant| {
                participant.kind == ParticipantKind::Human
                    && participant.meeting_role == MeetingRole::Participant
            })
            .map(|participant| participant.participant_id.clone())
            .expect("alice participant should exist");

        let error = service
            .end_meeting(&meeting.meeting_id, &alice_id)
            .await
            .expect_err("non-owner should not end meeting");
        assert!(error.to_string().contains("is not the meeting owner"));
    }

    #[tokio::test]
    async fn recent_agent_echo_is_ignored() {
        let repository = TestMeetingRepository::default();
        let service = MeetingApplicationService::new(
            repository,
            Some(Arc::new(TestIntelligence)),
            Arc::new(TestSpeechSynthesizer),
            Arc::new(TestClock),
        );
        let meeting = service
            .create_meeting(CreateMeetingRequest {
                title: "Test".to_owned(),
                owner: CreateMeetingParticipant::Human {
                    name: "Frederic".to_owned(),
                },
                invited_participants: vec![CreateMeetingParticipant::Agent {
                    name: "CTO".to_owned(),
                    instructions: "You are the CTO".to_owned(),
                    voice_id: "alloy".to_owned(),
                }],
                agenda: vec!["review launch timeline".to_owned()],
            })
            .await
            .expect("meeting should be created");

        let owner_id = meeting.owner_participant_id.clone();
        let first_events = service
            .record_human_utterance(
                &meeting.meeting_id,
                &owner_id,
                "CTO, what is the biggest risk here?",
            )
            .await
            .expect("first transcript should be recorded");

        let echoed_text = first_events
            .iter()
            .find_map(|event| match event {
                MeetingClientEvent::AgentText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .expect("first response should include agent text");

        let echoed_events = service
            .record_human_utterance(&meeting.meeting_id, &owner_id, &echoed_text)
            .await
            .expect("echoed transcript should be handled");

        assert!(echoed_events.is_empty());
    }

    #[tokio::test]
    async fn text_reply_survives_speech_synthesis_failure() {
        let repository = TestMeetingRepository::default();
        let service = MeetingApplicationService::new(
            repository,
            Some(Arc::new(TestIntelligence)),
            Arc::new(FailingSpeechSynthesizer),
            Arc::new(TestClock),
        );
        let meeting = service
            .create_meeting(CreateMeetingRequest {
                title: "Test".to_owned(),
                owner: CreateMeetingParticipant::Human {
                    name: "Frederic".to_owned(),
                },
                invited_participants: vec![CreateMeetingParticipant::Agent {
                    name: "CTO".to_owned(),
                    instructions: "You are the CTO".to_owned(),
                    voice_id: "alloy".to_owned(),
                }],
                agenda: vec!["review launch timeline".to_owned()],
            })
            .await
            .expect("meeting should be created");

        let owner_id = meeting.owner_participant_id.clone();
        let events = service
            .record_human_utterance(
                &meeting.meeting_id,
                &owner_id,
                "CTO, what is the biggest risk here?",
            )
            .await
            .expect("transcript should be recorded");

        assert!(events.iter().any(|event| matches!(
            event,
            MeetingClientEvent::AgentText { participant_name, .. } if participant_name == "CTO"
        )));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, MeetingClientEvent::AgentSpeech { .. }))
        );
    }

    #[tokio::test]
    async fn disabled_intelligence_yields_no_agent_reply_or_minutes() {
        let repository = TestMeetingRepository::default();
        let service = MeetingApplicationService::new(
            repository,
            None,
            Arc::new(TestSpeechSynthesizer),
            Arc::new(TestClock),
        );
        let meeting = service
            .create_meeting(CreateMeetingRequest {
                title: "Test".to_owned(),
                owner: CreateMeetingParticipant::Human {
                    name: "Frederic".to_owned(),
                },
                invited_participants: vec![CreateMeetingParticipant::Agent {
                    name: "CTO".to_owned(),
                    instructions: "You are the CTO".to_owned(),
                    voice_id: "alloy".to_owned(),
                }],
                agenda: vec!["review launch timeline".to_owned()],
            })
            .await
            .expect("meeting should be created");

        let owner_id = meeting.owner_participant_id.clone();
        let events = service
            .record_human_utterance(
                &meeting.meeting_id,
                &owner_id,
                "CTO, what is the biggest risk here?",
            )
            .await
            .expect("transcript should be recorded");
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, MeetingClientEvent::AgentText { .. }))
        );

        let end_events = service
            .end_meeting(&meeting.meeting_id, &owner_id)
            .await
            .expect("owner should end meeting");
        assert!(
            end_events
                .iter()
                .any(|event| matches!(event, MeetingClientEvent::MeetingEnded { .. }))
        );
        assert!(
            !end_events
                .iter()
                .any(|event| matches!(event, MeetingClientEvent::MinutesReady { .. }))
        );
    }
}
