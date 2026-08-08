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
use crate::turn_state::{compute_spoken_this_cycle_after_message, MessageView, ParticipantView};

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
    impersonating_participant_ids: Option<&[String]>,
    debug: Option<SelectionDebug>,
) -> SelectionResult {
    // A seat the human owns OR is impersonating this session takes a *user* turn —
    // the orchestrator pauses the chain so the human types or skips. Impersonation
    // is an overlay (v4 Bug 44): `controlledBy` stays `'llm'`, so consult the
    // overlay rather than the bare column, otherwise a weighted pick would try to
    // generate an LLM response as the character the human is speaking for.
    let reason = if crate::participant_filters::is_user_driven_seat(
        &participant.id,
        &participant.controlled_by,
        impersonating_participant_ids,
    ) {
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

/// Checks if it's a specific participant's turn (v4 `isParticipantsTurn`,
/// utils.ts). True iff the selection's `next_speaker_id` equals this
/// participant's id.
pub fn is_participants_turn(result: &SelectionResult, participant_id: &str) -> bool {
    result.next_speaker_id.as_deref() == Some(participant_id)
}

/// Checks if it's the user's turn — no AI character should speak (v4
/// `isUsersTurn`, utils.ts).
///
/// Two shapes mean "the turn has closed":
///   - `next_speaker_id === None` — classic case, no character was picked at all
///     (e.g. only-LLM chats where the cycle completed).
///   - `reason === "user_turn"` — the rotation landed on a user-controlled
///     CHARACTER participant, whose `next_speaker_id` is the participant's own
///     id (not `None`). This is the path introduced when user characters joined
///     the talkativeness rotation.
///
/// Both branches must be recognized; downstream code (memory extraction,
/// orchestrator chain control, UI banners) keys off this to decide whether the
/// human now has the floor.
pub fn is_users_turn(result: &SelectionResult) -> bool {
    result.next_speaker_id.is_none() || result.reason == "user_turn"
}

/// Human-readable explanation of why a participant was selected (v4
/// `getSelectionExplanation`, utils.ts). Unknown reasons fall through to the
/// `default` branch, matching v4's `switch`.
pub fn get_selection_explanation(result: &SelectionResult) -> &'static str {
    match result.reason {
        "queue" => "Selected from queue (manually nudged/queued)",
        "weighted_selection" => "Selected by weighted random based on talkativeness",
        "only_character" => "Only character in chat",
        "user_turn" => "User's turn - waiting for user input",
        "cycle_complete" => "All characters have spoken this cycle - waiting for user",
        _ => "Unknown selection reason",
    }
}

/// Select the next speaker. See module docs for the algorithm; `random01` is the
/// injected `Math.random()` value used by the weighted picks.
#[allow(clippy::too_many_arguments)]
pub fn select_next_speaker(
    participants: &[SpeakerParticipant],
    characters: &HashMap<String, f64>,
    queue: &[String],
    spoken_since_user_turn: &[String],
    last_speaker_id: Option<&str>,
    random01: f64,
    impersonating_participant_ids: Option<&[String]>,
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
        return build_result(
            active[0],
            "only_character",
            false,
            impersonating_participant_ids,
            None,
        );
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
            impersonating_participant_ids,
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
        impersonating_participant_ids,
        Some(SelectionDebug {
            eligible_speakers: new_cycle.iter().map(|p| p.id.clone()).collect(),
            weights: pick.weights,
            random_value: pick.random_value,
            all_llm_new_cycle: true,
        }),
    )
}

/// Fail-soft parse of a JSON string-array id list (v4's inline `parseIds`): a
/// missing/empty/non-array/invalid JSON payload yields `[]`, and only string
/// elements survive.
fn parse_ids(json: Option<&str>) -> Vec<String> {
    let Some(text) = json.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Array(items)) => items
            .into_iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}

