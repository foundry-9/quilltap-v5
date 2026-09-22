//! Speaker names for transcripts handed to a cheap LLM — v4
//! `lib/chat/speaker-names.ts` (`e7821606f`, bug 161).
//!
//! A transcript labelled `USER:` / `ASSISTANT:` tells a model which side of the
//! wire a line came from and nothing about who said it. Ask that model for a
//! summary "in character names" and it will supply one — inventing a name for
//! any speaker the dialogue never happens to address by name, then carrying the
//! invention forward through every later fold (bug 161).
//!
//! This module is the one place a `participantId` becomes a display name. The
//! episode pass had a private copy of it and got the right answer; the context
//! summary had none and got "Vivienne". Two implementations of the same map is
//! how that happened, so there is now one.
//!
//! Reads are raw ([`characters_read::find_by_id_raw`]) so a character whose
//! vault is unavailable degrades to a role label rather than throwing into a
//! best-effort pass.
//!
//! ⚠ `e7821606f`'s commit message says the episode pass "was already getting
//! this right" — true of NAMED characters only. Its private loop gated on
//! `if (character)`; the shared resolver gates on `if (character?.name)`, so a
//! character row whose `name` is the EMPTY string now falls back to a role
//! label on the episode side too, where it used to render an empty label.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::characters_read;

/// participantId → display name (v4 `SpeakerNames`, a `ReadonlyMap`). Seats
/// with no resolvable name are absent. Insertion order is v4's `Map` order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpeakerNames {
    entries: Vec<(String, String)>,
}

impl SpeakerNames {
    /// v4 `names.get(id)`.
    pub fn get(&self, participant_id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(id, _)| id == participant_id)
            .map(|(_, n)| n.as_str())
    }

    /// v4 `names.has(id)`.
    pub fn has(&self, participant_id: &str) -> bool {
        self.entries.iter().any(|(id, _)| id == participant_id)
    }

    /// v4 `names.size`.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The `(participantId, name)` pairs in insertion order.
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// v4 `resolveSpeakerNames(chat)` — resolve every seat in a chat to its
/// character's name.
///
/// Iterates **all** participants — removed and silent ones included — because a
/// message from a seat that has since left the chat still deserves its name. The
/// user's own seat is an ordinary `CHARACTER` participant with a `characterId`,
/// so the persona resolves exactly the way an LLM seat does; there is no special
/// case for it.
///
/// Never fails: a failed character read simply leaves the seat unnamed, and
/// [`speaker_label`] falls back to a role label. Reads are sequential, one per
/// seat not yet named (two seats on one character are two reads).
pub fn resolve_speaker_names(conn: &Connection, participants: &[Value]) -> SpeakerNames {
    let mut names = SpeakerNames::default();
    for p in participants {
        // v4 `if (!p.characterId || names.has(p.id)) continue`.
        let Some(character_id) = str_field(p, "characterId").filter(|c| !c.is_empty()) else {
            continue;
        };
        let participant_id = str_field(p, "id").unwrap_or_default();
        if names.has(participant_id) {
            continue;
        }
        match characters_read::find_by_id_raw(conn, character_id) {
            // v4 `if (character?.name)` — an empty name is falsy, so the seat
            // stays absent and the label falls back to the role.
            Ok(Some(character)) => {
                if let Some(name) = str_field(&character, "name").filter(|n| !n.is_empty()) {
                    names
                        .entries
                        .push((participant_id.to_string(), name.to_string()));
                }
            }
            Ok(None) => {}
            // Name stays role-labelled. A broken vault costs a label, not a fold.
            Err(_) => {}
        }
    }
    names
}

/// v4 `speakerLabel(m, names)` — the label a transcript line gets. Never a bare
/// LLM role: an unresolvable seat becomes `User` or `Character`, which the fold
/// prompt is told to keep verbatim rather than to name.
///
/// `participant_id` is v4's `m.participantId` (JS truthiness: `None` and `""`
/// both skip the lookup); a resolved name that is empty falls through too.
pub fn speaker_label(participant_id: Option<&str>, role: &str, names: &SpeakerNames) -> String {
    let resolved = participant_id
        .filter(|id| !id.is_empty())
        .and_then(|id| names.get(id))
        .filter(|n| !n.is_empty());
    match resolved {
        Some(name) => name.to_string(),
        None if role == "USER" => "User".to_string(),
        None => "Character".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn conn_with_characters(rows: &[(&str, &str)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE characters (id TEXT PRIMARY KEY NOT NULL, userId TEXT NOT NULL, \
             name TEXT NOT NULL, defaultImageId TEXT, defaultConnectionProfileId TEXT, \
             defaultPartnerId TEXT, defaultRoleplayTemplateId TEXT, defaultImageProfileId TEXT, \
             sillyTavernData TEXT, isFavorite INTEGER, npc INTEGER, controlledBy TEXT, \
             defaultAgentModeEnabled INTEGER, defaultHelpToolsEnabled INTEGER, \
             defaultTimestampConfig TEXT, defaultScenarioId TEXT, defaultSystemPromptId TEXT, \
             characterDocumentMountPointId TEXT, canDressThemselves INTEGER, \
             canCreateOutfits INTEGER, systemTransparency TEXT, coreWhisperEnabled INTEGER, \
             canBeCarina INTEGER, partnerLinks TEXT, tags TEXT, avatarOverrides TEXT, \
             createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .unwrap();
        for (id, name) in rows {
            conn.execute(
                "INSERT INTO characters (id, userId, name, createdAt, updatedAt) \
                 VALUES (?1, 'u', ?2, 'x', 'x')",
                [id, name],
            )
            .unwrap();
        }
        conn
    }

    /// Tier-3 deferral (P4.D212 item 11): the "broken vault" swallow cannot be
    /// planted in a real-DB oracle — `findByIdRaw` reads the row, never the
    /// vault, so the only throw is a DB error. v4's evidence is its unit
    /// test's mock (`falls back without throwing when a read blows up on a
    /// broken vault`); this is v5's: a connection with no `characters` table
    /// makes every read an `Err`, and the resolver answers an empty map.
    #[test]
    fn a_failed_read_leaves_the_seat_unnamed_without_failing() {
        let conn = Connection::open_in_memory().unwrap();
        let participants = vec![
            json!({"id": "p1", "characterId": "c1"}),
            json!({"id": "p2", "characterId": "c2"}),
        ];
        let names = resolve_speaker_names(&conn, &participants);
        assert!(names.is_empty());
        assert_eq!(speaker_label(Some("p1"), "ASSISTANT", &names), "Character");
        assert_eq!(speaker_label(Some("p2"), "USER", &names), "User");
    }

    #[test]
    fn a_failed_read_on_one_seat_does_not_cost_the_others() {
        let conn = conn_with_characters(&[("c1", "Aria")]);
        // A seat whose character row is gone, before a resolving one: the
        // miss costs only its own label.
        let participants = vec![
            json!({"id": "p0", "characterId": "missing"}),
            json!({"id": "p1", "characterId": "c1"}),
        ];
        let names = resolve_speaker_names(&conn, &participants);
        assert_eq!(names.entries(), &[("p1".to_string(), "Aria".to_string())]);
    }

    #[test]
    fn an_empty_name_is_absent_and_labels_by_role() {
        let conn = conn_with_characters(&[("c1", "")]);
        let names = resolve_speaker_names(&conn, &[json!({"id": "p1", "characterId": "c1"})]);
        assert!(!names.has("p1"));
        assert_eq!(speaker_label(Some("p1"), "ASSISTANT", &names), "Character");
    }
}
