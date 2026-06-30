//! The `chats` **participant** ops (the conversation capstone, sub-unit 5). Ports
//! v4's `ChatParticipantsOps`
//! (`lib/database/repositories/chats-participants.ops.ts`): the
//! read-modify-write participant mutators (`addParticipant` / `updateParticipant`
//! / `removeParticipant` / `setParticipantStatus`) plus the four pure in-memory
//! filters.
//!
//! ## The RMW shape
//!
//! Every mutator is the same three-beat: `findById` (the slim-row read,
//! [`chats_read::find_by_id`] — `chats` has no vault overlay) → mutate the
//! `participants` array in memory (minting the participant's own
//! id/`createdAt`/`updatedAt` the v4 way) → `update` the chat with the rewritten
//! `participants` column. The chat's OWN `updatedAt` is **not** passed, so v4's
//! `_update` override preserves it (only a new message bumps a chat's
//! `updatedAt`) — the minted clock values live INSIDE the `participants` JSON.
//!
//! v4 validates the mutated participant through `ChatParticipantBaseSchema.parse`,
//! which materializes the same Zod `.default(...)`s [`ChatParticipant`]'s serde
//! defaults do (`controlledBy: 'llm'`, `displayOrder: 0`, `isActive: true`,
//! `status: 'active'`, `hasHistoryAccess: false`) and strips unknown keys, so the
//! re-serialized array is byte-identical (schema field order).
//!
//! `removeParticipant` carries the **last-participant guard** — refusing to
//! soft-delete the final present participant ([`ParticipantOpError::LastParticipant`],
//! v4's thrown `Error`). `addParticipant` carries the **user-control side-effect**:
//! a `controlledBy: 'user'` participant is appended to `impersonatingParticipantIds`
//! and, when no one is typing yet, becomes `activeTypingParticipantId`.

use rusqlite::Connection;
use serde_json::Value;

use super::chats::{ChatParticipant, ChatUpdate, ChatsRepository};
use super::{chats_read, DbError};
use crate::clock::now_iso;

/// Whether a participant `status` string counts as present in the scene (v4
/// `isParticipantPresent` — `active` or `silent`).
fn is_present(status: &str) -> bool {
    matches!(status, "active" | "silent")
}

/// Error from a participant op. [`LastParticipant`](Self::LastParticipant) is v4's
/// thrown `Error('Cannot remove the last participant from a chat')`, kept distinct
/// from a SQL failure so the harness can assert the guard fires.
#[derive(Debug)]
pub enum ParticipantOpError {
    Db(DbError),
    LastParticipant,
}

impl std::fmt::Display for ParticipantOpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParticipantOpError::Db(e) => write!(f, "{e}"),
            ParticipantOpError::LastParticipant => {
                write!(f, "Cannot remove the last participant from a chat")
            }
        }
    }
}

impl std::error::Error for ParticipantOpError {}

impl From<DbError> for ParticipantOpError {
    fn from(e: DbError) -> Self {
        ParticipantOpError::Db(e)
    }
}

/// Pull the `participants` array out of a marshaled chat ([`chats_read`] shape)
/// as typed [`ChatParticipant`]s (defaults already materialized, nullable
/// optionals dropped — round-trips byte-for-byte on re-serialize).
fn read_participants(chat: &Value) -> Result<Vec<ChatParticipant>, DbError> {
    let arr = chat.get("participants").cloned().unwrap_or(Value::Null);
    serde_json::from_value(arr).map_err(|e| DbError::Key(format!("participants parse: {e}")))
}

