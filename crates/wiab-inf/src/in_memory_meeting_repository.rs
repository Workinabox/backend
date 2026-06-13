use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::meeting::{
    AgendaItem, Meeting, MeetingParticipant, MeetingRepository, MeetingRole, ParticipantKind,
};
use wiab_core::repository::{RepoError, SaveError, Version};

#[derive(Debug, Clone, Default)]
pub struct InMemoryMeetingRepository {
    meetings: Arc<RwLock<HashMap<String, (Meeting, u64)>>>,
}

impl InMemoryMeetingRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_seed_data(now: impl Fn() -> String) -> Self {
        let repository = Self::new();

        // Seed at construction time (no concurrency): insert the meeting directly at
        // version 1 rather than going through the async `save`.
        let meeting = seeded_meeting(
            "Angela Meeting",
            "Frederic",
            &[SeedParticipant::agent(
                "Angela",
                "You are Angela. Focus on clear, practical, well-reasoned engineering guidance.",
                "alloy",
            )],
            &["decide the most important next step"],
            &now,
        );
        repository
            .meetings
            .write()
            .expect("meeting repository write lock poisoned")
            .insert(meeting.meeting_id.clone(), (meeting, 1));

        repository
    }
}

impl MeetingRepository for InMemoryMeetingRepository {
    async fn save(&self, meeting: Meeting, expected: Version) -> Result<Version, SaveError> {
        let mut meetings = self
            .meetings
            .write()
            .expect("meeting repository write lock poisoned");
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
            .expect("meeting repository read lock poisoned")
            .get(meeting_id)
            .map(|(meeting, version)| (meeting.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Meeting>, RepoError> {
        Ok(self
            .meetings
            .read()
            .expect("meeting repository read lock poisoned")
            .values()
            .map(|(meeting, _)| meeting.clone())
            .collect())
    }
}

struct SeedParticipant<'a> {
    kind: ParticipantKind,
    name: &'a str,
    instructions: Option<&'a str>,
    voice_id: Option<&'a str>,
}

impl<'a> SeedParticipant<'a> {
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

    let mut meeting = Meeting::new(
        title.to_owned(),
        owner.participant_id.clone(),
        moderator.participant_id.clone(),
        participants,
        agenda,
        now(),
    )
    .expect("seed meeting should be valid");
    meeting.record_created(now());
    meeting
}
