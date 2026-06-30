//! The `chat_messages` **write path** (the conversation capstone, sub-unit 4a).
//! Ports the insert surface of v4's `ChatMessagesOps`
//! (`lib/database/repositories/chats-messages.ops.ts`): `addMessage` and
//! `addMessages`. (`updateMessage` / `deleteMessagesByIds` / `clearMessages` are
//! sub-unit 4b.)
//!
//! ## Two halves: the row insert + the chat-metadata side-effect
//!
//! v4 `addMessage` runs `ChatEventSchema.parse(message)` then
//! `insertOne({...validated, chatId})`, and afterwards updates the **chat** row's
//! metadata — `messageCount` (recounted from the live message set), and, **only
//! for an actual `type:'message'` event**, `lastMessageAt` + `updatedAt` (both
//! minted `now`), plus `spokenThisCycleParticipantIds` when the turn cycle
//! advances (`computeSpokenThisCycleAfterMessage`, already ported in
//! [`crate::turn_state`]). `addMessages` is the batch form: insert each, then one
//! metadata update with `spokenThisCycle` **folded** over the batch in order.
//!
//! ## The write marshaling — bake the Zod defaults, fix the key order
//!
//! Unlike the read path (sub-unit 3), the writer must reproduce
//! `ChatEventSchema.parse`'s output bytes itself: materialize each `.default(...)`
//! (`attachments` → `[]`, a `DangerFlag`'s `userOverridden`/`wasRerouted` →
//! `false`) and emit every JSON-column object in **schema field order** (v4's
//! `JSON.stringify` of a Zod-parsed object follows the shape order), with
//! integer-valued numbers rendered bare (`1`, not `1.0`) since the stored bytes
//! are compared directly. Each fixed-shape nested object is a typed struct in
//! schema order; the open-JSON `rawResponse` (`z.record`) is constrained to
//! `{}`/single-key by the corpus (the multi-key insertion-order seam, item 5 in
//! `docs/developer/porting/phase-2-onramp.md`).
//!
//! v4 inserts **only the keys present in the validated event**, so the columns a
//! union member doesn't carry fall to their DDL defaults — `NULL` for every
//! nullable column, and `'[]'` for `attachments` (its `.default([])` becomes a
//! `DEFAULT '[]'` clause). The port mirrors this: a `message` insert names the
//! `MessageEvent` columns (with `attachments` always written); a
//! `context-summary` / `system` insert names only that member's columns and
//! **omits `attachments`** so SQLite fills the same `'[]'` default. The final cell
//! state is therefore byte-identical to v4's. `isSilentMessage` is never written
//! (the read seam — see sub-unit 3 / seam #8).

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::chats::{ChatUpdate, ChatsRepository};
use super::{chats_messages_read, chats_read, js_number_to_json, DbError};
use crate::chat_predicates::ParticipantStatus;
use crate::clock::now_iso;
use crate::turn_state::{compute_spoken_this_cycle_after_message, MessageView, ParticipantView};

// ===========================================================================
// Typed input — a `ChatEvent` (the three-member union), deserialized from a spec
// or caller and serialized (for JSON columns) byte-for-byte like v4's
// Zod-parsed `JSON.stringify`.
// ===========================================================================

/// One `ChatEvent`, internally tagged by `type` (matching `ChatEventSchema`'s
/// discriminator).
#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum ChatEventInput {
    #[serde(rename = "message")]
    Message(Box<MessageEventInput>),
    #[serde(rename = "context-summary")]
    ContextSummary(ContextSummaryInput),
    #[serde(rename = "system")]
    System(SystemEventInput),
}

