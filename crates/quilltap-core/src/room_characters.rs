//! Port of v4's lib/chat/turn-manager/room-characters.ts (`d14da3a56`, bug 131)
//! — the one place a turn path loads "who is in this room".
//!
//! Every server path that asks "who speaks next" needs a `characterId →
//! Character` map first: [`crate::cycle_order::resolve_cycle_order_pure`] weights
//! the rotation draw by talkativeness out of it, and
//! [`crate::select_speaker::select_next_speaker`] reads `archivedAt` out of it to
//! drop tombstoned seats. Six call sites used to build that map by hand, and four
//! of them built it from `getActiveCharacterParticipants` — which, despite the
//! name, returns only LLM-controlled seats. A seat the human drives was therefore
//! invisible to both readers: its character's talkativeness never reached the draw
//! (it fell through to the 0.5 default unless the seat carried a per-chat
//! override), and an archived character on it was never excluded. Building the map
//! over [`crate::participant_filters::get_present_character_seats`] instead is what
//! this module is for.
//!
//! It is also the cheaper read, which is why widening the map costs nothing.
//! `find_by_id` overlays one character's vault through `loadVaultFileMaps([one
//! mount])` — eleven vault queries plus the row — so the old per-seat loops paid
//! that eleven times over in a four-seat room. `find_by_ids` overlays the whole
//! room in one pass, whatever the seat count.
//!
//! **Best-effort, by contract.** [`crate::cycle_order::cycle_candidates`]
//! documents its input as a best-effort read and deliberately KEEPS a seat whose
//! character is missing from the map, so that a failed lookup costs a name rather
//! than silently shrinking the room. The batched read is what actually honours
//! that: the list overlay logs and DROPS a character whose vault is unavailable,
//! where the single-character overlay behind `find_by_id` returns an error —
//! which, in the old loops, propagated straight out of speaker selection and took
//! the whole turn (and the read-only `?action=turn` projection behind the
//! participant sidebar) with it.

use std::collections::HashMap;

use serde_json::Value;

use crate::db::DbError;
use crate::participant_filters::is_present_character_seat;
use crate::select_speaker::{SpeakerCharacter, SpeakerParticipant};

/// The batched read [`load_room_characters`] needs — v4's
/// `repos.characters.findByIds`.
///
/// A closure rather than a trait so the differential can count calls on the Rust
/// side exactly as v4's `jest.fn` counts them on its own, and so the production
/// caller can pass its nested `read_main(read_mount_index(…))` without a wrapper
/// type.
pub type FindCharactersByIds<'a> = &'a mut dyn FnMut(&[String]) -> Result<Vec<Value>, DbError>;

/// Loads `characterId → Character` for every character seat present in the room —
/// LLM-driven and user-driven alike — in a single batched read.
///
/// Seats whose character cannot be read are simply absent from the map. That is
/// the documented contract of every consumer: an unknown character is treated as
/// present and unarchived at the default weight, never as a reason to drop a seat
/// or fail a turn.
///
/// `preloaded` characters are seeded AFTER the batch, so the caller's copy WINS.
/// The message finalizer uses this for the character that just spoke: it has the
/// authoritative post-turn record in hand and must not be handed a staler one.
/// Entries without a (truthy) `id` are ignored, matching v4's `if (preloaded?.id)`.
pub fn load_room_characters(
    participants: &[SpeakerParticipant],
    preloaded: &[Value],
    find_by_ids: FindCharactersByIds<'_>,
) -> Result<HashMap<String, Value>, DbError> {
    let seats: Vec<&SpeakerParticipant> = participants
        .iter()
        .filter(|p| {
            is_present_character_seat(&p.participant_type, p.status, p.character_id.as_deref())
        })
        .collect();

    // v4 `Array.from(new Set(seats.map(characterId)))` — deduped, in first-seen
    // order (two seats can play the same character).
    let mut ids: Vec<String> = Vec::new();
    for s in &seats {
        let cid = s.character_id.clone().unwrap_or_default();
        if !ids.contains(&cid) {
            ids.push(cid);
        }
    }

    let mut characters: HashMap<String, Value> = HashMap::new();

    // v4 skips the read entirely for an empty room — the call COUNT is a
    // comparand, so this guard is behaviour, not an optimisation.
    if !ids.is_empty() {
        for row in find_by_ids(&ids)? {
            if let Some(id) = row.get("id").and_then(Value::as_str) {
                characters.insert(id.to_string(), row);
            }
        }
    }

    for p in preloaded {
        if let Some(id) = p
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            characters.insert(id.to_string(), p.clone());
        }
    }

    tracing::debug!(
        seats = seats.len(),
        requested = ids.len(),
        resolved = characters.len(),
        "[Turn Manager] Room characters loaded"
    );

    // A shortfall means a seat's character row is gone or its vault is on the
    // shelf. Worth saying out loud — the room is about to be weighted and ordered
    // without it — but never worth failing a turn over.
    let missing: Vec<&String> = ids
        .iter()
        .filter(|id| !characters.contains_key(*id))
        .collect();
    if !missing.is_empty() {
        tracing::warn!(
            requested = ids.len(),
            missing = ?missing,
            "[Turn Manager] Room seats with no readable character"
        );
    }

    Ok(characters)
}

