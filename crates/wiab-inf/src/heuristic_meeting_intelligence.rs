use std::collections::HashSet;

use uuid::Uuid;
use wiab_core::{
    agent::{FloorRequestCandidate, MeetingIntelligence},
    meeting::{
        AgendaItem, Meeting, MeetingEvent, MeetingParticipant, MinutesAgendaItem, MinutesDocument,
    },
};

pub struct HeuristicMeetingIntelligence;

impl MeetingIntelligence for HeuristicMeetingIntelligence {
    fn evaluate_floor_requests(
        &self,
        meeting: &Meeting,
        utterance_text: &str,
        _source_utterance_id: &str,
    ) -> Vec<FloorRequestCandidate> {
        let utterance_tokens = token_set(utterance_text);
        let asks_for_input = utterance_text.contains('?')
            || utterance_tokens.contains("anyone")
            || utterance_tokens.contains("thoughts")
            || utterance_tokens.contains("perspective")
            || utterance_tokens.contains("view");

        meeting
            .non_moderator_agent_participants()
            .filter_map(|agent| {
                let score = overlap_score(utterance_text, agent);
                if score == 0 && !asks_for_input {
                    return None;
                }
                Some(FloorRequestCandidate {
                    floor_request_id: Uuid::new_v4().to_string(),
                    participant_id: agent.participant_id.clone(),
                    score: score + usize::from(asks_for_input),
                })
            })
            .collect()
    }

    fn select_floor_request(
        &self,
        meeting: &Meeting,
        utterance_text: &str,
        floor_requests: &[FloorRequestCandidate],
    ) -> Option<String> {
        let mut ranked = floor_requests
            .iter()
            .map(|request| {
                let agent = meeting.participant(&request.participant_id);
                let tie_breaker = agent
                    .map(|participant| overlap_score(utterance_text, participant))
                    .unwrap_or_default();
                (request.participant_id.clone(), request.score, tie_breaker)
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then(right.2.cmp(&left.2))
                .then(left.0.cmp(&right.0))
        });
        ranked
            .first()
            .map(|(participant_id, _, _)| participant_id.clone())
    }

    fn generate_agent_reply(
        &self,
        meeting: &Meeting,
        agent: &MeetingParticipant,
        utterance_text: &str,
    ) -> String {
        let agenda_context = best_matching_agenda_phrase(meeting, utterance_text)
            .unwrap_or_else(|| "the current discussion".to_owned());
        let recommendation = role_based_recommendation(agent, utterance_text);
        format!(
            "{} here. For {}, I would focus on {}.",
            agent.name, agenda_context, recommendation
        )
    }

    fn generate_minutes(&self, meeting: &Meeting) -> MinutesDocument {
        MinutesDocument {
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
            agenda: meeting
                .agenda
                .iter()
                .map(|item| MinutesAgendaItem {
                    agenda_item_id: item.agenda_item_id.clone(),
                    phrase: item.phrase.clone(),
                    decisions: collect_decisions_for_agenda_item(meeting, item),
                })
                .collect(),
        }
    }
}

fn token_set(text: &str) -> HashSet<String> {
    text.split_whitespace()
        .map(trim_ascii_punctuation)
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn trim_ascii_punctuation(token: &str) -> &str {
    token.trim_matches(|character: char| character.is_ascii_punctuation())
}

fn overlap_score(text: &str, participant: &MeetingParticipant) -> usize {
    let utterance_tokens = token_set(text);
    let mut participant_tokens = token_set(&participant.name);
    if let Some(instructions) = participant.instructions.as_deref() {
        participant_tokens.extend(token_set(instructions));
    }
    utterance_tokens.intersection(&participant_tokens).count()
}

fn best_matching_agenda_phrase(meeting: &Meeting, utterance_text: &str) -> Option<String> {
    meeting
        .agenda
        .iter()
        .map(|item| {
            (
                item.phrase.clone(),
                agenda_overlap_score(&item.phrase, utterance_text),
            )
        })
        .max_by(|left, right| left.1.cmp(&right.1))
        .and_then(|(phrase, score)| if score == 0 { None } else { Some(phrase) })
}

fn agenda_overlap_score(agenda_phrase: &str, text: &str) -> usize {
    let agenda_tokens = token_set(agenda_phrase);
    let utterance_tokens = token_set(text);
    agenda_tokens.intersection(&utterance_tokens).count()
}

fn role_based_recommendation(agent: &MeetingParticipant, utterance_text: &str) -> String {
    let lowered_instructions = agent
        .instructions
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let lowered_text = utterance_text.to_ascii_lowercase();

    if lowered_instructions.contains("technical") || agent.name.eq_ignore_ascii_case("cto") {
        if lowered_text.contains("timeline") || lowered_text.contains("launch") {
            "de-risking the launch sequence, keeping scope stable, and testing the critical path early".to_owned()
        } else {
            "the main technical bottleneck, the sequencing risk, and the operational load before adding scope".to_owned()
        }
    } else if lowered_instructions.contains("design") || lowered_instructions.contains("usability")
    {
        "clarifying the user flow, cutting confusing edges, and validating the simplest usable version".to_owned()
    } else if lowered_instructions.contains("product")
        || lowered_instructions.contains("scope")
        || agent.name.eq_ignore_ascii_case("pm")
    {
        "protecting the minimum viable scope, naming the tradeoffs explicitly, and assigning a single owner per follow-up".to_owned()
    } else if lowered_instructions.contains("finance") || lowered_instructions.contains("budget") {
        "the downside cost, the budget guardrails, and the smallest next commitment that buys clarity".to_owned()
    } else if lowered_instructions.contains("operations") || lowered_instructions.contains("coo") {
        "clear owners, a firm sequence of work, and the communication steps needed to keep execution aligned".to_owned()
    } else {
        "the most constrained next step, the decision still blocking progress, and who should own it".to_owned()
    }
}

fn collect_decisions_for_agenda_item(meeting: &Meeting, agenda_item: &AgendaItem) -> Vec<String> {
    let mut decisions = Vec::new();
    for entry in &meeting.event_log {
        match &entry.event {
            MeetingEvent::HumanUtteranceRecorded { text, .. }
            | MeetingEvent::AgentUtteranceRecorded { text, .. } => {
                if agenda_overlap_score(&agenda_item.phrase, text) == 0 {
                    continue;
                }
                if is_decision_text(text) {
                    decisions.push(text.clone());
                }
            }
            _ => {}
        }
    }
    decisions
}

fn is_decision_text(text: &str) -> bool {
    let tokens = token_set(text);
    [
        "decide",
        "decision",
        "agreed",
        "agree",
        "will",
        "should",
        "must",
        "assign",
        "owner",
        "prioritize",
    ]
    .iter()
    .any(|token| tokens.contains(*token))
}
