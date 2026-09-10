//! Port of v4's lib/chat/turn-manager/cycle-order.ts (`2aca73ad6`) — the
//! rotation, drawn once and then kept.
//!
//! A cycle is one full pass of the room: every present character seat speaks
//! exactly once, then the cycle wraps and a new order is drawn. The order is a
//! weighted permutation — the same successive-sampling draw the per-turn
//! selection used to make one pick at a time — so the *distribution* of
//! rotations is unchanged. What changes is that the whole permutation is decided
//! up front and persisted, so every reader (the chain loop, the `?action=turn`
//! projection, the participant sidebar) sees the same answer to "who is after
//! whom", and the sidebar can show a real position rather than a guess.
//!
//! The stored value (`chat.cycleOrderParticipantIds`) is the *remaining* speakers
//! of the current cycle, in order:
//!
//!   - **Consumed** by the pure helpers in [`crate::turn_state`], at the same
//!     write chokepoints that advance `spokenThisCycleParticipantIds` — a
//!     speaker's id is struck from the list when their message lands (or when
//!     their user turn is skipped). No weights are needed to consume, so those
//!     paths stay pure.
//!   - **Drawn** by [`resolve_cycle_order`], the one writer, when the remaining
//!     list holds nobody who can still speak. Drawing needs talkativeness, which
//!     needs the characters map, which is why it lives here.
//!
//! **The characters map must cover the whole room.** Build it with
//! [`crate::room_characters::load_room_characters`], never by hand from
//! `get_active_character_participants` — that helper returns LLM-controlled seats
//! only, and a seat missing from the map is weighted at the 0.5 default and never
//! checked for `archived`. A map built that way silently ignores the
//! talkativeness of every character the human drives (v4 bug 131).
//!
//! [`crate::select_speaker::select_next_speaker`] treats the order as an overlay:
//! it takes the first usable id from the order, and falls back to the old
//! one-at-a-time weighted pick when the order is empty or stale.

use std::collections::{HashMap, HashSet};

use crate::participant_filters::is_present_character_seat;
use crate::select_speaker::{SpeakerCharacter, SpeakerParticipant};
use crate::weighted_random::{pick_weighted_random, DrawSource};

/// Parses a stored cycle order, tolerating null, malformed JSON, and non-string
/// members (v4 `parseCycleOrder`, `:66-75`). A bad value reads as an empty cycle,
/// which redraws on the next selection — the field is never load-bearing enough
/// to fail a turn over.
///
/// v4's guard is `if (!json) return []`, i.e. JS truthiness: `null`, `undefined`
/// AND the empty string all read as "no rotation on file".
pub fn parse_cycle_order(json: Option<&str>) -> Vec<String> {
    let Some(text) = json.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}

/// JSON-encode a rotation exactly as JS `JSON.stringify` does (compact).
pub fn stringify_cycle_order(order: &[String]) -> String {
    serde_json::to_string(order).expect("string array always serializes")
}

/// The seats that take part in a rotation (v4 `cycleCandidates`, `:83-94`):
/// present CHARACTER participants whose character still exists and is not
/// archived. User-driven seats are INCLUDED — they hold a place in the order,
/// and the orchestrator pauses the chain when the rotation reaches them.
///
/// A character NOT in the map is KEPT (v4's optional chain
/// `!characters.get(id)?.archivedAt`): the map is built from a best-effort read,
/// and dropping a seat over a failed lookup would silently shrink the room. Only
/// a *known* archived character is excluded.
pub fn cycle_candidates<'a>(
    participants: &'a [SpeakerParticipant],
    characters: &HashMap<String, SpeakerCharacter>,
) -> Vec<&'a SpeakerParticipant> {
    participants
        .iter()
        .filter(|p| {
            is_present_character_seat(&p.participant_type, p.status, p.character_id.as_deref())
        })
        .filter(|p| {
            !p.character_id
                .as_deref()
                .and_then(|cid| characters.get(cid))
                .is_some_and(|c| c.archived)
        })
        .collect()
}

