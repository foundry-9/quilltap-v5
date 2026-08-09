//! The user-identity resolver (Phase-3 Unit-3 wave 3) — v4
//! `lib/services/chat-message/user-identity-resolver.service.ts`
//! (`resolveUserIdentity`, the four-step fallback chain).
//!
//! Resolves the human user's identity for chat context. The chain, in order:
//!
//!   1. **chat-participant** — a user-controlled character participant already in
//!      the chat. When several characters are user-controlled, we prefer the one
//!      the human is currently "Speaking As" (the active-typing participant) so
//!      the responder's view of "who you're talking to" matches the active
//!      speaker rather than merely the first participant in list order — via the
//!      already-ported [`crate::participant_filters::find_active_user_participant`].
//!   2. **single-user-character** — exactly one user-controlled character exists
//!      system-wide (`characters.findUserControlled` →
//!      [`crate::db::characters_read::find_user_controlled`]).
//!   3. **user-profile** — the user profile's `name`
//!      (`users.findById(userId)?.name` → the scoped
//!      [`crate::db::users::find_name_by_id`]).
//!   4. **default** — the generic `"User"` fallback.
//!
//! Reads only: two possible character reads (both vault-overlaid, main+mount) and
//! one users read. No model call, no write. `chat` is passed in already-loaded (a
//! `serde_json::Value` from [`crate::db::chats_read::find_by_id`]), mirroring v4's
//! `ChatMetadataBase` argument.

use serde_json::Value;

use crate::chat_predicates::ParticipantStatus;
use crate::db::runtime::Db;
use crate::db::{characters_read, users, DbError};
use crate::participant_filters::{find_active_user_participant, ParticipantView};

/// How the identity was resolved (v4 `ResolvedUserIdentity.source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentitySource {
    ChatParticipant,
    SingleUserCharacter,
    UserProfile,
    Default,
}

impl IdentitySource {
    /// The v4 string form (used in the differential).
    pub fn as_str(self) -> &'static str {
        match self {
            IdentitySource::ChatParticipant => "chat-participant",
            IdentitySource::SingleUserCharacter => "single-user-character",
            IdentitySource::UserProfile => "user-profile",
            IdentitySource::Default => "default",
        }
    }
}

/// The resolved identity (v4 `ResolvedUserIdentity`). `character_id` is present
/// only when resolved from a user-controlled character (sources 1 and 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedUserIdentity {
    pub name: String,
    pub description: String,
    pub character_id: Option<String>,
    pub source: IdentitySource,
}

// ---------------------------------------------------------------------------
// Shared participant marshaling (chat JSON → the pure-filter view).
// ---------------------------------------------------------------------------

fn parse_status(s: Option<&str>) -> ParticipantStatus {
    match s.unwrap_or("active") {
        "active" => ParticipantStatus::Active,
        "silent" => ParticipantStatus::Silent,
        "removed" => ParticipantStatus::Removed,
        _ => ParticipantStatus::Absent,
    }
}

fn str_field<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}

/// A participant's non-empty character id (JS truthiness: empty string dropped).
fn nonempty_character_id(p: &Value) -> Option<String> {
    str_field(p, "characterId")
        .filter(|c| !c.is_empty())
        .map(String::from)
}

fn to_filter_participant(p: &Value) -> ParticipantView {
    ParticipantView {
        id: str_field(p, "id").unwrap_or_default().to_string(),
        status: parse_status(str_field(p, "status")),
        controlled_by: str_field(p, "controlledBy").unwrap_or("llm").to_string(),
        character_id: nonempty_character_id(p),
    }
}

fn participants_of(chat: &Value) -> Vec<Value> {
    chat.get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Read a character (vault-overlaid, v4 `repos.characters.findById`).
fn read_character(db: &Db, id: &str) -> Result<Option<Value>, DbError> {
    let id = id.to_string();
    db.read_main(|main| db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &id)))
}