/// `MessageEventSchema`. Scalar columns bind directly; JSON columns serialize via
/// the typed nested structs (schema order). Nullable-optionals are `Option`
/// (absent → SQL NULL); `attachments` carries the `.default([])`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEventInput {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub raw_response: Option<Value>,
    #[serde(default)]
    pub token_count: Option<f64>,
    #[serde(default)]
    pub prompt_tokens: Option<f64>,
    #[serde(default)]
    pub completion_tokens: Option<f64>,
    #[serde(default)]
    pub swipe_group_id: Option<String>,
    #[serde(default)]
    pub swipe_index: Option<f64>,
    #[serde(default)]
    pub attachments: Vec<String>,
    pub created_at: String,
    #[serde(default)]
    pub debug_memory_logs: Option<Vec<String>>,
    #[serde(default)]
    pub thought_signature: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub reasoning_segments: Option<Vec<ReasoningSegmentIn>>,
    #[serde(default)]
    pub participant_id: Option<String>,
    #[serde(default)]
    pub recovery_type: Option<String>,
    #[serde(default)]
    pub rendered_html: Option<String>,
    #[serde(default)]
    pub danger_flags: Option<Vec<DangerFlagIn>>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub target_participant_ids: Option<Vec<String>>,
    #[serde(default)]
    pub system_sender: Option<String>,
    #[serde(default)]
    pub system_kind: Option<String>,
    #[serde(default)]
    pub opaque_content: Option<String>,
    #[serde(default)]
    pub host_event: Option<HostEventIn>,
    #[serde(default)]
    pub custom_announcer: Option<CustomAnnouncerIn>,
    #[serde(default)]
    pub carina_meta: Option<CarinaMetaIn>,
    #[serde(default)]
    pub pending_external_prompt: Option<String>,
    #[serde(default)]
    pub pending_external_prompt_full: Option<String>,
    #[serde(default)]
    pub pending_external_attachments: Option<Vec<PendingExternalAttachmentIn>>,
    #[serde(default)]
    pub summary_anchor: Option<SummaryAnchorIn>,
}

/// `ContextSummaryEventSchema` — only `id` / `context` / `createdAt`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSummaryInput {
    pub id: String,
    pub context: String,
    pub created_at: String,
}

/// `SystemEventSchema`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemEventInput {
    pub id: String,
    pub system_event_type: String,
    pub description: String,
    #[serde(default)]
    pub prompt_tokens: Option<f64>,
    #[serde(default)]
    pub completion_tokens: Option<f64>,
    #[serde(default)]
    pub total_tokens: Option<f64>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model_name: Option<String>,
    // Explicit rename: serde's camelCase would yield `estimatedCostUsd`, but the
    // schema key keeps the `USD` acronym uppercase.
    #[serde(rename = "estimatedCostUSD", default)]
    pub estimated_cost_usd: Option<f64>,
    pub created_at: String,
}

