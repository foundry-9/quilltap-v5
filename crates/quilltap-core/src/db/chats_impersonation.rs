//! The `chats` **impersonation** ops (the conversation capstone, sub-unit 6).
//! Ports v4's `ChatImpersonationOps`
//! (`lib/database/repositories/chats-impersonation.ops.ts`):
//! `addImpersonation` / `removeImpersonation` / `getImpersonatedParticipantIds`
//! / `setActiveTypingParticipant` / `updateAllLLMPauseTurnCount`.
//!
//! ## The RMW shape
//!
//! Each mutator is the same beat the participant ops use: `findById` (the
//! slim-row read, [`chats_read::find_by_id`] — `chats` has no vault overlay) →
//! decide the new `impersonatingParticipantIds` / `activeTypingParticipantId` in
//! memory → `update` the chat with those two columns. The chat's OWN `updatedAt`
//! is **not** passed, so v4's `_update` override preserves it (these ops mint
//! NO timestamps and NO ids — they only rewrite three columns).
//!
//! v4 always passes BOTH `impersonatingParticipantIds` and
//! `activeTypingParticipantId` to `update` in `addImpersonation` /
//! `removeImpersonation` (the latter possibly `null`), so the port mirrors that:
//! [`ChatUpdate::active_typing_participant_id`] is always `Some(_)` on those two
//! paths (`Some(Some(id))` to set, `Some(None)` to clear to SQL NULL).
//! `setActiveTypingParticipant` writes ONLY `activeTypingParticipantId`;
//! `updateAllLLMPauseTurnCount` writes ONLY `allLLMPauseTurnCount` (and does NOT
//! `findById` first — it is a bare `update`, so it is a no-op when the row is
//! absent).

use rusqlite::Connection;
use serde_json::Value;

use super::chats::{ChatUpdate, ChatsRepository};
use super::{chats_read, DbError};

/// The chat's `impersonatingParticipantIds` (always present in the marshaled
/// shape — `.default([])`), but tolerate absence the v4 `|| []` way.
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

/// Whether a participant with this id exists in the chat's `participants` array.
fn participant_exists(chat: &Value, participant_id: &str) -> bool {
    chat.get("participants")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .any(|p| p.get("id").and_then(Value::as_str) == Some(participant_id))
        })
        .unwrap_or(false)
}

/// Repository over a borrowed MAIN-db connection (held by the [`super::Writer`]).
pub struct ChatImpersonationRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ChatImpersonationRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `addImpersonation` — verify the chat and participant exist, append the
    /// participant id to `impersonatingParticipantIds` (deduped), and set it as
    /// `activeTypingParticipantId` only when none is set yet
    /// (`activeTyping || participantId`). Returns `Ok(false)` when the chat is
    /// absent OR the participant is not in the chat (both v4 `null`).
    pub fn add_impersonation(&self, chat_id: &str, participant_id: &str) -> Result<bool, DbError> {
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok(false);
        };
        if !participant_exists(&chat, participant_id) {
            return Ok(false);
        }

        let mut impersonating = read_impersonating(&chat);
        if !impersonating.iter().any(|id| id == participant_id) {
            impersonating.push(participant_id.to_string());
        }

        // activeTyping = chat.activeTypingParticipantId || participantId
        let active_typing = read_active_typing(&chat).unwrap_or_else(|| participant_id.to_string());

        let update = ChatUpdate {
            impersonating_participant_ids: Some(impersonating),
            active_typing_participant_id: Some(Some(active_typing)),
            ..Default::default()
        };
        ChatsRepository::new(self.conn).update(chat_id, &update)?;
        Ok(true)
    }

    /// `removeImpersonation` — drop the participant id from
    /// `impersonatingParticipantIds`; if it was the active typer, reassign to the
    /// first remaining impersonated id (or SQL NULL when none remain). Returns
    /// `Ok(false)` when the chat is absent (v4 `null`). NOTE: v4 does NOT verify
    /// the participant exists here — it filters unconditionally and writes.
    pub fn remove_impersonation(
        &self,
        chat_id: &str,
        participant_id: &str,
    ) -> Result<bool, DbError> {
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok(false);
        };

        let impersonating: Vec<String> = read_impersonating(&chat)
            .into_iter()
            .filter(|id| id != participant_id)
            .collect();

        // Clear active typing if it was this participant: reassign to the first
        // remaining impersonated id, else null.
        let mut active_typing = read_active_typing(&chat);
        if active_typing.as_deref() == Some(participant_id) {
            active_typing = impersonating.first().cloned();
        }

        let update = ChatUpdate {
            impersonating_participant_ids: Some(impersonating),
            active_typing_participant_id: Some(active_typing),
            ..Default::default()
        };
        ChatsRepository::new(self.conn).update(chat_id, &update)?;
        Ok(true)
    }

    /// `getImpersonatedParticipantIds` — the chat's `impersonatingParticipantIds`
    /// (v4 `|| []`), or `[]` when the chat is absent.
    pub fn get_impersonated_participant_ids(&self, chat_id: &str) -> Result<Vec<String>, DbError> {
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok(Vec::new());
        };
        Ok(read_impersonating(&chat))
    }

    /// `setActiveTypingParticipant` — set `activeTypingParticipantId` to the given
    /// id (or SQL NULL to clear). When setting a non-null id, it must already be
    /// in `impersonatingParticipantIds` (else v4 warns + returns `null`, a no-op).
    /// Returns `Ok(false)` when the chat is absent OR the id is not impersonated.
    pub fn set_active_typing_participant(
        &self,
        chat_id: &str,
        participant_id: Option<&str>,
    ) -> Result<bool, DbError> {
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok(false);
        };

        if let Some(pid) = participant_id {
            let impersonating = read_impersonating(&chat);
            if !impersonating.iter().any(|id| id == pid) {
                // v4: logger.warn(...) then return null — a no-op, no write.
                return Ok(false);
            }
        }

        let update = ChatUpdate {
            active_typing_participant_id: Some(participant_id.map(str::to_string)),
            ..Default::default()
        };
        ChatsRepository::new(self.conn).update(chat_id, &update)?;
        Ok(true)
    }

    /// `updateAllLLMPauseTurnCount` — bare `update` of `allLLMPauseTurnCount` (no
    /// `findById` first). Returns `Ok(false)` when the row is missing (v4's
    /// `update` returns `null`), else `Ok(true)`.
    pub fn update_all_llm_pause_turn_count(
        &self,
        chat_id: &str,
        count: f64,
    ) -> Result<bool, DbError> {
        let update = ChatUpdate {
            all_llm_pause_turn_count: Some(count),
            ..Default::default()
        };
        let n = ChatsRepository::new(self.conn).update(chat_id, &update)?;
        Ok(n)
    }
}
