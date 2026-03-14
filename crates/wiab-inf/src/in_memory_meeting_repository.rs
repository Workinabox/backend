use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::meeting::{
    AgendaItem, Meeting, MeetingParticipant, MeetingRepository, MeetingRole, ParticipantKind,
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

        repository.save(seeded_meeting(
            "Angela Meeting",
            "Frederic",
            &[SeedParticipant::agent(
                "Angela",
                "You are Angela. Focus on clear, practical, well-reasoned engineering guidance.",
                "alloy",
            )],
            &["decide the most important next step"],
            &now,
        ));

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