/// The chat's `impersonatingParticipantIds` (always present in the marshaled
/// shape — `.default([])`).
fn read_impersonating(chat: &Value) -> Vec<String> {
    chat.get("impersonatingParticipantIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The chat's `activeTypingParticipantId`, present only when non-null (the
/// marshaler drops a NULL cell).
fn read_active_typing(chat: &Value) -> Option<String> {
    chat.get("activeTypingParticipantId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Repository over a borrowed MAIN-db connection (held by the [`super::Writer`]).
pub struct ChatParticipantsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ChatParticipantsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `addParticipant` — mint id + timestamps, validate (applying the Zod
    /// defaults), append, and write. A `controlledBy: 'user'` participant is also
    /// added to `impersonatingParticipantIds` (deduped) and, if no one is typing,
    /// set as `activeTypingParticipantId`. Returns `Ok(false)` when the chat is
    /// absent (v4 `null`).
    pub fn add_participant(&self, chat_id: &str, participant: &Value) -> Result<bool, DbError> {
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok(false);
        };

        let now = now_iso();
        let new_id = uuid::Uuid::new_v4().to_string();
        let mut input = participant.clone();
        let obj = input
            .as_object_mut()
            .ok_or_else(|| DbError::Key("addParticipant: participant not an object".into()))?;
        obj.insert("id".into(), Value::String(new_id.clone()));
        obj.insert("createdAt".into(), Value::String(now.clone()));
        obj.insert("updatedAt".into(), Value::String(now));
        let new_participant: ChatParticipant = serde_json::from_value(input)
            .map_err(|e| DbError::Key(format!("addParticipant parse: {e}")))?;
        let is_user_controlled = new_participant.controlled_by == "user";

        let mut participants = read_participants(&chat)?;
        participants.push(new_participant);

        let mut update = ChatUpdate {
            participants: Some(participants),
            ..Default::default()
        };
        if is_user_controlled {
            let mut impersonating = read_impersonating(&chat);
            if !impersonating.iter().any(|id| id == &new_id) {
                impersonating.push(new_id.clone());
            }
            update.impersonating_participant_ids = Some(impersonating);
            if read_active_typing(&chat).is_none() {
                update.active_typing_participant_id = Some(Some(new_id));
            }
        }

        ChatsRepository::new(self.conn).update(chat_id, &update)?;
        Ok(true)
    }

    /// `updateParticipant` — overlay `data` onto the existing participant, force
    /// its id/`createdAt` and a minted `updatedAt`, re-validate, and write.
    /// Returns `Ok(false)` when the chat or participant is absent.
    pub fn update_participant(
        &self,
        chat_id: &str,
        participant_id: &str,
        data: &Value,
    ) -> Result<bool, DbError> {
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok(false);
        };
        let mut participants = read_participants(&chat)?;
        let Some(idx) = participants.iter().position(|p| p.id == participant_id) else {
            return Ok(false);
        };

        let now = now_iso();
        let existing = &participants[idx];
        let existing_id = existing.id.clone();
        let existing_created = existing.created_at.clone();

        // {...existing, ...data, id, createdAt, updatedAt: now} — the spread,
        // re-validated through the participant schema (schema field order).
        let mut merged = serde_json::to_value(existing)
            .map_err(|e| DbError::Key(format!("updateParticipant ser: {e}")))?;
        if let (Some(dst), Some(src)) = (merged.as_object_mut(), data.as_object()) {
            for (k, v) in src {
                dst.insert(k.clone(), v.clone());
            }
        }
        let obj = merged
            .as_object_mut()
            .expect("participant serializes to an object");
        obj.insert("id".into(), Value::String(existing_id));
        obj.insert("createdAt".into(), Value::String(existing_created));
        obj.insert("updatedAt".into(), Value::String(now));
        participants[idx] = serde_json::from_value(merged)
            .map_err(|e| DbError::Key(format!("updateParticipant parse: {e}")))?;

        let update = ChatUpdate {
            participants: Some(participants),
            ..Default::default()
        };
        ChatsRepository::new(self.conn).update(chat_id, &update)?;
        Ok(true)
    }

    /// `removeParticipant` — soft-delete (`status: 'removed'`, `isActive: false`,
    /// `removedAt: now`). Refuses to remove the **last present** participant
    /// ([`ParticipantOpError::LastParticipant`]). Returns `Ok(false)` when the chat
    /// or participant is absent.
    pub fn remove_participant(
        &self,
        chat_id: &str,
        participant_id: &str,
    ) -> Result<bool, ParticipantOpError> {
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok(false);
        };
        let mut participants = read_participants(&chat)?;
        let Some(idx) = participants.iter().position(|p| p.id == participant_id) else {
            return Ok(false);
        };

        let now = now_iso();
        let p = &mut participants[idx];
        p.status = "removed".to_string();
        p.is_active = false;
        p.removed_at = Some(Some(now.clone()));
        p.updated_at = now;

        // Guard: never strand a chat with no present participants.
        if participants
            .iter()
            .filter(|p| is_present(&p.status))
            .count()
            == 0
        {
            return Err(ParticipantOpError::LastParticipant);
        }

        let update = ChatUpdate {
            participants: Some(participants),
            ..Default::default()
        };
        ChatsRepository::new(self.conn).update(chat_id, &update)?;
        Ok(true)
    }

    /// `setParticipantStatus` — set `status` and keep `isActive`/`removedAt` in
    /// sync (`removedAt: now` only when the new status is `removed`, else explicit
    /// **null**). Returns `(applied, oldStatus)` — `oldStatus` is `'active'` when
    /// the chat/participant is absent (v4's default), `applied = false` then.
    pub fn set_participant_status(
        &self,
        chat_id: &str,
        participant_id: &str,
        new_status: &str,
    ) -> Result<(bool, String), DbError> {
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok((false, "active".to_string()));
        };
        let mut participants = read_participants(&chat)?;
        let Some(idx) = participants.iter().position(|p| p.id == participant_id) else {
            return Ok((false, "active".to_string()));
        };

        let now = now_iso();
        let old_status = participants[idx].status.clone();
        let p = &mut participants[idx];
        p.status = new_status.to_string();
        p.is_active = is_present(new_status);
        p.removed_at = if new_status == "removed" {
            Some(Some(now.clone()))
        } else {
            Some(None) // explicit JSON null (v4 writes `removedAt: null`)
        };
        p.updated_at = now;

        let update = ChatUpdate {
            participants: Some(participants),
            ..Default::default()
        };
        ChatsRepository::new(self.conn).update(chat_id, &update)?;
        Ok((true, old_status))
    }
}