/// Who speaks next *after* a user's just-typed message — projected one step past
/// a post that has NOT been persisted yet (v4
/// `selectNextSpeakerAfterUserMessage`, selection.ts).
///
/// The first-responder decision on a fresh user send happens before the message
/// is written to history, so a turn-state recomputed from history would still
/// resolve to the poster (whose turn it currently is), not the seat that follows
/// them. This helper advances the persisted cycle exactly the way the message
/// write will (via [`compute_spoken_this_cycle_after_message`], so the projection
/// and the eventual persisted state agree), sets the poster as `last_speaker_id`,
/// then runs the normal full-rotation [`select_next_speaker`] over ALL
/// participants.
///
/// The caller uses this to detect when the floor after a human's post belongs to
/// ANOTHER seat the human drives — in which case the chat pauses for that seat
/// instead of forcing an LLM to answer every human turn (the fair-rotation fix
/// for rooms where the human drives two or more seats alongside a single LLM).
///
/// `user_participant_id` matches v4's parameter; like v4's `selectNextSpeaker` it
/// is unused by the selection (kept for signature fidelity). `random01` is the
/// injected `Math.random()` value threaded to the delegated weighted pick.
#[allow(clippy::too_many_arguments)]
pub fn select_next_speaker_after_user_message(
    participants: &[SpeakerParticipant],
    characters: &HashMap<String, f64>,
    poster_participant_id: &str,
    persisted_spoken_this_cycle_json: Option<&str>,
    turn_queue_json: Option<&str>,
    _user_participant_id: Option<&str>,
    random01: f64,
    impersonating_participant_ids: Option<&[String]>,
) -> SelectionResult {
    // v4 builds a synthetic `{ type: 'message', role: 'USER', participantId:
    // poster }` event and advances the persisted cycle the same way the eventual
    // message write will.
    let synthetic_post = MessageView {
        msg_type: Some("message".to_string()),
        role: "USER".to_string(),
        participant_id: Some(poster_participant_id.to_string()),
        target_participant_ids: None,
    };
    let cycle_views: Vec<ParticipantView> = participants
        .iter()
        .map(|p| ParticipantView {
            id: p.id.clone(),
            participant_type: p.participant_type.clone(),
            status: p.status,
            character_id: p.character_id.clone(),
        })
        .collect();

    let advanced_json = compute_spoken_this_cycle_after_message(
        &synthetic_post,
        &cycle_views,
        persisted_spoken_this_cycle_json,
    );

    // `advanced_json === null` means the write is a no-op (poster already
    // recorded, no wrap) — keep the persisted set as-is.
    let spoken_since_user_turn = match advanced_json {
        Some(ref json) => parse_ids(Some(json.as_str())),
        None => parse_ids(persisted_spoken_this_cycle_json),
    };
    let queue = parse_ids(turn_queue_json);

    select_next_speaker(
        participants,
        characters,
        &queue,
        &spoken_since_user_turn,
        Some(poster_participant_id),
        random01,
        impersonating_participant_ids,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(id: &str) -> SpeakerParticipant {
        SpeakerParticipant {
            id: id.to_string(),
            participant_type: "CHARACTER".to_string(),
            status: ParticipantStatus::Active,
            character_id: Some(format!("char-{id}")),
            controlled_by: "llm".to_string(),
            talkativeness: None,
        }
    }

    // v4 turn-manager.test.ts (Bug 44): an impersonated LLM seat takes a *user*
    // turn via the overlay, without the column moving.
    #[test]
    fn impersonated_llm_seat_is_user_turn_via_overlay() {
        let impersonated = character("p1");
        let characters = HashMap::new();

        // Without the overlay it is an ordinary LLM speaker (only_character).
        let llm_turn = select_next_speaker(
            std::slice::from_ref(&impersonated),
            &characters,
            &[],
            &[],
            None,
            0.5,
            None,
        );
        assert_eq!(llm_turn.next_speaker_id.as_deref(), Some("p1"));
        assert_eq!(llm_turn.reason, "only_character");

        // With the overlay it pauses for the human, without the column moving.
        let overlay = ["p1".to_string()];
        let user_turn = select_next_speaker(
            std::slice::from_ref(&impersonated),
            &characters,
            &[],
            &[],
            None,
            0.5,
            Some(&overlay),
        );
        assert_eq!(user_turn.next_speaker_id.as_deref(), Some("p1"));
        assert_eq!(user_turn.reason, "user_turn");
        assert_eq!(impersonated.controlled_by, "llm");
    }

    // ---- selectNextSpeakerAfterUserMessage (Bug 50 fair rotation) ----
    //
    // The reported bug: the human plays Charlie (user) AND impersonates Lorian
    // (an LLM seat, controlledBy still 'llm' under the Bug 44 overlay), with Kumar
    // the sole real LLM. The first-responder picker used an LLM-only shortlist, so
    // Kumar answered EVERY human turn. These mirror v4's four new jest cases.

    fn user_seat(id: &str) -> SpeakerParticipant {
        SpeakerParticipant {
            id: id.to_string(),
            participant_type: "CHARACTER".to_string(),
            status: ParticipantStatus::Active,
            character_id: Some(format!("char-{id}")),
            controlled_by: "user".to_string(),
            talkativeness: None,
        }
    }

    fn fair_room() -> Vec<SpeakerParticipant> {
        // charlie: user seat; lorian: LLM seat impersonated; kumar: sole real LLM.
        vec![
            user_seat("charlie"),
            character("lorian"),
            character("kumar"),
        ]
    }

    #[test]
    fn after_user_hands_floor_to_impersonated_seat_pause() {
        // Kumar already spoke this cycle; Charlie is posting now. The only eligible
        // seat left is Lorian — and Lorian is user-driven, so the caller pauses.
        let parts = fair_room();
        let impersonating = ["lorian".to_string()];
        let result = select_next_speaker_after_user_message(
            &parts,
            &HashMap::new(),
            "charlie",
            Some("[\"kumar\"]"),
            Some("[]"),
            Some("charlie"),
            0.5,
            Some(&impersonating),
        );
        assert_eq!(result.next_speaker_id.as_deref(), Some("lorian"));
        assert_eq!(result.reason, "user_turn");
    }

    #[test]
    fn after_impersonated_post_lets_real_llm_answer_no_pause() {
        // Charlie already spoke; the human just typed as Lorian. The only eligible
        // seat is Kumar (a real LLM) — the caller must NOT pause.
        let parts = fair_room();
        let impersonating = ["lorian".to_string()];
        let result = select_next_speaker_after_user_message(
            &parts,
            &HashMap::new(),
            "lorian",
            Some("[\"charlie\"]"),
            Some("[]"),
            Some("charlie"),
            0.5,
            Some(&impersonating),
        );
        assert_eq!(result.next_speaker_id.as_deref(), Some("kumar"));
        assert_ne!(result.reason, "user_turn");
    }

    #[test]
    fn after_user_wraps_cycle_then_picks_from_fresh_set() {
        // Kumar and Lorian have spoken; Charlie posting completes the 3-seat cycle,
        // so it wraps and the next speaker is drawn from {Lorian, Kumar}.
        let parts = fair_room();
        let impersonating = ["lorian".to_string()];
        let result = select_next_speaker_after_user_message(
            &parts,
            &HashMap::new(),
            "charlie",
            Some("[\"kumar\",\"lorian\"]"),
            Some("[]"),
            Some("charlie"),
            0.1,
            Some(&impersonating),
        );
        let next = result.next_speaker_id.as_deref();
        assert_ne!(next, Some("charlie"));
        assert!(matches!(next, Some("lorian") | Some("kumar")));
    }

    #[test]
    fn after_user_honours_the_queue_ahead_of_rotation() {
        let parts = fair_room();
        let impersonating = ["lorian".to_string()];
        let result = select_next_speaker_after_user_message(
            &parts,
            &HashMap::new(),
            "charlie",
            Some("[\"kumar\"]"),
            Some("[\"kumar\"]"), // Kumar explicitly queued
            Some("charlie"),
            0.5,
            Some(&impersonating),
        );
        assert_eq!(result.next_speaker_id.as_deref(), Some("kumar"));
        assert_eq!(result.reason, "queue");
    }

    #[test]
    fn after_user_bad_json_is_fail_soft_empty() {
        // Bad/absent JSON id lists parse to `[]` (v4's inline `parseIds`).
        assert_eq!(parse_ids(None), Vec::<String>::new());
        assert_eq!(parse_ids(Some("")), Vec::<String>::new());
        assert_eq!(parse_ids(Some("{not json")), Vec::<String>::new());
        assert_eq!(parse_ids(Some("\"scalar\"")), Vec::<String>::new());
        assert_eq!(parse_ids(Some("[1,\"a\",2]")), vec!["a".to_string()]);
    }
}
