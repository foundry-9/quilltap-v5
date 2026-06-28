//! Port of v4's lib/chat/turn-manager/selection.ts — the weighted-random
//! next-speaker selection for multi-character chats.
//!
//! The sole impurity in v4 is `Math.random()` inside `pickWeighted`; here it is
//! injected as `random01` (the value `Math.random()` would return, in [0, 1)),
//! so selection is a pure function of its inputs. A user-controlled pick keeps
//! the participant's id as `next_speaker_id` but reports reason `user_turn`
//! (the orchestrator then pauses for the human).

use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::chat_predicates::{is_participant_present, ParticipantStatus};

/// A participant in the rotation. `talkativeness` (a per-chat override) wins over
/// the character's value when set.
#[derive(Clone, Debug)]
pub struct SpeakerParticipant {
    pub id: String,
    pub participant_type: String,
    pub status: ParticipantStatus,
    pub character_id: Option<String>,
    pub controlled_by: String,
    pub talkativeness: Option<f64>,
}

impl SpeakerParticipant {
    fn is_active_character(&self) -> bool {
        self.participant_type == "CHARACTER"
            && is_participant_present(self.status)
            && self.character_id.as_deref().is_some_and(|c| !c.is_empty())
    }
}

/// Debug detail attached to a weighted selection.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionDebug {
    pub eligible_speakers: Vec<String>,
    pub weights: BTreeMap<String, f64>,
    pub random_value: f64,
    pub all_llm_new_cycle: bool,
}

/// The result of a selection. `next_speaker_id` is `None` only when no character
/// can speak (cycle complete / user's turn with nobody picked).
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionResult {
    pub next_speaker_id: Option<String>,
    pub reason: &'static str,
    pub cycle_complete: bool,
    pub debug: Option<SelectionDebug>,
}

struct WeightedPick {
    participant_id: String,
    weights: BTreeMap<String, f64>,
    random_value: f64,
}

/// Weighted-random pick over `candidates`. `talkativeness` is the participant
/// override, else the character's value, else 0.5; if all weights are zero they
/// reset to 1 (equal). `random01` is the injected `Math.random()` value.
fn pick_weighted(
    candidates: &[&SpeakerParticipant],
    characters: &HashMap<String, f64>,
    random01: f64,
) -> WeightedPick {
    let mut weights: BTreeMap<String, f64> = BTreeMap::new();
    let mut total_weight = 0.0;
    for p in candidates {
        let character_talk = p
            .character_id
            .as_deref()
            .and_then(|cid| characters.get(cid).copied());
        let talkativeness = p.talkativeness.or(character_talk).unwrap_or(0.5);
        weights.insert(p.id.clone(), talkativeness);
        total_weight += talkativeness;
    }
    if total_weight == 0.0 {
        for p in candidates {
            weights.insert(p.id.clone(), 1.0);
            total_weight += 1.0;
        }
    }
    let random_value = random01 * total_weight;
    let mut cumulative = 0.0;
    for p in candidates {
        cumulative += weights[&p.id];
        if random_value < cumulative {
            return WeightedPick {
                participant_id: p.id.clone(),
                weights,
                random_value,
            };
        }
    }
    WeightedPick {
        participant_id: candidates[candidates.len() - 1].id.clone(),
        weights,
        random_value,
    }
}

fn build_result(
    participant: &SpeakerParticipant,
    reason: &'static str,
    cycle_complete: bool,
    debug: Option<SelectionDebug>,
) -> SelectionResult {
    let reason = if participant.controlled_by == "user" {
        "user_turn"
    } else {
        reason
    };
    SelectionResult {
        next_speaker_id: Some(participant.id.clone()),
        reason,
        cycle_complete,
        debug,
    }
}

/// Select the next speaker. See module docs for the algorithm; `random01` is the
/// injected `Math.random()` value used by the weighted picks.
pub fn select_next_speaker(
    participants: &[SpeakerParticipant],
    characters: &HashMap<String, f64>,
    queue: &[String],
    spoken_since_user_turn: &[String],
    last_speaker_id: Option<&str>,
    random01: f64,
) -> SelectionResult {
    // Step 1: the manual queue wins.
    if let Some(first) = queue.first() {
        return SelectionResult {
            next_speaker_id: Some(first.clone()),
            reason: "queue",
            cycle_complete: false,
            debug: None,
        };
    }

    let active: Vec<&SpeakerParticipant> = participants
        .iter()
        .filter(|p| p.is_active_character())
        .collect();

    if active.is_empty() {
        return SelectionResult {
            next_speaker_id: None,
            reason: "user_turn",
            cycle_complete: true,
            debug: None,
        };
    }

    // Single character: let them continue (the no-back-to-back guard is moot).
    if active.len() == 1 {
        return build_result(active[0], "only_character", false, None);
    }

    // Step 2: eligible = active minus { last speaker, already-spoken }.
    let eligible: Vec<&SpeakerParticipant> = active
        .iter()
        .copied()
        .filter(|p| {
            Some(p.id.as_str()) != last_speaker_id
                && !spoken_since_user_turn.iter().any(|s| s == &p.id)
        })
        .collect();

    if !eligible.is_empty() {
        let pick = pick_weighted(&eligible, characters, random01);
        let picked = eligible
            .iter()
            .find(|p| p.id == pick.participant_id)
            .unwrap();
        return build_result(
            picked,
            "weighted_selection",
            false,
            Some(SelectionDebug {
                eligible_speakers: eligible.iter().map(|p| p.id.clone()).collect(),
                weights: pick.weights,
                random_value: pick.random_value,
                all_llm_new_cycle: false,
            }),
        );
    }

    // Step 3: cycle wrapped — pick from { active minus last speaker }.
    let new_cycle: Vec<&SpeakerParticipant> = active
        .iter()
        .copied()
        .filter(|p| Some(p.id.as_str()) != last_speaker_id)
        .collect();

    if new_cycle.is_empty() {
        return SelectionResult {
            next_speaker_id: None,
            reason: "cycle_complete",
            cycle_complete: true,
            debug: None,
        };
    }

    let pick = pick_weighted(&new_cycle, characters, random01);
    let picked = new_cycle
        .iter()
        .find(|p| p.id == pick.participant_id)
        .unwrap();
    build_result(
        picked,
        "weighted_selection",
        true,
        Some(SelectionDebug {
            eligible_speakers: new_cycle.iter().map(|p| p.id.clone()).collect(),
            weights: pick.weights,
            random_value: pick.random_value,
            all_llm_new_cycle: true,
        }),
    )
}
