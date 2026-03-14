use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::meeting::{
    AgendaItem, Meeting, MeetingParticipant, MeetingRepository, MeetingRole, MeetingState,
    ParticipantKind,
};

#[derive(Debug, Clone, Default)]
pub struct InMemoryMeetingRepository {
    meetings: Arc<RwLock<HashMap<String, Meeting>>>,
}

impl InMemoryMeetingRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_seed_data(now: impl Fn() -> String) -> Self {
        let repository = Self::new();

        for meeting in [
            seeded_meeting(
                "Leadership Sync",
                "Frederic",
                &[
                    SeedParticipant::human("Alice"),
                    SeedParticipant::agent(
                        "CTO",
                        "You are the CTO. Focus on technical risk, sequencing, and staffing.",
                        "alloy",
                    ),
                    SeedParticipant::agent(
                        "PM",
                        "You are the product manager. Focus on scope, sequencing, and delivery risks.",
                        "alloy",
                    ),
                ],
                &[
                    "review launch timeline",
                    "decide hiring priorities",
                    "assign follow-up tasks",
                ],
                &now,
            ),
            seeded_meeting(
                "Design Review",
                "Frederic",
                &[
                    SeedParticipant::human("Alice"),
                    SeedParticipant::agent(
                        "Designer",
                        "You are the product designer. Focus on usability, clarity, and adoption risk.",
                        "alloy",
                    ),
                    SeedParticipant::agent(
                        "Engineer",
                        "You are the lead engineer. Focus on implementation cost and technical tradeoffs.",
                        "alloy",
                    ),
                ],
                &["review onboarding flow", "decide MVP scope"],
                &now,
            ),
            seeded_meeting(
                "Townhall Prep",
                "Frederic",
                &[
                    SeedParticipant::agent(
                        "COO",
                        "You are the COO. Focus on operational execution and communications.",
                        "alloy",
                    ),
                    SeedParticipant::agent(
                        "CFO",
                        "You are the CFO. Focus on budget, tradeoffs, and financial risk.",
                        "alloy",
                    ),
                ],
                &["draft key messages", "decide open questions for the team"],
                &now,
            ),
        ] {
            repository.save(meeting);
        }

        repository
    }
}

impl MeetingRepository for InMemoryMeetingRepository {
    fn save(&self, meeting: Meeting) {
        self.meetings
            .write()
            .expect("meeting repository write lock poisoned")
            .insert(meeting.meeting_id.clone(), meeting);
    }

    fn get(&self, meeting_id: &str) -> Option<Meeting> {
        self.meetings
            .read()
            .expect("meeting repository read lock poisoned")
            .get(meeting_id)
            .cloned()
    }

    fn list(&self) -> Vec<Meeting> {
        self.meetings
            .read()
            .expect("meeting repository read lock poisoned")
            .values()
            .cloned()
            .collect()
    }
}

struct SeedParticipant<'a> {
    kind: ParticipantKind,
    name: &'a str,
    instructions: Option<&'a str>,
    voice_id: Option<&'a str>,
}

impl<'a> SeedParticipant<'a> {
    fn human(name: &'a str) -> Self {
        Self {
            kind: ParticipantKind::Human,
            name,
            instructions: None,
            voice_id: None,
        }
    }

    fn agent(name: &'a str, instructions: &'a str, voice_id: &'a str) -> Self {
        Self {
            kind: ParticipantKind::Agent,
            name,
            instructions: Some(instructions),
            voice_id: Some(voice_id),
        }
    }
}

fn seeded_meeting(
    title: &str,
    owner_name: &str,
    invited: &[SeedParticipant<'_>],
    agenda: &[&str],
    now: &impl Fn() -> String,
) -> Meeting {
    let owner = MeetingParticipant {
        participant_id: uuid::Uuid::new_v4().to_string(),
        kind: ParticipantKind::Human,
        meeting_role: MeetingRole::Owner,
        name: owner_name.to_owned(),
        instructions: None,
        voice_id: None,
    };
    let moderator = MeetingParticipant {
        participant_id: uuid::Uuid::new_v4().to_string(),
        kind: ParticipantKind::Agent,
        meeting_role: MeetingRole::Moderator,
        name: "Moderator".to_owned(),
        instructions: Some(
            "You are the meeting moderator. Control agent turn-taking and produce minutes."
                .to_owned(),
        ),
        voice_id: Some("alloy".to_owned()),
    };

    let mut participants = vec![owner.clone(), moderator.clone()];
    for participant in invited {
        participants.push(MeetingParticipant {
            participant_id: uuid::Uuid::new_v4().to_string(),
            kind: participant.kind,
            meeting_role: MeetingRole::Participant,
            name: participant.name.to_owned(),
            instructions: participant.instructions.map(str::to_owned),
            voice_id: participant.voice_id.map(str::to_owned),
        });
    }

    let agenda = agenda
        .iter()
        .map(|phrase| AgendaItem {
            agenda_item_id: uuid::Uuid::new_v4().to_string(),
            phrase: (*phrase).to_owned(),
        })
        .collect();

    Meeting {
        meeting_id: uuid::Uuid::new_v4().to_string(),
        title: title.to_owned(),
        state: MeetingState::Active,
        owner_participant_id: owner.participant_id.clone(),
        moderator_participant_id: moderator.participant_id.clone(),
        participants,
        agenda,
        started_at: now(),
        ended_at: None,
        event_log: Vec::new(),
        next_sequence_number: 1,
    }
}