// ============================================================================
// Pure in-memory filters (v4's `get*Participants` — operate on a read chat)
// ============================================================================

/// Character participants (`type === 'CHARACTER'`). `CHARACTER` is the only
/// participant type today, so this is currently an identity filter — ported
/// faithfully against v4 adding more types later.
pub fn get_character_participants(participants: &[ChatParticipant]) -> Vec<ChatParticipant> {
    participants
        .iter()
        .filter(|p| p.participant_type == "CHARACTER")
        .cloned()
        .collect()
}

/// Present participants (`isParticipantPresent(status)` — active or silent).
pub fn get_active_participants(participants: &[ChatParticipant]) -> Vec<ChatParticipant> {
    participants
        .iter()
        .filter(|p| is_present(&p.status))
        .cloned()
        .collect()
}

/// LLM-controlled participants (`controlledBy === 'llm'`).
pub fn get_llm_controlled_participants(participants: &[ChatParticipant]) -> Vec<ChatParticipant> {
    participants
        .iter()
        .filter(|p| p.controlled_by == "llm")
        .cloned()
        .collect()
}

/// User-controlled participants (`controlledBy === 'user'`).
pub fn get_user_controlled_participants(participants: &[ChatParticipant]) -> Vec<ChatParticipant> {
    participants
        .iter()
        .filter(|p| p.controlled_by == "user")
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A participant with the given control mode + status (other fields default).
    fn part(id: &str, controlled_by: &str, status: &str) -> ChatParticipant {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "type": "CHARACTER",
            "characterId": id,
            "controlledBy": controlled_by,
            "status": status,
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z",
        }))
        .unwrap()
    }

    #[test]
    fn pure_filters_match_v4_predicates() {
        let ps = vec![
            part("a", "llm", "active"),
            part("b", "user", "silent"),
            part("c", "llm", "absent"),
            part("d", "user", "removed"),
        ];

        // CHARACTER is the only type today → identity filter (all four).
        assert_eq!(get_character_participants(&ps).len(), 4);

        // present = active | silent → a, b.
        let active: Vec<_> = get_active_participants(&ps)
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(active, vec!["a", "b"]);

        // controlledBy == 'llm' → a, c.
        let llm: Vec<_> = get_llm_controlled_participants(&ps)
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(llm, vec!["a", "c"]);

        // controlledBy == 'user' → b, d.
        let user: Vec<_> = get_user_controlled_participants(&ps)
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(user, vec!["b", "d"]);
    }
}