/// Project the room map down to what the selection and the rotation draw read —
/// v4's `Map<string, Character>` narrowed to `talkativeness` + `archivedAt`.
///
/// Kept beside the loader so the two facts always travel together: a parallel
/// "archived ids" argument could disagree with the map, and a caller that forgot
/// it would silently let a tombstone take turns
/// ([`crate::select_speaker::SpeakerCharacter`]'s own note).
pub fn to_speaker_characters(room: &HashMap<String, Value>) -> HashMap<String, SpeakerCharacter> {
    room.iter()
        .map(|(id, row)| {
            (
                id.clone(),
                SpeakerCharacter {
                    talkativeness: row.get("talkativeness").and_then(Value::as_f64),
                    archived: crate::api::characters::is_archived(row),
                },
            )
        })
        .collect()
}

/// The `name` of a room character, for the chain decision's `characterName`
/// (v4 `charactersMap.get(id)?.name`).
pub fn room_character_name<'a>(
    room: &'a HashMap<String, Value>,
    character_id: Option<&str>,
) -> Option<&'a str> {
    character_id
        .filter(|c| !c.is_empty())
        .and_then(|cid| room.get(cid))
        .and_then(|c| c.get("name"))
        .and_then(Value::as_str)
}

/// [`load_room_characters`] over the real repositories — the shape every server
/// selection site uses (v4's `loadRoomCharacters(repos, participants, …)`).
///
/// The nested `read_main(read_mount_index(…))` is what
/// `characters_read::find_by_ids` needs for its vault overlay; keeping it here
/// means the six call sites do not each re-derive it.
pub fn load_room_characters_from_db(
    db: &crate::db::runtime::Db,
    participants: &[SpeakerParticipant],
    preloaded: &[Value],
) -> Result<HashMap<String, Value>, DbError> {
    let mut find_by_ids = |ids: &[String]| -> Result<Vec<Value>, DbError> {
        db.read_main(|main| {
            db.read_mount_index(|mount| crate::db::characters_read::find_by_ids(main, mount, ids))
        })
    };
    load_room_characters(participants, preloaded, &mut find_by_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_predicates::ParticipantStatus;

    fn seat(id: &str, character_id: Option<&str>, controlled_by: &str) -> SpeakerParticipant {
        SpeakerParticipant {
            id: id.to_string(),
            participant_type: "CHARACTER".to_string(),
            status: ParticipantStatus::Active,
            character_id: character_id.map(String::from),
            controlled_by: controlled_by.to_string(),
            talkativeness: None,
        }
    }

    fn row(id: &str) -> Value {
        serde_json::json!({ "id": id, "name": format!("Character {id}") })
    }

    // v4 `room-characters.ts:85-89`, on the hot turn path.
    #[test]
    fn the_loaded_line_carries_v4s_three_counts() {
        let parts = vec![
            seat("p1", Some("c1"), "llm"),
            seat("p2", Some("c1"), "user"),
        ];
        let lines = crate::test_support::captured(|| {
            let mut find = |_ids: &[String]| Ok(vec![row("c1")]);
            let map = load_room_characters(&parts, &[], &mut find).unwrap();
            assert_eq!(map.len(), 1);
        });
        // Two seats, ONE requested id (deduped), one resolved.
        let hit = lines
            .iter()
            .find(|l| l.contains("[Turn Manager] Room characters loaded"))
            .unwrap_or_else(|| panic!("no loaded line in {lines:?}"));
        assert!(hit.contains("seats=2"), "{hit}");
        assert!(hit.contains("requested=1"), "{hit}");
        assert!(hit.contains("resolved=1"), "{hit}");
    }

    // v4 `room-characters.ts:96-99` — and its SILENCE arm, without which the
    // assertion above passes on a logger that shouts about everything.
    #[test]
    fn the_shortfall_warns_and_a_full_room_does_not() {
        let parts = vec![
            seat("p1", Some("c1"), "llm"),
            seat("p2", Some("shelved"), "llm"),
        ];
        let short = crate::test_support::captured(|| {
            let mut find = |_ids: &[String]| Ok(vec![row("c1")]);
            load_room_characters(&parts, &[], &mut find).unwrap();
        });
        let hit = short
            .iter()
            .find(|l| l.contains("[Turn Manager] Room seats with no readable character"))
            .unwrap_or_else(|| panic!("no shortfall warn in {short:?}"));
        assert!(hit.contains("requested=2"), "{hit}");
        assert!(hit.contains("shelved"), "{hit}");

        let full = crate::test_support::captured(|| {
            let mut find = |_ids: &[String]| Ok(vec![row("c1"), row("shelved")]);
            load_room_characters(&parts, &[], &mut find).unwrap();
        });
        assert!(
            !full
                .iter()
                .any(|l| l.contains("Room seats with no readable character")),
            "a fully-resolved room must not warn: {full:?}"
        );
    }

    // The read error is NOT swallowed: v4's `await` propagates, and so must this.
    #[test]
    fn a_failed_read_propagates() {
        let parts = vec![seat("p1", Some("c1"), "llm")];
        let mut find = |_ids: &[String]| Err(DbError::Internal("boom".into()));
        assert!(load_room_characters(&parts, &[], &mut find).is_err());
    }
}