/// Build a [`ResolvedUserIdentity`] from a character JSON object. v4 reads
/// `character.name` (required TEXT) and `character.description || ''` (nullable —
/// omitted from the net read shape → treated as empty string).
fn identity_from_character(character: &Value, source: IdentitySource) -> ResolvedUserIdentity {
    ResolvedUserIdentity {
        name: character
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        description: character
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        character_id: character
            .get("id")
            .and_then(Value::as_str)
            .map(String::from),
        source,
    }
}

/// Resolve the user's identity through the four-step fallback chain (v4
/// `resolveUserIdentity`). `chat` is the already-loaded chat (its `participants`
/// array is read); `active_typing_participant_id` is the human's "Speaking As"
/// selection.
pub async fn resolve_user_identity(
    db: &Db,
    user_id: &str,
    chat: &Value,
    active_typing_participant_id: Option<&str>,
) -> Result<ResolvedUserIdentity, DbError> {
    // Step 1: a user-controlled character participant already in the chat.
    // Prefer the active-typing ("Speaking As") one over the first in order.
    let participants = participants_of(chat);
    let filter_participants: Vec<ParticipantView> =
        participants.iter().map(to_filter_participant).collect();
    // v4 Bug 44: honour the impersonation overlay when resolving the "Speaking As"
    // seat (its durable `controlledBy` is still `'llm'`).
    let impersonating = crate::db::chats_impersonation::read_impersonating(chat);

    let user_controlled = find_active_user_participant(
        &filter_participants,
        active_typing_participant_id,
        Some(&impersonating),
    );

    if let Some(ucp) = user_controlled {
        if let Some(cid) = &ucp.character_id {
            if let Some(character) = read_character(db, cid)? {
                return Ok(identity_from_character(
                    &character,
                    IdentitySource::ChatParticipant,
                ));
            }
        }
    }

    // Step 2: exactly one user-controlled character system-wide.
    let user_controlled_characters = {
        let user_id = user_id.to_string();
        db.read_main(|main| {
            db.read_mount_index(|mount| {
                characters_read::find_user_controlled(main, mount, &user_id)
            })
        })?
    };
    if user_controlled_characters.len() == 1 {
        return Ok(identity_from_character(
            &user_controlled_characters[0],
            IdentitySource::SingleUserCharacter,
        ));
    }

    // Step 3: fall back to the user profile name (falsy → keep falling through).
    let user_name = {
        let user_id = user_id.to_string();
        db.read_main(move |conn| users::find_name_by_id(conn, &user_id))?
    };
    // v4 `userProfile?.name` truthy: a present, non-empty name.
    if let Some(Some(name)) = user_name {
        if !name.is_empty() {
            return Ok(ResolvedUserIdentity {
                name,
                description: String::new(),
                character_id: None,
                source: IdentitySource::UserProfile,
            });
        }
    }

    // Step 4: the generic default.
    Ok(ResolvedUserIdentity {
        name: "User".to_string(),
        description: String::new(),
        character_id: None,
        source: IdentitySource::Default,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_strings_match_v4() {
        assert_eq!(IdentitySource::ChatParticipant.as_str(), "chat-participant");
        assert_eq!(
            IdentitySource::SingleUserCharacter.as_str(),
            "single-user-character"
        );
        assert_eq!(IdentitySource::UserProfile.as_str(), "user-profile");
        assert_eq!(IdentitySource::Default.as_str(), "default");
    }

    #[test]
    fn identity_from_character_reads_name_and_desc() {
        let ch = serde_json::json!({ "id": "c1", "name": "Ada", "description": "an engineer" });
        let r = identity_from_character(&ch, IdentitySource::ChatParticipant);
        assert_eq!(r.name, "Ada");
        assert_eq!(r.description, "an engineer");
        assert_eq!(r.character_id.as_deref(), Some("c1"));
        assert_eq!(r.source, IdentitySource::ChatParticipant);

        // A NULL description (omitted from the net read shape) → empty string.
        let ch2 = serde_json::json!({ "id": "c2", "name": "Bo" });
        let r2 = identity_from_character(&ch2, IdentitySource::SingleUserCharacter);
        assert_eq!(r2.description, "");
    }
}