/// Draws a fresh rotation (v4 `drawCycleOrder`, `:104-132`): a weighted
/// permutation of the candidate seats, sampled WITHOUT replacement,
/// heaviest-talkers-likeliest at every position.
///
/// `exclude_first` keeps the seat that just spoke out of position 1, which is the
/// no-back-to-back guard the one-at-a-time algorithm applied at each wrap. They
/// stay in the permutation — just never at its head. A one-candidate pool seats
/// the previous speaker anyway (v4's `pool.length > 1` conjunct).
///
/// Consumes exactly ONE draw per remaining candidate: N draws for an N-seat room,
/// over a shrinking pool with a recomputed total weight. That is the whole reason
/// the injection is a [`DrawSource`] and not a scalar.
pub fn draw_cycle_order(
    participants: &[SpeakerParticipant],
    characters: &HashMap<String, SpeakerCharacter>,
    exclude_first: Option<&str>,
    draws: &DrawSource,
) -> Vec<String> {
    let remaining = cycle_candidates(participants, characters);
    if remaining.is_empty() {
        return Vec::new();
    }

    // v4's `weightOf`: the per-chat override wins, then the character's value,
    // then 0.5 — the same chain `pickWeighted` applies per turn.
    let weight_of = |p: &&SpeakerParticipant| -> f64 {
        let character_talk = p
            .character_id
            .as_deref()
            .and_then(|cid| characters.get(cid).and_then(|c| c.talkativeness));
        p.talkativeness.or(character_talk).unwrap_or(0.5)
    };

    let mut order: Vec<String> = Vec::new();
    let mut pool: Vec<&SpeakerParticipant> = remaining;

    while !pool.is_empty() {
        // Position 1 only: hold back the previous speaker when anyone else could
        // take the floor instead.
        let hold_back =
            order.is_empty() && exclude_first.is_some_and(|e| !e.is_empty()) && pool.len() > 1;
        let eligible: Vec<&SpeakerParticipant> = if hold_back {
            let excluded = exclude_first.unwrap();
            pool.iter().copied().filter(|p| p.id != excluded).collect()
        } else {
            pool.clone()
        };

        // v4 discards `equalWeights` here on purpose: a wholly silent room would
        // otherwise log the per-turn pick's warn once per seat, per cycle.
        let picked = pick_weighted_random(&eligible, weight_of, draws);
        let chosen_id = picked.item.id.clone();
        order.push(chosen_id.clone());
        // v4 `pool.splice(pool.indexOf(item), 1)` — the FIRST occurrence.
        if let Some(i) = pool.iter().position(|p| p.id == chosen_id) {
            pool.remove(i);
        }
    }

    order
}

/// The first seat in `order` that can still take the floor (v4
/// `pickFromCycleOrder`, `:145-160`): present, not already spoken this cycle, and
/// not the seat that just spoke. `None` when the order holds nobody usable — the
/// signal that the cycle is spent and a fresh one must be drawn.
///
/// Stale ids (a departed seat, an archived character) are SKIPPED rather than
/// treated as terminal, so a cast change mid-cycle costs that seat its turn and
/// nothing more. An empty/missing `order` reads as "no rotation on file".
pub fn pick_from_cycle_order(
    order: &[String],
    participants: &[SpeakerParticipant],
    characters: &HashMap<String, SpeakerCharacter>,
    spoken_since_user_turn: &[String],
    last_speaker_id: Option<&str>,
) -> Option<String> {
    if order.is_empty() {
        return None;
    }
    let usable: HashSet<&str> = cycle_candidates(participants, characters)
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    for id in order {
        if !usable.contains(id.as_str()) {
            continue;
        }
        if spoken_since_user_turn.iter().any(|s| s == id) {
            continue;
        }
        if Some(id.as_str()) == last_speaker_id {
            continue;
        }
        return Some(id.clone());
    }
    None
}

/// How [`resolve_cycle_order`] settled the rotation — v4's `how` field on the
/// two persist log lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CyclePersistHow {
    /// A fresh permutation was drawn (the stored one could seat nobody).
    Drawn,
    /// The stored rotation still works and gained a mid-cycle joiner at the back.
    Extended,
}