// --- nested JSON-column shapes (schema order; integer-valued numbers bare) ---

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningSegmentIn {
    #[serde(serialize_with = "ser_js_number")]
    pub anchor_offset: f64,
    pub content: String,
    #[serde(serialize_with = "ser_js_number")]
    pub seq: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DangerFlagIn {
    pub category: String,
    #[serde(serialize_with = "ser_js_number")]
    pub score: f64,
    // Zod `.default(false)` — materialized on deserialize, ALWAYS serialized.
    #[serde(default)]
    pub user_overridden: bool,
    #[serde(default)]
    pub was_rerouted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerouted_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerouted_model: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEventIn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub introduced_character_ids: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAnnouncerIn {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarinaMetaIn {
    pub answerer_id: String,
    pub question: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryAnchorIn {
    #[serde(serialize_with = "ser_js_number")]
    pub compaction_generation: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingExternalAttachmentIn {
    pub file_id: String,
    pub filename: String,
    pub mime_type: String,
    #[serde(serialize_with = "ser_js_number")]
    pub size_bytes: f64,
    pub download_url: String,
}

/// Serialize an `f64` the JS way (an integer-valued double renders bare) for a
/// JSON-column number, since the stored bytes are compared directly.
fn ser_js_number<S: serde::Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    js_number_to_json(*v).serialize(s)
}

// ===========================================================================
// Repository
// ===========================================================================

/// The `chat_messages` write surface over a borrowed MAIN-db connection.
pub struct ChatMessagesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ChatMessagesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `addMessage` — insert one event, then update the owning chat's metadata
    /// (`messageCount`; for a `type:'message'` event also `lastMessageAt` /
    /// `updatedAt` minted `now` and `spokenThisCycle`). No-op metadata if the chat
    /// is gone (v4's `if (chat)` guard).
    pub fn add_message(&self, chat_id: &str, event: &ChatEventInput) -> Result<(), DbError> {
        insert_event(self.conn, chat_id, event)?;
        self.update_chat_metadata(chat_id, std::slice::from_ref(event))
    }

    /// `addMessages` — insert each event in order, then ONE metadata update with
    /// `spokenThisCycle` folded over the batch.
    pub fn add_messages(&self, chat_id: &str, events: &[ChatEventInput]) -> Result<(), DbError> {
        for e in events {
            insert_event(self.conn, chat_id, e)?;
        }
        self.update_chat_metadata(chat_id, events)
    }

    /// The shared metadata side-effect: recount visible messages; for a batch that
    /// contains any actual message bump `lastMessageAt`/`updatedAt` to a freshly
    /// minted `now`; fold `spokenThisCycle` over the batch in order.
    fn update_chat_metadata(
        &self,
        chat_id: &str,
        events: &[ChatEventInput],
    ) -> Result<(), DbError> {
        let Some(chat) = chats_read::find_by_id(self.conn, chat_id)? else {
            return Ok(());
        };

        let all = chats_messages_read::get_messages(self.conn, chat_id)?;
        let message_count = count_visible_messages(&all) as f64;

        let mut update = ChatUpdate {
            message_count: Some(message_count),
            ..Default::default()
        };

        let has_actual = events
            .iter()
            .any(|e| matches!(e, ChatEventInput::Message(_)));
        if has_actual {
            let now = now_iso();
            update.last_message_at = Some(Some(now.clone()));
            update.updated_at = Some(now);
        }

        // Fold each event through the cycle helper in order so a batch that wraps
        // mid-stream lands on the right final state (v4 `addMessages`).
        let participants = participants_from_chat(&chat);
        let mut current = chat
            .get("spokenThisCycleParticipantIds")
            .and_then(Value::as_str)
            .unwrap_or("[]")
            .to_string();
        let mut changed = false;
        for e in events {
            let view = message_view(e);
            if let Some(next) =
                compute_spoken_this_cycle_after_message(&view, &participants, Some(&current))
            {
                current = next;
                changed = true;
            }
        }
        if changed {
            update.spoken_this_cycle_participant_ids = Some(current);
        }

        ChatsRepository::new(self.conn).update(chat_id, &update)?;
        Ok(())
    }
}

/// Count events visible as UI bubbles: `type:'message'` with a role other than
/// `SYSTEM`/`TOOL` (v4 `countVisibleMessages`).
fn count_visible_messages(messages: &[Value]) -> usize {
    messages
        .iter()
        .filter(|m| {
            m.get("type").and_then(Value::as_str) == Some("message")
                && !matches!(
                    m.get("role").and_then(Value::as_str),
                    Some("SYSTEM") | Some("TOOL")
                )
        })
        .count()
}

/// Build the turn-machine view for the spoken-this-cycle helper. Only
/// `type:'message'` events affect the cycle; the others resolve to `None` inside
/// the helper (its first `type` check), so a placeholder role is fine.
fn message_view(event: &ChatEventInput) -> MessageView {
    match event {
        ChatEventInput::Message(m) => MessageView {
            msg_type: Some("message".to_string()),
            role: m.role.clone(),
            participant_id: m.participant_id.clone(),
            target_participant_ids: m.target_participant_ids.clone(),
        },
        ChatEventInput::ContextSummary(_) => MessageView {
            msg_type: Some("context-summary".to_string()),
            role: String::new(),
            participant_id: None,
            target_participant_ids: None,
        },
        ChatEventInput::System(_) => MessageView {
            msg_type: Some("system".to_string()),
            role: String::new(),
            participant_id: None,
            target_participant_ids: None,
        },
    }
}

/// Extract the cycle-relevant participant view from a hydrated chat row.
fn participants_from_chat(chat: &Value) -> Vec<ParticipantView> {
    chat.get("participants")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let id = p.get("id").and_then(Value::as_str)?.to_string();
                    let participant_type = p
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let status = match p.get("status").and_then(Value::as_str) {
                        Some("silent") => ParticipantStatus::Silent,
                        Some("absent") => ParticipantStatus::Absent,
                        Some("removed") => ParticipantStatus::Removed,
                        _ => ParticipantStatus::Active,
                    };
                    let character_id = p
                        .get("characterId")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    Some(ParticipantView {
                        id,
                        participant_type,
                        status,
                        character_id,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ===========================================================================
// Write marshaling — insert one event row
// ===========================================================================

fn insert_event(conn: &Connection, chat_id: &str, event: &ChatEventInput) -> Result<(), DbError> {
    match event {
        ChatEventInput::Message(m) => insert_message(conn, chat_id, m),
        ChatEventInput::ContextSummary(c) => insert_context_summary(conn, chat_id, c),
        ChatEventInput::System(s) => insert_system(conn, chat_id, s),
    }
}

fn insert_message(conn: &Connection, chat_id: &str, m: &MessageEventInput) -> Result<(), DbError> {
    let raw_response = opt_value_json(&m.raw_response)?;
    let attachments = json_text(&m.attachments)?;
    let debug_memory_logs = opt_json(&m.debug_memory_logs)?;
    let reasoning_segments = opt_json(&m.reasoning_segments)?;
    let danger_flags = opt_json(&m.danger_flags)?;
    let target_participant_ids = opt_json(&m.target_participant_ids)?;
    let host_event = opt_json(&m.host_event)?;
    let custom_announcer = opt_json(&m.custom_announcer)?;
    let carina_meta = opt_json(&m.carina_meta)?;
    let pending_external_attachments = opt_json(&m.pending_external_attachments)?;
    let summary_anchor = opt_json(&m.summary_anchor)?;

    conn.execute(
        "INSERT INTO chat_messages (\
           id, chatId, type, role, content, rawResponse, tokenCount, promptTokens, \
           completionTokens, swipeGroupId, swipeIndex, attachments, debugMemoryLogs, \
           thoughtSignature, reasoningContent, reasoningSegments, participantId, recoveryType, \
           renderedHtml, dangerFlags, targetParticipantIds, systemSender, systemKind, \
           opaqueContent, hostEvent, customAnnouncer, carinaMeta, pendingExternalPrompt, \
           pendingExternalPromptFull, pendingExternalAttachments, summaryAnchor, provider, \
           modelName, createdAt) \
         VALUES (\
           ?1, ?2, 'message', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, \
           ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33)",
        rusqlite::params![
            m.id,
            chat_id,
            m.role,
            m.content,
            raw_response,
            m.token_count,
            m.prompt_tokens,
            m.completion_tokens,
            m.swipe_group_id,
            m.swipe_index,
            attachments,
            debug_memory_logs,
            m.thought_signature,
            m.reasoning_content,
            reasoning_segments,
            m.participant_id,
            m.recovery_type,
            m.rendered_html,
            danger_flags,
            target_participant_ids,
            m.system_sender,
            m.system_kind,
            m.opaque_content,
            host_event,
            custom_announcer,
            carina_meta,
            m.pending_external_prompt,
            m.pending_external_prompt_full,
            pending_external_attachments,
            summary_anchor,
            m.provider,
            m.model_name,
            m.created_at,
        ],
    )?;
    Ok(())
}

fn insert_context_summary(
    conn: &Connection,
    chat_id: &str,
    c: &ContextSummaryInput,
) -> Result<(), DbError> {
    // `attachments` omitted → SQLite fills its DDL `DEFAULT '[]'`, matching v4
    // (which inserts only the validated keys).
    conn.execute(
        "INSERT INTO chat_messages (id, chatId, type, context, createdAt) \
         VALUES (?1, ?2, 'context-summary', ?3, ?4)",
        rusqlite::params![c.id, chat_id, c.context, c.created_at],
    )?;
    Ok(())
}

fn insert_system(conn: &Connection, chat_id: &str, s: &SystemEventInput) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO chat_messages (\
           id, chatId, type, systemEventType, description, promptTokens, completionTokens, \
           totalTokens, provider, modelName, estimatedCostUSD, createdAt) \
         VALUES (?1, ?2, 'system', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            s.id,
            chat_id,
            s.system_event_type,
            s.description,
            s.prompt_tokens,
            s.completion_tokens,
            s.total_tokens,
            s.provider,
            s.model_name,
            s.estimated_cost_usd,
            s.created_at,
        ],
    )?;
    Ok(())
}

/// Compact JSON text for a JSON column.
fn json_text<T: Serialize>(v: &T) -> Result<String, DbError> {
    serde_json::to_string(v).map_err(|e| DbError::Key(format!("json serialize: {e}")))
}

/// Optional fixed-shape JSON column: `None` → SQL NULL, `Some` → compact JSON.
fn opt_json<T: Serialize>(v: &Option<T>) -> Result<Option<String>, DbError> {
    match v {
        Some(val) => Ok(Some(json_text(val)?)),
        None => Ok(None),
    }
}

/// Optional open-JSON column (`rawResponse`): `None` → SQL NULL, `Some` → compact
/// JSON. Constrained to `{}`/single-key by the corpus (multi-key insertion-order
/// seam).
fn opt_value_json(v: &Option<Value>) -> Result<Option<String>, DbError> {
    match v {
        Some(val) => Ok(Some(json_text(val)?)),
        None => Ok(None),
    }
}