impl CyclePersistHow {
    /// v4's literal, as it appears in the log bag.
    pub fn as_str(self) -> &'static str {
        match self {
            CyclePersistHow::Drawn => "drawn",
            CyclePersistHow::Extended => "extended",
        }
    }
}

/// What [`resolve_cycle_order_pure`] decided: the rotation to use, and the write
/// to make (if any).
#[derive(Clone, Debug, PartialEq)]
pub struct CycleOrderResolution {
    /// The remaining rotation for this cycle — what the caller threads into the
    /// turn state, whether or not the write succeeds.
    pub order: Vec<String>,
    /// `Some` when the row must be updated. `None` for the three no-write arms:
    /// no candidates, a single-seat room, and a stored rotation that still works
    /// with nobody new to append.
    pub persist: Option<CyclePersistHow>,
}

/// The pure half of v4's `resolveCycleOrder` (`:174-211`) — everything but the
/// repository write.
///
/// Split out because v5's write path is a channel to the single writer, not an
/// awaited repo call: the decision is a pure function of the row and the room,
/// and [`resolve_cycle_order`] is the thin async shell that performs the write
/// and logs. It is also what makes the decision tier-1 comparable against v4's
/// real module.
///
/// Late arrivals are APPENDED rather than triggering a redraw: joining mid-cycle
/// puts you at the back of the queue, and the next cycle deals you in properly.
/// Departed/archived seats are never removed, only skipped on read.
pub fn resolve_cycle_order_pure(
    participants: &[SpeakerParticipant],
    characters: &HashMap<String, SpeakerCharacter>,
    stored: &[String],
    spoken_since_user_turn: &[String],
    last_speaker_id: Option<&str>,
    draws: &DrawSource,
) -> CycleOrderResolution {
    let candidates = cycle_candidates(participants, characters);

    if candidates.is_empty() {
        return CycleOrderResolution {
            order: Vec::new(),
            persist: None,
        };
    }

    // A single-seat room has no rotation to speak of — the one character simply
    // continues. Storing a one-entry order would churn the row every turn.
    if candidates.len() == 1 {
        return CycleOrderResolution {
            order: Vec::new(),
            persist: None,
        };
    }

    if pick_from_cycle_order(
        stored,
        participants,
        characters,
        spoken_since_user_turn,
        last_speaker_id,
    )
    .is_some()
    {
        // Still usable. Seat anyone who joined mid-cycle at the back, so they are
        // not passed over twice.
        let known: HashSet<&str> = stored.iter().map(String::as_str).collect();
        let latecomers: Vec<String> = candidates
            .iter()
            .map(|p| p.id.clone())
            .filter(|id| {
                !known.contains(id.as_str()) && !spoken_since_user_turn.iter().any(|s| s == id)
            })
            .collect();

        if latecomers.is_empty() {
            return CycleOrderResolution {
                order: stored.to_vec(),
                persist: None,
            };
        }

        let mut extended = stored.to_vec();
        extended.extend(latecomers);
        return CycleOrderResolution {
            order: extended,
            persist: Some(CyclePersistHow::Extended),
        };
    }

    let fresh = draw_cycle_order(participants, characters, last_speaker_id, draws);
    CycleOrderResolution {
        order: fresh,
        persist: Some(CyclePersistHow::Drawn),
    }
}

/// Emit v4's two `persist` log lines (`cycle-order.ts:218-239`).
///
/// The write failure is SWALLOWED by contract: the caller still has the order in
/// memory and this turn proceeds on it, and the next selection simply draws
/// again. A bookkeeping column is never worth failing a turn over.
pub fn log_cycle_order_persisted(chat_id: &str, how: CyclePersistHow, order: &[String]) {
    tracing::debug!(
        chatId = chat_id,
        how = how.as_str(),
        order = ?order,
        "[Turn Manager] Cycle order persisted"
    );
}

/// The swallowed-failure twin of [`log_cycle_order_persisted`].
pub fn log_cycle_order_persist_failed(chat_id: &str, how: CyclePersistHow, error: &str) {
    tracing::warn!(
        chatId = chat_id,
        how = how.as_str(),
        error = error,
        "[Turn Manager] Failed to persist cycle order"
    );
}

/// v4 `resolveCycleOrder` (`cycle-order.ts:174-211`) whole — the pure decision
/// plus the write and its two log lines.
///
/// The single writer of `chats.cycleOrderParticipantIds`. Every server path that
/// is about to ask "who speaks next" calls this first and threads the result into
/// the turn state, so all of them read ONE rotation instead of each drawing their
/// own.
///
/// A failed write is logged and SWALLOWED: the caller still has the order in
/// memory and this turn proceeds on it, and the next selection simply draws
/// again. A bookkeeping column is never worth failing a turn over.
pub async fn resolve_cycle_order(
    db: &crate::db::runtime::Db,
    chat_id: &str,
    participants: &[SpeakerParticipant],
    characters: &HashMap<String, SpeakerCharacter>,
    turn_state: &crate::turn_state::TurnState,
    draws: &DrawSource,
) -> Vec<String> {
    let decision = resolve_cycle_order_pure(
        participants,
        characters,
        &turn_state.cycle_order,
        &turn_state.spoken_since_user_turn,
        turn_state.last_speaker_id.as_deref(),
        draws,
    );

    let Some(how) = decision.persist else {
        return decision.order;
    };

    let json = stringify_cycle_order(&decision.order);
    let chat_id_owned = chat_id.to_string();
    let write = db
        .write(move |writers| {
            writers.main().chats().update(
                &chat_id_owned,
                &crate::db::chats::ChatUpdate {
                    cycle_order_participant_ids: Some(json),
                    ..Default::default()
                },
            )
        })
        .await;

    match write {
        Ok(_) => log_cycle_order_persisted(chat_id, how, &decision.order),
        Err(e) => log_cycle_order_persist_failed(chat_id, how, &e.to_string()),
    }
    decision.order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_predicates::ParticipantStatus;

    fn seat(id: &str, talk: Option<f64>) -> SpeakerParticipant {
        SpeakerParticipant {
            id: id.to_string(),
            participant_type: "CHARACTER".to_string(),
            status: ParticipantStatus::Active,
            character_id: Some(format!("char-{id}")),
            controlled_by: "llm".to_string(),
            talkativeness: talk,
        }
    }

    #[test]
    fn parse_is_fail_soft() {
        assert!(parse_cycle_order(None).is_empty());
        assert!(parse_cycle_order(Some("")).is_empty());
        assert!(parse_cycle_order(Some("{not json")).is_empty());
        assert!(parse_cycle_order(Some("\"scalar\"")).is_empty());
        assert_eq!(parse_cycle_order(Some("[1,\"a\",null]")), vec!["a"]);
    }

    // v4 `drawCycleOrder` samples WITHOUT replacement: every candidate appears
    // exactly once, whatever the draws.
    #[test]
    fn the_draw_is_a_permutation() {
        let parts = vec![
            seat("A", Some(0.9)),
            seat("B", Some(0.3)),
            seat("C", Some(0.8)),
        ];
        for pin in [0.0, 0.25, 0.5, 0.75, 0.999] {
            let order = draw_cycle_order(&parts, &HashMap::new(), None, &DrawSource::constant(pin));
            let mut sorted = order.clone();
            sorted.sort();
            assert_eq!(sorted, vec!["A", "B", "C"], "pin {pin} gave {order:?}");
        }
    }

    #[test]
    fn exclude_first_holds_the_last_speaker_off_the_head() {
        let parts = vec![seat("A", Some(1.0)), seat("B", Some(1.0))];
        // A would otherwise lead on a 0.0 draw; excluded, B leads and A follows.
        let order = draw_cycle_order(
            &parts,
            &HashMap::new(),
            Some("A"),
            &DrawSource::constant(0.0),
        );
        assert_eq!(order, vec!["B", "A"]);
    }

    // v4's `pool.length > 1` conjunct: a one-candidate pool seats the previous
    // speaker anyway rather than returning an empty rotation.
    #[test]
    fn a_lone_candidate_is_seated_even_when_excluded() {
        let parts = vec![seat("A", Some(1.0))];
        let order = draw_cycle_order(
            &parts,
            &HashMap::new(),
            Some("A"),
            &DrawSource::constant(0.0),
        );
        assert_eq!(order, vec!["A"]);
    }

    // The cycle draw does NOT log the per-turn pick's equal-weights warn (v4
    // discards the flag).
    #[test]
    fn the_draw_never_warns_on_equal_weights() {
        let parts = vec![seat("A", Some(0.0)), seat("B", Some(0.0))];
        let lines = crate::test_support::captured(|| {
            let order = draw_cycle_order(&parts, &HashMap::new(), None, &DrawSource::constant(0.6));
            assert_eq!(order.len(), 2);
        });
        assert!(
            !lines.iter().any(|l| l.contains("Total talkativeness is 0")),
            "the cycle draw must not log the per-turn warn: {lines:?}"
        );
    }

    // v4 `cycleCandidates`' documented contract: a character absent from the map
    // is KEPT (only a KNOWN archived one is dropped).
    #[test]
    fn an_unknown_character_keeps_its_seat() {
        let parts = vec![seat("A", None), seat("B", None)];
        let mut chars = HashMap::new();
        chars.insert(
            "char-B".to_string(),
            SpeakerCharacter {
                talkativeness: None,
                archived: true,
            },
        );
        let kept: Vec<&str> = cycle_candidates(&parts, &chars)
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(kept, vec!["A"], "A has no map entry and must be kept");
    }

    #[test]
    fn a_one_seat_room_stores_nothing() {
        let parts = vec![seat("A", None)];
        let r = resolve_cycle_order_pure(
            &parts,
            &HashMap::new(),
            &[],
            &[],
            None,
            &DrawSource::constant(0.0),
        );
        assert!(r.order.is_empty());
        assert_eq!(r.persist, None, "a one-entry order would churn the row");
    }

    #[test]
    fn a_usable_stored_order_is_returned_without_a_write() {
        let parts = vec![seat("A", None), seat("B", None)];
        let stored = vec!["A".to_string(), "B".to_string()];
        let r = resolve_cycle_order_pure(
            &parts,
            &HashMap::new(),
            &stored,
            &[],
            None,
            &DrawSource::constant(0.0),
        );
        assert_eq!(r.order, stored);
        assert_eq!(r.persist, None);
    }

    #[test]
    fn a_latecomer_goes_to_the_back() {
        let parts = vec![seat("A", None), seat("B", None), seat("C", None)];
        let stored = vec!["A".to_string(), "B".to_string()];
        let r = resolve_cycle_order_pure(
            &parts,
            &HashMap::new(),
            &stored,
            &[],
            None,
            &DrawSource::constant(0.0),
        );
        assert_eq!(r.order, vec!["A", "B", "C"]);
        assert_eq!(r.persist, Some(CyclePersistHow::Extended));
    }

    #[test]
    fn a_spent_rotation_is_redrawn() {
        let parts = vec![seat("A", None), seat("B", None)];
        // Both have spoken: nothing in the stored order is usable.
        let r = resolve_cycle_order_pure(
            &parts,
            &HashMap::new(),
            &["A".to_string(), "B".to_string()],
            &["A".to_string(), "B".to_string()],
            Some("B"),
            &DrawSource::constant(0.0),
        );
        assert_eq!(r.persist, Some(CyclePersistHow::Drawn));
        assert_eq!(r.order, vec!["A", "B"], "B was held off the head");
    }

    // A stale id (a departed seat) is skipped, not terminal.
    #[test]
    fn stale_ids_are_skipped_on_read() {
        let parts = vec![seat("A", None), seat("B", None)];
        let stored = vec!["GONE".to_string(), "B".to_string()];
        assert_eq!(
            pick_from_cycle_order(&stored, &parts, &HashMap::new(), &[], None).as_deref(),
            Some("B")
        );
    }
}
