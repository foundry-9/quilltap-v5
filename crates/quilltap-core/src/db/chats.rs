//! The `chats` repository — slim-row marshaling (Phase-2, the conversation
//! capstone, sub-unit 1). Ports the `create` / `update` / `delete` of v4's
//! `ChatsRepository` (`lib/database/repositories/chats.repository.ts`) over the
//! base `_create`/`_update`/`_delete` internals, for the MAIN-db `chats` table
//! (`lib/schemas/chat.types.ts` `ChatMetadataSchema`, ~96 columns).
//!
//! `chats` extends `TaggableBaseRepository` → `UserOwnedBaseRepository` →
//! `AbstractBaseRepository`, so it carries `userId` + `tags`; the write path has
//! no base-method override beyond what's below.
//!
//! ## Two non-obvious write invariants
//!
//!   - **`update` never mints `updatedAt`.** v4's override preserves the existing
//!     `updatedAt` unless the caller explicitly passes one (only a *new message*
//!     bumps a chat's modified time; background jobs must not). So this whole
//!     sub-unit's differential is the **pinned, zero-normalization** form — no
//!     minted timestamp ever appears.
//!   - **`create` writes nothing to `chat_messages` on SQLite** (messages are
//!     individual rows added later); the legacy empty-doc path is non-SQLite
//!     only. And `delete`'s participant-vault summary sweep is an external
//!     subsystem (`conversation-summary-vault-bridge`) that touches vault files,
//!     not the `chats` table — **deferred** here (tracked), so the port's
//!     `delete` is the slim row drop + the `chat_messages` cleanup.
//!
//! ## Marshaling surface (the widest in Phase 2)
//!
//!   - `participants` is the **typed array-of-objects JSON column**
//!     ([`ChatParticipant`], 18 fields in schema order, `skip_serializing_if` on
//!     the nullable optionals; `displayOrder` an `i64`, `talkativeness` rendered
//!     the JS way so an integer-valued `1.0` → `1`). The schema `.refine()`
//!     requires ≥1 participant, so it is never empty.
//!   - the simple JSON-array columns (`tags`, `summaryAnchorMessageIds`,
//!     `impersonatingParticipantIds`, `disabledTools`, `disabledToolGroups`,
//!     `dangerCategories`) are `Vec<String>` → compact JSON text.
//!   - `turnQueue` / `spokenThisCycleParticipantIds` are **plain `z.string()`**
//!     columns holding JSON text (`'[]'`) — bound as the raw string, NOT a JSON
//!     column.
//!   - every numeric column binds `f64` (REAL or INTEGER affinity both collapse
//!     correctly via the dump's `js_number_to_json`); booleans bind 0/1; the many
//!     nullable strings/uuids/enums/timestamps bind SQL NULL when absent.
//!   - the open-JSON object columns (`state` default `{}`, and the nullable
//!     `sillyTavernMetadata` / `timestampConfig` / `sceneState` / `equippedOutfit`
//!     / … ) are `serde_json::Value`, which is key-order-faithful here: the crate
//!     enables serde_json's `preserve_order`, so a `Value::Object` emits
//!     insertion order like v4's `JSON.stringify`. (The multi-key
//!     insertion-order seam this sub-unit once tracked is closed.) The corpus
//!     still keeps them `{}` / single-key / null, having no need for more.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::DbError;

/// One entry of the `participants` JSON-array column — v4 `ChatParticipantBase`
/// in schema field order. Serializes to the object `JSON.stringify` produces
/// (nullable optionals omitted when `None`); deserializes from a (possibly
/// sparse) spec entry, filling the same Zod `.default()`s so both sides resolve
/// identically.
#[derive(Serialize, Deserialize, Clone)]
pub struct ChatParticipant {
    pub id: String,
    #[serde(rename = "type")]
    pub participant_type: String,
    #[serde(rename = "characterId")]
    pub character_id: String,
    #[serde(rename = "controlledBy", default = "default_llm")]
    pub controlled_by: String,
    /// Double-`Option` (see [`ChatParticipant::removed_at`] for the full
    /// rationale): v4 `buildCharacterParticipant` writes this as
    /// `isUserControlled ? null : data.connectionProfileId || null` — **always
    /// present**, `null` when falsy — and v4's `.nullable().optional()` KEEPS
    /// the stored `null`, so a v4-created participant carries an explicit
    /// `"connectionProfileId":null`. Plain `Option<String>` would collapse that
    /// null to `None` and DROP the key on re-serialization; the double-`Option`
    /// with [`de_double_opt_string`] preserves all three on-disk shapes
    /// (absent, `null`, string) byte-for-byte.
    #[serde(
        rename = "connectionProfileId",
        default,
        deserialize_with = "de_double_opt_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub connection_profile_id: Option<Option<String>>,
    /// Double-`Option` — v4 writes `data.imageProfileId || null` (always
    /// present, `null` when falsy). See [`ChatParticipant::connection_profile_id`].
    #[serde(
        rename = "imageProfileId",
        default,
        deserialize_with = "de_double_opt_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub image_profile_id: Option<Option<String>>,
    #[serde(
        rename = "roleplayTemplateId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub roleplay_template_id: Option<String>,
    /// Double-`Option` — v4 writes `data.selectedSystemPromptId || null` (always
    /// present, `null` when falsy). See [`ChatParticipant::connection_profile_id`].
    #[serde(
        rename = "selectedSystemPromptId",
        default,
        deserialize_with = "de_double_opt_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub selected_system_prompt_id: Option<Option<String>>,
    #[serde(rename = "displayOrder", default)]
    pub display_order: i64,
    #[serde(rename = "isActive", default = "default_true")]
    pub is_active: bool,
    #[serde(default = "default_active")]
    pub status: String,
    /// Soft-delete timestamp. A double-`Option` so the participant ops can write
    /// the three distinct on-disk shapes v4 produces: `None` → key **absent**
    /// (create / a never-removed participant — v4 `undefined`, dropped by
    /// `JSON.stringify`); `Some(None)` → explicit JSON **`null`** (v4
    /// `setParticipantStatus` to a non-removed status writes `removedAt: null`);
    /// `Some(Some(ts))` → the string (v4 `removeParticipant`). The double-option
    /// **deserializer** is required: plain serde maps a JSON `null` to the OUTER
    /// `None` (dropping it), but v4's Zod `.nullable().optional()` KEEPS a stored
    /// `null` (a cleared `removedAt` survives a re-read + re-write), so
    /// [`de_double_opt_string`] forces present→`Some(_)`. Serialization is
    /// serde-default (`Some(None)` → `null`, `Some(Some)` → string, `None`
    /// skipped).
    #[serde(
        rename = "removedAt",
        default,
        deserialize_with = "de_double_opt_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub removed_at: Option<Option<String>>,
    #[serde(rename = "hasHistoryAccess", default)]
    pub has_history_access: bool,
    #[serde(
        rename = "joinScenario",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub join_scenario: Option<String>,
    #[serde(
        rename = "talkativeness",
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_opt_js_number"
    )]
    pub talkativeness: Option<f64>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// Double-option deserializer: a PRESENT field (even JSON `null`) becomes
/// `Some(_)`; an ABSENT field falls to the `#[serde(default)]` `None`. Lets a
/// stored `removedAt: null` round-trip as `Some(None)` instead of collapsing to
/// the outer `None` (matching v4's `.nullable().optional()` keep-null).
fn de_double_opt_string<'de, D>(de: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(de)?))
}

fn default_llm() -> String {
    "llm".to_string()
}
fn default_true() -> bool {
    true
}
fn default_active() -> String {
    "active".to_string()
}

/// Serialize an `Option<f64>` the JS way for the participant JSON: an
/// integer-valued double renders bare (`1.0` → `1`), matching `JSON.stringify`.
/// Only called when `Some` (the field is `skip_serializing_if` on `None`).
fn ser_opt_js_number<S: serde::Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(n) => super::js_number_to_json(*n).serialize(s),
        None => s.serialize_none(),
    }
}

/// Create fields — the post-default `Omit<ChatMetadata,'id'|'createdAt'|
/// 'updatedAt'>` shape. `userId` / `title` / `participants` are required (the
/// schema `.refine()` needs ≥1 participant); everything else carries its Zod
/// default via `serde(default …)`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCreate {
    pub user_id: String,
    pub title: String,
    pub participants: Vec<ChatParticipant>,

    #[serde(default)]
    pub context_summary: Option<String>,
    #[serde(default)]
    pub silly_tavern_metadata: Option<Value>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub roleplay_template_id: Option<String>,
    #[serde(default)]
    pub timestamp_config: Option<Value>,
    #[serde(default)]
    pub last_turn_participant_id: Option<String>,
    #[serde(default)]
    pub message_count: f64,
    #[serde(default)]
    pub last_message_at: Option<String>,
    #[serde(default)]
    pub last_rename_check_interchange: f64,
    #[serde(default)]
    pub compaction_generation: f64,
    #[serde(default)]
    pub last_summary_turn: f64,
    #[serde(default)]
    pub last_summary_tokens: f64,
    #[serde(default)]
    pub last_full_rebuild_turn: f64,
    #[serde(default)]
    pub summary_anchor_message_ids: Vec<String>,
    #[serde(default)]
    pub is_paused: bool,
    #[serde(default)]
    pub is_manually_renamed: bool,
    #[serde(default)]
    pub impersonating_participant_ids: Vec<String>,
    #[serde(default)]
    pub active_typing_participant_id: Option<String>,
    #[serde(default)]
    pub all_llm_pause_turn_count: f64,
    #[serde(default = "default_empty_json_array_str")]
    pub turn_queue: String,
    #[serde(default = "default_empty_json_array_str")]
    pub spoken_this_cycle_participant_ids: String,
    #[serde(default)]
    pub document_editing_mode: bool,
    #[serde(default = "default_normal")]
    pub document_mode: String,
    #[serde(default = "default_45")]
    pub divider_position: f64,
    #[serde(default = "default_normal")]
    pub terminal_mode: String,
    #[serde(default)]
    pub active_terminal_session_id: Option<String>,
    #[serde(default = "default_50")]
    pub right_pane_vertical_split: f64,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub scenario_text: Option<String>,
    #[serde(default)]
    pub total_prompt_tokens: f64,
    #[serde(default)]
    pub total_completion_tokens: f64,
    #[serde(default)]
    pub estimated_cost_usd: Option<f64>,
    #[serde(default)]
    pub price_source: Option<String>,
    #[serde(default)]
    pub show_system_events_override: Option<bool>,
    #[serde(default)]
    pub request_full_context_on_next_message: bool,
    #[serde(default)]
    pub disabled_tools: Vec<String>,
    #[serde(default)]
    pub disabled_tool_groups: Vec<String>,
    #[serde(default)]
    pub force_tools_on_next_message: bool,
    #[serde(default)]
    pub allow_cross_character_vault_reads: bool,
    #[serde(default)]
    pub pending_outfit_notifications: Option<Value>,
    #[serde(default = "default_empty_json_object")]
    pub state: Value,
    #[serde(default)]
    pub compression_cache: Option<Value>,
    #[serde(default)]
    pub agent_mode_enabled: Option<bool>,
    #[serde(default)]
    pub agent_turn_count: f64,
    #[serde(default)]
    pub story_background_image_id: Option<String>,
    #[serde(default)]
    pub last_background_generated_at: Option<String>,
    #[serde(default)]
    pub image_profile_id: Option<String>,
    #[serde(default)]
    pub alert_characters_of_lantern_images: Option<bool>,
    #[serde(default)]
    pub is_dangerous_chat: Option<bool>,
    #[serde(default)]
    pub danger_score: Option<f64>,
    #[serde(default)]
    pub danger_categories: Vec<String>,
    #[serde(default)]
    pub danger_classified_at: Option<String>,
    #[serde(default)]
    pub danger_classified_at_message_count: Option<f64>,
    #[serde(default)]
    pub concierge_override: Option<String>,
    /// Per-chat answer-confirmation override: `z.enum(['ON','OFF']).nullable()
    /// .optional()`; `None` => SQL NULL. Added by v4
    /// `add-answer-confirmation-columns-v2`.
    #[serde(default)]
    pub answer_confirmation_override: Option<String>,
    #[serde(default)]
    pub scene_state: Option<Value>,
    #[serde(default)]
    pub rendered_markdown: Option<String>,
    #[serde(default)]
    pub equipped_outfit: Option<Value>,
    #[serde(default)]
    pub character_avatars: Option<Value>,
    #[serde(default)]
    pub avatar_generation_enabled: Option<bool>,
    #[serde(default = "default_salon")]
    pub chat_type: String,
    #[serde(default)]
    pub help_page_url: Option<String>,
    #[serde(default)]
    pub console_connection_profile_id: Option<String>,
    #[serde(default)]
    pub compiled_identity_stacks: Option<Value>,
    #[serde(default)]
    pub courier_checkpoints: Option<Value>,
    #[serde(default)]
    pub commonplace_scene_cache: Option<Value>,
    #[serde(default)]
    pub commonplace_recall_history: Option<Value>,
    /// Episodic spine (v4 8bf3cb5f): which clock the story runs on —
    /// `'realtime'` | `'narrative'`; `None` (NULL) reads as realtime.
    #[serde(default)]
    pub timeline_mode: Option<String>,
    #[serde(default)]
    pub budget_max_turns: Option<f64>,
    #[serde(default)]
    pub budget_max_tokens: Option<f64>,
    #[serde(default)]
    pub budget_max_wall_clock_ms: Option<f64>,
    #[serde(default)]
    pub budget_estimated_spend_cap_usd: Option<f64>,
    #[serde(default)]
    pub schedule_cron: Option<String>,
    #[serde(default)]
    pub schedule_freshness_window_ms: Option<f64>,
    #[serde(default)]
    pub schedule_next_run_at: Option<String>,
    #[serde(default)]
    pub schedule_last_run_at: Option<String>,
    #[serde(default)]
    pub run_state: Option<String>,
    #[serde(default)]
    pub current_run_id: Option<String>,
    #[serde(default)]
    pub run_state_message: Option<String>,
    #[serde(default)]
    pub run_started_at: Option<String>,
    #[serde(default)]
    pub run_ended_at: Option<String>,
    #[serde(default)]
    pub run_paused_at: Option<String>,
    #[serde(default)]
    pub run_paused_accum_ms: Option<f64>,
    #[serde(default)]
    pub run_turns_consumed: Option<f64>,
    #[serde(default)]
    pub run_tokens_consumed: Option<f64>,
    #[serde(default)]
    pub run_milestones_announced: f64,
    #[serde(default)]
    pub run_destructive_tools_allowed: f64,
    #[serde(default = "default_one")]
    pub budget_exclude_cache_hits: f64,
    #[serde(default)]
    pub run_visibility: Option<String>,
    #[serde(default)]
    pub core_whisper_enabled: Option<bool>,
    #[serde(default)]
    pub core_whisper_interval: Option<f64>,
    /// "Nothing to add" turn-skipping — per-chat toggle:
    /// `z.boolean().nullable().optional()`. NULL = enabled (the default);
    /// false = disabled. Schema position between `coreWhisperInterval` and
    /// `showThinking` (v4 b90cd1f5).
    #[serde(default)]
    pub turn_skipping_enabled: Option<bool>,
    #[serde(default)]
    pub show_thinking: Option<bool>,
}

fn default_empty_json_array_str() -> String {
    "[]".to_string()
}
fn default_normal() -> String {
    "normal".to_string()
}
fn default_salon() -> String {
    "salon".to_string()
}
fn default_45() -> f64 {
    45.0
}
fn default_50() -> f64 {
    50.0
}
fn default_one() -> f64 {
    1.0
}
fn default_empty_json_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A focused update patch for sub-unit 1 (representative columns exercising the
/// scalar / bool / number / JSON-array / open-JSON / enum paths + the
/// updatedAt-preservation invariant). Each `Some` sets that column; nullable
/// columns use `Option<Option<_>>`. `updated_at`: `Some` → use it; `None` →
/// preserve the row's existing `updatedAt` (v4's override). Later sub-units add
/// the remaining column setters.
#[derive(Default)]
pub struct ChatUpdate {
    pub title: Option<String>,
    pub context_summary: Option<Option<String>>,
    pub is_paused: Option<bool>,
    pub is_manually_renamed: Option<bool>,
    pub message_count: Option<f64>,
    pub danger_score: Option<Option<f64>>,
    pub chat_type: Option<String>,
    pub state: Option<Value>,
    pub tags: Option<Vec<String>>,
    /// The `participants` JSON-array column — set by the participant RMW ops
    /// ([`super::chats_participants`]). Re-serialized in [`ChatParticipant`]
    /// schema-field order, matching v4's `JSON.stringify` of the spread array.
    pub participants: Option<Vec<ChatParticipant>>,
    /// `impersonatingParticipantIds` (JSON string-array) — set when a
    /// user-controlled participant is added.
    pub impersonating_participant_ids: Option<Vec<String>>,
    /// Nullable `activeTypingParticipantId`. `Some(Some(id))` sets it;
    /// `Some(None)` clears to SQL NULL; `None` leaves it unset.
    pub active_typing_participant_id: Option<Option<String>>,
    /// Nullable timestamp column. `Some(Some(ts))` sets it; `Some(None)` clears it
    /// to SQL NULL; `None` leaves it unset. Set by the message-write metadata path
    /// ([`super::chats_messages`]) — an actual message bumps it to `now`.
    pub last_message_at: Option<Option<String>>,
    /// The plain-string `spokenThisCycleParticipantIds` column (holds JSON text);
    /// set by the message-write metadata path when the turn cycle advances.
    pub spoken_this_cycle_participant_ids: Option<String>,
    /// The plain-string `turnQueue` column (holds JSON text) — set by the
    /// turn-orchestration decision core ([`crate::services::turn_orchestrator`]):
    /// the queue-pop write-back and the turn-action mutators.
    pub turn_queue: Option<String>,
    /// Nullable `lastTurnParticipantId`. `Some(Some(id))` sets it; `Some(None)`
    /// clears it to SQL NULL (v4 writes `participantId ?? null`); `None` leaves it
    /// unset. Set by the turn-orchestration decision core.
    pub last_turn_participant_id: Option<Option<String>>,
    /// `allLLMPauseTurnCount` (REAL) — set by the impersonation ops
    /// ([`super::chats_impersonation`]).
    pub all_llm_pause_turn_count: Option<f64>,
    /// `totalPromptTokens` (REAL) — set by the token-tracking ops
    /// ([`super::chats_tokens`]) on reset.
    pub total_prompt_tokens: Option<f64>,
    /// `totalCompletionTokens` (REAL) — set by the token-tracking ops on reset.
    pub total_completion_tokens: Option<f64>,
    /// Nullable `estimatedCostUSD` (REAL). `Some(Some(v))` sets it; `Some(None)`
    /// clears it to SQL NULL (token reset); `None` leaves it unset.
    pub estimated_cost_usd: Option<Option<f64>>,
    /// Nullable `equippedOutfit` (JSON object) — set by the outfit ops
    /// ([`super::chats_outfits`]). `Some(Some(v))` sets it; `Some(None)` clears to
    /// SQL NULL; `None` leaves it unset.
    pub equipped_outfit: Option<Option<Value>>,
    /// `compactionGeneration` (REAL) — bumped by the context-summary fold write
    /// and the summary invalidation ([`crate::services::context_summary`]).
    pub compaction_generation: Option<f64>,
    /// `lastSummaryTurn` (REAL) — the fold-cadence watermark, set by the
    /// context-summary fold write (reset to 0 on invalidation).
    pub last_summary_turn: Option<f64>,
    /// `lastSummaryTokens` (REAL) — set by the context-summary fold write.
    pub last_summary_tokens: Option<f64>,
    /// `lastFullRebuildTurn` (REAL) — set on a hard rebuild (reset to 0 on
    /// invalidation).
    pub last_full_rebuild_turn: Option<f64>,
    /// `summaryAnchorMessageIds` (JSON string-array) — the fold-input message ids
    /// the edit/delete invalidation hook intersects against.
    pub summary_anchor_message_ids: Option<Vec<String>>,
    /// `lastRenameCheckInterchange` (REAL) — the title-check watermark, set when
    /// the interchange-count checkpoint fires.
    pub last_rename_check_interchange: Option<f64>,
    /// Nullable `roleplayTemplateId` — the chat's own active template. Persisted
    /// by the roleplay-template fallback chain
    /// ([`crate::services::participant_resolver`]) when an older / imported chat
    /// inherits a default (v4 `repos.chats.update(chat.id, { roleplayTemplateId })`).
    /// `Some(Some(id))` sets it; `Some(None)` clears to SQL NULL; `None` leaves it
    /// unset.
    pub roleplay_template_id: Option<Option<String>>,
    /// Nullable `projectId` — set by the qtap import's reconcile pass
    /// ([`crate::services::quilltap_import::reconcile`], v4
    /// `repos.chats.update(newId, { projectId })`) when an imported chat's
    /// project FK remaps onto an imported project. `Some(Some(id))` sets it;
    /// `Some(None)` clears to SQL NULL; `None` leaves it unset.
    pub project_id: Option<Option<String>>,
    /// `agentTurnCount` (REAL) — set by the native tool loop in agent mode
    /// ([`crate::services::native_tool_loop`]) once per iteration
    /// (`repos.chats.update(chatId, { agentTurnCount })`, no `updatedAt` bump).
    pub agent_turn_count: Option<f64>,
    /// `documentMode` (enum TEXT: `normal` / `split` / …) — the split-panel editor
    /// mode, set by the document-UI doc-edit tools
    /// ([`crate::tools::doc_edit::document_ui`]) on open/close.
    /// `repos.chats.update(chatId, { documentMode })` — no `updatedAt` bump
    /// (the chats `_update` preserves `updatedAt` unless the caller passes one).
    pub document_mode: Option<String>,
    /// `compiledIdentityStacks` (nullable JSON object) — the per-participant
    /// identity-stack cache written by the system-prompt compiler
    /// ([`crate::services::system_prompt_compiler`], v4
    /// `repos.chats.update(chatId, { compiledIdentityStacks })`). `Some(obj)`
    /// stores the map; `Some(Value::Null)` clears the column to SQL NULL (v4's
    /// `keys > 0 ? stacks : null`); `None` leaves it unset. No `updatedAt` bump.
    pub compiled_identity_stacks: Option<Value>,
    /// `compressionCache` (nullable JSON object) — the async pre-compression cache
    /// ([`crate::services::compression_cache`]) `persistToDatabase` /
    /// `clearFromDatabase`. `Some(obj)` stores the cache object; `Some(Value::Null)`
    /// clears the column to SQL NULL; `None` leaves it unset. No `updatedAt` bump.
    pub compression_cache: Option<Value>,
    /// `sceneState` (nullable JSON object) — persisted by the scene-state tracking
    /// job ([`crate::services::scene_state_tracking`], v4
    /// `repos.chats.update(chatId, { sceneState })`). `Some(obj)` stores the object;
    /// `Some(Value::Null)` clears the column to SQL NULL; `None` leaves it unset.
    /// No `updatedAt` bump (v4 `_update` preserves it).
    pub scene_state: Option<Value>,
    /// `commonplaceSceneCache` (nullable JSON object) — the per-recipient scene-state
    /// emission cache, persisted by `buildContext`'s Commonplace tail
    /// ([`crate::services::build_context`] `persist_scene_cache`, W4.6a; v4
    /// `repos.chats.update(chatId, { commonplaceSceneCache })`). `Some(obj)` stores;
    /// `Some(Value::Null)` clears; `None` leaves unset. No `updatedAt` bump.
    pub commonplace_scene_cache: Option<Value>,
    /// `commonplaceRecallHistory` (nullable JSON array) — the whispered-memory-id
    /// recall history, persisted by `buildContext`'s Commonplace tail
    /// ([`crate::services::build_context`] `persist_recall_history`, W4.6a; v4
    /// `repos.chats.update(chatId, { commonplaceRecallHistory })`). `Some(arr)`
    /// stores; `Some(Value::Null)` clears; `None` leaves unset. No `updatedAt` bump.
    pub commonplace_recall_history: Option<Value>,
    /// `timelineMode` (nullable enum TEXT — the episodic spine, v4 8bf3cb5f):
    /// `Some(Some(mode))` stores, `Some(None)` clears to NULL (realtime),
    /// `None` leaves unset. Written in production by the Salon sidebar's
    /// Story's Clock switch (P4.9H1) through the chat-PUT `timelineMode` arm.
    pub timeline_mode: Option<Option<String>>,
    /// `courierCheckpoints` (nullable JSON object) — the per-character delta-mode
    /// checkpoints, advanced by the Courier paste resolver
    /// ([`crate::services::courier_transport`] `resolve_external_turn`, W4.4a4; v4
    /// `repos.chats.update(id, { courierCheckpoints })`). `Some(obj)` stores;
    /// `Some(Value::Null)` clears; `None` leaves unset. No `updatedAt` bump beyond
    /// the caller's explicit `updated_at`.
    pub courier_checkpoints: Option<Value>,
    /// v4 `repos.chats.update({ characterAvatars })` (the avatar job's
    /// per-character `{ imageId, generatedAt, afterMessageCount }` map). Stored as
    /// compact JSON text. No `updatedAt` bump (v4 `_update` preserves it).
    pub character_avatars: Option<Value>,
    /// v4 `repos.chats.update({ storyBackgroundImageId })` (the story-background
    /// job's newly-generated image id). No `updatedAt` bump beyond an explicit
    /// `updated_at`.
    pub story_background_image_id: Option<Option<String>>,
    /// v4 `repos.chats.update({ lastBackgroundGeneratedAt })` (the story-background
    /// job's minted generation timestamp). No `updatedAt` bump beyond an explicit
    /// `updated_at`.
    pub last_background_generated_at: Option<Option<String>>,
    // ------------------------------------------------------------------
    // Autonomous-room run-state / schedule / budget setters (U4.3, the
    // enclave lifecycle — v4 `autonomous-room.service.ts` +
    // `autonomous-run-start.ts` / `autonomous-room-announce.ts`). All ride
    // the same no-`updatedAt`-mint invariant (v4 `_update` preserves it;
    // none of the lifecycle writes pass one).
    // ------------------------------------------------------------------
    /// `runState` (enum TEXT: `idle|running|paused|stopped|budgetExhausted|error`).
    /// Always written with a value (v4 never clears it to NULL).
    pub run_state: Option<String>,
    /// Nullable `runStateMessage` — the run-status cause vocabulary
    /// (`manual:paused`, `manual:stopped`, `restart:interrupted`,
    /// `turn_failed:{reason}`, `start:enqueue_failed`, `schedule:enqueue_failed`,
    /// the `budget:*` family). `Some(Some(msg))` sets it; `Some(None)` clears to
    /// SQL NULL (v4 `runStateMessage: null` in `runStartPatch` / resume); `None`
    /// leaves it unset.
    pub run_state_message: Option<Option<String>>,
    /// `currentRunId` — always written with a value (a fresh UUID on start, and a
    /// bump on stop/reconcile so queued turns die on the stale-run guard).
    pub current_run_id: Option<String>,
    /// `runStartedAt` — stamped by `runStartPatch` (anchors both the wall-clock
    /// budget and the per-run token window). Never written NULL by v4.
    pub run_started_at: Option<String>,
    /// Nullable `runEndedAt`. `Some(Some(ts))` sets it (stop / the enqueue-failure
    /// rollback); `Some(None)` clears to SQL NULL (`runStartPatch`, resume, the
    /// resumable-pause reconcile patch); `None` leaves it unset.
    pub run_ended_at: Option<Option<String>>,
    /// Nullable `runPausedAt`. `Some(Some(ts))` stamps the pause instant;
    /// `Some(None)` clears it (fresh start / resume); `None` leaves it unset.
    pub run_paused_at: Option<Option<String>>,
    /// `runPausedAccumMs` (REAL) — the accumulated paused wall-clock the budget
    /// check subtracts. Zeroed by `runStartPatch`; accumulated on resume.
    pub run_paused_accum_ms: Option<f64>,
    /// `runTurnsConsumed` (REAL) — zeroed by `runStartPatch`; bumped by the turn
    /// handler (U4.4).
    pub run_turns_consumed: Option<f64>,
    /// `runTokensConsumed` (REAL) — zeroed by `runStartPatch`; maintained by the
    /// turn handler's token accounting (U4.4).
    pub run_tokens_consumed: Option<f64>,
    /// `runMilestonesAnnounced` (REAL bitmask: 1=halfway, 2=near-end, 4=grace) —
    /// zeroed by `runStartPatch`; set by the milestone pass (U4.4).
    pub run_milestones_announced: Option<f64>,
    /// Nullable `scheduleCron`. `Some(Some(expr))` sets it; `Some(None)` clears
    /// (drops the room to manual-only); `None` leaves it unset.
    pub schedule_cron: Option<Option<String>>,
    /// Nullable `scheduleFreshnessWindowMs` (REAL). `Some(None)` clears back to
    /// the user-default window.
    pub schedule_freshness_window_ms: Option<Option<f64>>,
    /// Nullable `scheduleNextRunAt`. `Some(Some(ts))` advances the slot;
    /// `Some(None)` clears it (cron removed / failed to advance); `None` leaves it
    /// unset (a manual start that didn't consume a cron slot).
    pub schedule_next_run_at: Option<Option<String>>,
    /// Nullable `scheduleLastRunAt`. `Some(Some(ts))` stamps the run-start time;
    /// `Some(None)` writes NULL (v4's tri-state passes the value through); `None`
    /// leaves it unset.
    pub schedule_last_run_at: Option<Option<String>>,
    /// Nullable `budgetMaxTurns` (REAL int). `Some(None)` clears the cap.
    pub budget_max_turns: Option<Option<f64>>,
    /// Nullable `budgetMaxTokens` (REAL int). `Some(None)` clears the cap.
    pub budget_max_tokens: Option<Option<f64>>,
    /// Nullable `budgetMaxWallClockMs` (REAL int). `Some(None)` clears the cap.
    pub budget_max_wall_clock_ms: Option<Option<f64>>,
    /// Nullable `budgetEstimatedSpendCapUSD` (REAL). `Some(None)` clears the cap.
    pub budget_estimated_spend_cap_usd: Option<Option<f64>>,
    /// `budgetExcludeCacheHits` — v4 writes the NUMBER `0`/`1` (not a bool) from
    /// the settings editor (`=== false ? 0 : 1`).
    pub budget_exclude_cache_hits: Option<f64>,
    /// Nullable `runVisibility` (enum TEXT: `owner_only|household|open`).
    /// `Some(None)` clears back to "inherit the user default".
    pub run_visibility: Option<Option<String>>,
    /// `runDestructiveToolsAllowed` — v4 writes the NUMBER `0`/`1` (clamped by the
    /// user-level `destructiveToolPolicy` ceiling at write time).
    pub run_destructive_tools_allowed: Option<f64>,
    /// Nullable `turnSkippingEnabled` (INTEGER 0/1) — the "nothing to add"
    /// per-chat toggle (v4 b90cd1f5). `Some(Some(b))` sets it; `Some(None)`
    /// clears to SQL NULL (= enabled, the default); `None` leaves it unset.
    pub turn_skipping_enabled: Option<Option<bool>>,
    pub updated_at: Option<String>,
}

/// Repository over a borrowed MAIN-db connection (held by the [`super::Writer`]).
pub struct ChatsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ChatsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// `create` — insert one chat row with pinned id + timestamps. JSON columns
    /// (`participants`, the array columns, `state`, the nullable objects) are
    /// pre-serialized to text; numbers bind `f64`; booleans bind 0/1; unset
    /// nullables bind SQL NULL.
    pub fn create(&self, data: &ChatCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let participants_json = json_text(&data.participants)?;
        let tags_json = json_text(&data.tags)?;
        let summary_anchor_json = json_text(&data.summary_anchor_message_ids)?;
        let impersonating_json = json_text(&data.impersonating_participant_ids)?;
        let disabled_tools_json = json_text(&data.disabled_tools)?;
        let disabled_tool_groups_json = json_text(&data.disabled_tool_groups)?;
        let danger_categories_json = json_text(&data.danger_categories)?;
        let state_json = json_text(&data.state)?;
        let silly_json = opt_json_text(&data.silly_tavern_metadata)?;
        let timestamp_config_json = opt_json_text(&data.timestamp_config)?;
        let pending_outfit_json = opt_json_text(&data.pending_outfit_notifications)?;
        let compression_cache_json = opt_json_text(&data.compression_cache)?;
        let scene_state_json = opt_json_text(&data.scene_state)?;
        let equipped_outfit_json = opt_json_text(&data.equipped_outfit)?;
        let character_avatars_json = opt_json_text(&data.character_avatars)?;
        let compiled_identity_json = opt_json_text(&data.compiled_identity_stacks)?;
        let courier_json = opt_json_text(&data.courier_checkpoints)?;
        let commonplace_scene_json = opt_json_text(&data.commonplace_scene_cache)?;
        let commonplace_recall_json = opt_json_text(&data.commonplace_recall_history)?;

        self.conn.execute(
            "INSERT INTO chats (\
               id, userId, participants, title, contextSummary, sillyTavernMetadata, tags, \
               roleplayTemplateId, timestampConfig, lastTurnParticipantId, messageCount, \
               lastMessageAt, lastRenameCheckInterchange, compactionGeneration, lastSummaryTurn, \
               lastSummaryTokens, lastFullRebuildTurn, summaryAnchorMessageIds, isPaused, \
               isManuallyRenamed, impersonatingParticipantIds, activeTypingParticipantId, \
               allLLMPauseTurnCount, turnQueue, spokenThisCycleParticipantIds, documentEditingMode, \
               documentMode, dividerPosition, terminalMode, activeTerminalSessionId, \
               rightPaneVerticalSplit, projectId, scenarioText, totalPromptTokens, \
               totalCompletionTokens, estimatedCostUSD, priceSource, showSystemEventsOverride, \
               requestFullContextOnNextMessage, disabledTools, disabledToolGroups, \
               forceToolsOnNextMessage, allowCrossCharacterVaultReads, pendingOutfitNotifications, \
               state, compressionCache, agentModeEnabled, agentTurnCount, storyBackgroundImageId, \
               lastBackgroundGeneratedAt, imageProfileId, alertCharactersOfLanternImages, \
               isDangerousChat, dangerScore, dangerCategories, dangerClassifiedAt, \
               dangerClassifiedAtMessageCount, conciergeOverride, sceneState, renderedMarkdown, \
               equippedOutfit, characterAvatars, avatarGenerationEnabled, chatType, helpPageUrl, \
               consoleConnectionProfileId, compiledIdentityStacks, courierCheckpoints, \
               commonplaceSceneCache, commonplaceRecallHistory, timelineMode, budgetMaxTurns, \
               budgetMaxTokens, \
               budgetMaxWallClockMs, budgetEstimatedSpendCapUSD, scheduleCron, \
               scheduleFreshnessWindowMs, scheduleNextRunAt, scheduleLastRunAt, runState, \
               currentRunId, runStateMessage, runStartedAt, runEndedAt, runPausedAt, \
               runPausedAccumMs, runTurnsConsumed, runTokensConsumed, runMilestonesAnnounced, \
               runDestructiveToolsAllowed, budgetExcludeCacheHits, runVisibility, coreWhisperEnabled, \
               coreWhisperInterval, showThinking, createdAt, updatedAt, answerConfirmationOverride, \
               turnSkippingEnabled) \
             VALUES (\
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, \
               ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, \
               ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44, ?45, ?46, ?47, ?48, ?49, ?50, \
               ?51, ?52, ?53, ?54, ?55, ?56, ?57, ?58, ?59, ?60, ?61, ?62, ?63, ?64, ?65, ?66, \
               ?67, ?68, ?69, ?70, ?71, ?72, ?73, ?74, ?75, ?76, ?77, ?78, ?79, ?80, ?81, ?82, \
               ?83, ?84, ?85, ?86, ?87, ?88, ?89, ?90, ?91, ?92, ?93, ?94, ?95, ?96, ?97, ?98, \
               ?99)",
            params![
                opts.id,
                data.user_id,
                participants_json,
                data.title,
                data.context_summary,
                silly_json,
                tags_json,
                data.roleplay_template_id,
                timestamp_config_json,
                data.last_turn_participant_id,
                data.message_count,
                data.last_message_at,
                data.last_rename_check_interchange,
                data.compaction_generation,
                data.last_summary_turn,
                data.last_summary_tokens,
                data.last_full_rebuild_turn,
                summary_anchor_json,
                data.is_paused,
                data.is_manually_renamed,
                impersonating_json,
                data.active_typing_participant_id,
                data.all_llm_pause_turn_count,
                data.turn_queue,
                data.spoken_this_cycle_participant_ids,
                data.document_editing_mode,
                data.document_mode,
                data.divider_position,
                data.terminal_mode,
                data.active_terminal_session_id,
                data.right_pane_vertical_split,
                data.project_id,
                data.scenario_text,
                data.total_prompt_tokens,
                data.total_completion_tokens,
                data.estimated_cost_usd,
                data.price_source,
                data.show_system_events_override,
                data.request_full_context_on_next_message,
                disabled_tools_json,
                disabled_tool_groups_json,
                data.force_tools_on_next_message,
                data.allow_cross_character_vault_reads,
                pending_outfit_json,
                state_json,
                compression_cache_json,
                data.agent_mode_enabled,
                data.agent_turn_count,
                data.story_background_image_id,
                data.last_background_generated_at,
                data.image_profile_id,
                data.alert_characters_of_lantern_images,
                data.is_dangerous_chat,
                data.danger_score,
                danger_categories_json,
                data.danger_classified_at,
                data.danger_classified_at_message_count,
                data.concierge_override,
                scene_state_json,
                data.rendered_markdown,
                equipped_outfit_json,
                character_avatars_json,
                data.avatar_generation_enabled,
                data.chat_type,
                data.help_page_url,
                data.console_connection_profile_id,
                compiled_identity_json,
                courier_json,
                commonplace_scene_json,
                commonplace_recall_json,
                data.timeline_mode,
                data.budget_max_turns,
                data.budget_max_tokens,
                data.budget_max_wall_clock_ms,
                data.budget_estimated_spend_cap_usd,
                data.schedule_cron,
                data.schedule_freshness_window_ms,
                data.schedule_next_run_at,
                data.schedule_last_run_at,
                data.run_state,
                data.current_run_id,
                data.run_state_message,
                data.run_started_at,
                data.run_ended_at,
                data.run_paused_at,
                data.run_paused_accum_ms,
                data.run_turns_consumed,
                data.run_tokens_consumed,
                data.run_milestones_announced,
                data.run_destructive_tools_allowed,
                data.budget_exclude_cache_hits,
                data.run_visibility,
                data.core_whisper_enabled,
                data.core_whisper_interval,
                data.show_thinking,
                opts.created_at,
                opts.updated_at,
                data.answer_confirmation_override,
                data.turn_skipping_enabled,
            ],
        )?;
        Ok(())
    }

    /// `update` — apply the patch to chat `id`. Returns `Ok(false)` when no row
    /// matched. `updatedAt` is **never minted**: `Some(updated_at)` sets it,
    /// `None` preserves the existing value (v4's override). Each other `Some`
    /// field sets its column.
    pub fn update(&self, id: &str, patch: &ChatUpdate) -> Result<bool, DbError> {
        // Resolve updatedAt (explicit override, else preserve existing). A
        // missing row makes the update a no-op (v4 `_update` → null).
        let resolved_updated_at = match &patch.updated_at {
            Some(v) => {
                if !self.row_exists(id)? {
                    return Ok(false);
                }
                v.clone()
            }
            None => match self.existing_updated_at(id)? {
                Some(u) => u,
                None => return Ok(false),
            },
        };

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();
        macro_rules! set_col {
            ($col:literal, $boxed:expr) => {{
                assignments.push(format!("{} = ?{}", $col, values.len() + 1));
                values.push($boxed);
            }};
        }

        if let Some(v) = &patch.title {
            set_col!("title", Box::new(v.clone()));
        }
        if let Some(v) = &patch.context_summary {
            set_col!("contextSummary", Box::new(v.clone()));
        }
        if let Some(v) = patch.is_paused {
            set_col!("isPaused", Box::new(v));
        }
        if let Some(v) = patch.is_manually_renamed {
            set_col!("isManuallyRenamed", Box::new(v));
        }
        if let Some(v) = patch.message_count {
            set_col!("messageCount", Box::new(v));
        }
        if let Some(v) = &patch.danger_score {
            set_col!("dangerScore", Box::new(*v));
        }
        if let Some(v) = &patch.chat_type {
            set_col!("chatType", Box::new(v.clone()));
        }
        if let Some(v) = &patch.state {
            set_col!("state", Box::new(json_text(v)?));
        }
        if let Some(v) = &patch.tags {
            set_col!("tags", Box::new(json_text(v)?));
        }
        if let Some(v) = &patch.participants {
            set_col!("participants", Box::new(json_text(v)?));
        }
        if let Some(v) = &patch.impersonating_participant_ids {
            set_col!("impersonatingParticipantIds", Box::new(json_text(v)?));
        }
        if let Some(v) = &patch.active_typing_participant_id {
            set_col!("activeTypingParticipantId", Box::new(v.clone()));
        }
        if let Some(v) = &patch.last_message_at {
            set_col!("lastMessageAt", Box::new(v.clone()));
        }
        if let Some(v) = &patch.spoken_this_cycle_participant_ids {
            set_col!("spokenThisCycleParticipantIds", Box::new(v.clone()));
        }
        if let Some(v) = &patch.turn_queue {
            set_col!("turnQueue", Box::new(v.clone()));
        }
        if let Some(v) = &patch.last_turn_participant_id {
            set_col!("lastTurnParticipantId", Box::new(v.clone()));
        }
        if let Some(v) = patch.all_llm_pause_turn_count {
            set_col!("allLLMPauseTurnCount", Box::new(v));
        }
        if let Some(v) = patch.total_prompt_tokens {
            set_col!("totalPromptTokens", Box::new(v));
        }
        if let Some(v) = patch.total_completion_tokens {
            set_col!("totalCompletionTokens", Box::new(v));
        }
        if let Some(v) = &patch.estimated_cost_usd {
            set_col!("estimatedCostUSD", Box::new(*v));
        }
        if let Some(v) = &patch.equipped_outfit {
            set_col!("equippedOutfit", Box::new(opt_json_text(v)?));
        }
        if let Some(v) = patch.compaction_generation {
            set_col!("compactionGeneration", Box::new(v));
        }
        if let Some(v) = patch.last_summary_turn {
            set_col!("lastSummaryTurn", Box::new(v));
        }
        if let Some(v) = patch.last_summary_tokens {
            set_col!("lastSummaryTokens", Box::new(v));
        }
        if let Some(v) = patch.last_full_rebuild_turn {
            set_col!("lastFullRebuildTurn", Box::new(v));
        }
        if let Some(v) = &patch.summary_anchor_message_ids {
            set_col!("summaryAnchorMessageIds", Box::new(json_text(v)?));
        }
        if let Some(v) = patch.last_rename_check_interchange {
            set_col!("lastRenameCheckInterchange", Box::new(v));
        }
        if let Some(v) = &patch.roleplay_template_id {
            set_col!("roleplayTemplateId", Box::new(v.clone()));
        }
        if let Some(v) = &patch.project_id {
            set_col!("projectId", Box::new(v.clone()));
        }
        if let Some(v) = patch.agent_turn_count {
            set_col!("agentTurnCount", Box::new(v));
        }
        if let Some(v) = &patch.document_mode {
            set_col!("documentMode", Box::new(v.clone()));
        }
        if let Some(v) = &patch.compiled_identity_stacks {
            // The identity-stack cache (v4 `writeStacks` via
            // `repos.chats.update({ compiledIdentityStacks })`). A JSON `null` clears
            // the column (SQL NULL, v4's `keys > 0 ? stacks : null`); an object is
            // stored as compact JSON text. No `updatedAt` bump.
            let text: Option<String> = if v.is_null() {
                None
            } else {
                Some(json_text(v)?)
            };
            set_col!("compiledIdentityStacks", Box::new(text));
        }
        if let Some(v) = &patch.compression_cache {
            // The async compression cache (v4 `persistToDatabase`/`clearFromDatabase`
            // via `repos.chats.update({ compressionCache })`). A JSON `null` clears
            // the column (SQL NULL); an object is stored as compact JSON text. Does
            // not bump `updatedAt` (v4's `_update` preserves it — the `resolved_updated_at`
            // path below keeps the existing value when `updated_at` is unset).
            let text: Option<String> = if v.is_null() {
                None
            } else {
                Some(json_text(v)?)
            };
            set_col!("compressionCache", Box::new(text));
        }
        if let Some(v) = &patch.scene_state {
            // v4 `repos.chats.update({ sceneState })` — a JSON object stored as
            // compact text; a JSON `null` clears the column. No `updatedAt` bump.
            let text: Option<String> = if v.is_null() {
                None
            } else {
                Some(json_text(v)?)
            };
            set_col!("sceneState", Box::new(text));
        }
        if let Some(v) = &patch.commonplace_scene_cache {
            // v4 `repos.chats.update({ commonplaceSceneCache })` (W4.6a scene cache
            // persist) — JSON object as text; `null` clears. No `updatedAt` bump.
            let text: Option<String> = if v.is_null() {
                None
            } else {
                Some(json_text(v)?)
            };
            set_col!("commonplaceSceneCache", Box::new(text));
        }
        if let Some(v) = &patch.commonplace_recall_history {
            // v4 `repos.chats.update({ commonplaceRecallHistory })` (W4.6a recall
            // history persist) — JSON array as text; `null` clears. No `updatedAt`
            // bump.
            let text: Option<String> = if v.is_null() {
                None
            } else {
                Some(json_text(v)?)
            };
            set_col!("commonplaceRecallHistory", Box::new(text));
        }
        if let Some(v) = &patch.timeline_mode {
            set_col!("timelineMode", Box::new(v.clone()));
        }
        if let Some(v) = &patch.courier_checkpoints {
            // v4 `repos.chats.update({ courierCheckpoints })` (W4.4a4 Courier paste
            // resolver) — a JSON object stored as compact text; a JSON `null` clears
            // the column. `updatedAt` IS bumped here (the resolver passes an explicit
            // `updated_at`).
            let text: Option<String> = if v.is_null() {
                None
            } else {
                Some(json_text(v)?)
            };
            set_col!("courierCheckpoints", Box::new(text));
        }
        if let Some(v) = &patch.character_avatars {
            // v4 `repos.chats.update({ characterAvatars })` (the avatar job) — a
            // JSON object stored as compact text; a JSON `null` clears the column.
            // No `updatedAt` bump.
            let text: Option<String> = if v.is_null() {
                None
            } else {
                Some(json_text(v)?)
            };
            set_col!("characterAvatars", Box::new(text));
        }
        if let Some(v) = &patch.story_background_image_id {
            // v4 `repos.chats.update({ storyBackgroundImageId })` (the story job).
            set_col!("storyBackgroundImageId", Box::new(v.clone()));
        }
        if let Some(v) = &patch.last_background_generated_at {
            // v4 `repos.chats.update({ lastBackgroundGeneratedAt })` (the story job).
            set_col!("lastBackgroundGeneratedAt", Box::new(v.clone()));
        }
        // Autonomous-room run-state / schedule / budget columns (U4.3).
        if let Some(v) = &patch.run_state {
            set_col!("runState", Box::new(v.clone()));
        }
        if let Some(v) = &patch.run_state_message {
            set_col!("runStateMessage", Box::new(v.clone()));
        }
        if let Some(v) = &patch.current_run_id {
            set_col!("currentRunId", Box::new(v.clone()));
        }
        if let Some(v) = &patch.run_started_at {
            set_col!("runStartedAt", Box::new(v.clone()));
        }
        if let Some(v) = &patch.run_ended_at {
            set_col!("runEndedAt", Box::new(v.clone()));
        }
        if let Some(v) = &patch.run_paused_at {
            set_col!("runPausedAt", Box::new(v.clone()));
        }
        if let Some(v) = patch.run_paused_accum_ms {
            set_col!("runPausedAccumMs", Box::new(v));
        }
        if let Some(v) = patch.run_turns_consumed {
            set_col!("runTurnsConsumed", Box::new(v));
        }
        if let Some(v) = patch.run_tokens_consumed {
            set_col!("runTokensConsumed", Box::new(v));
        }
        if let Some(v) = patch.run_milestones_announced {
            set_col!("runMilestonesAnnounced", Box::new(v));
        }
        if let Some(v) = &patch.schedule_cron {
            set_col!("scheduleCron", Box::new(v.clone()));
        }
        if let Some(v) = &patch.schedule_freshness_window_ms {
            set_col!("scheduleFreshnessWindowMs", Box::new(*v));
        }
        if let Some(v) = &patch.schedule_next_run_at {
            set_col!("scheduleNextRunAt", Box::new(v.clone()));
        }
        if let Some(v) = &patch.schedule_last_run_at {
            set_col!("scheduleLastRunAt", Box::new(v.clone()));
        }
        if let Some(v) = &patch.budget_max_turns {
            set_col!("budgetMaxTurns", Box::new(*v));
        }
        if let Some(v) = &patch.budget_max_tokens {
            set_col!("budgetMaxTokens", Box::new(*v));
        }
        if let Some(v) = &patch.budget_max_wall_clock_ms {
            set_col!("budgetMaxWallClockMs", Box::new(*v));
        }
        if let Some(v) = &patch.budget_estimated_spend_cap_usd {
            set_col!("budgetEstimatedSpendCapUSD", Box::new(*v));
        }
        if let Some(v) = patch.budget_exclude_cache_hits {
            set_col!("budgetExcludeCacheHits", Box::new(v));
        }
        if let Some(v) = &patch.run_visibility {
            set_col!("runVisibility", Box::new(v.clone()));
        }
        if let Some(v) = patch.run_destructive_tools_allowed {
            set_col!("runDestructiveToolsAllowed", Box::new(v));
        }
        if let Some(v) = patch.turn_skipping_enabled {
            set_col!("turnSkippingEnabled", Box::new(v));
        }
        set_col!("updatedAt", Box::new(resolved_updated_at));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));
        let sql = format!(
            "UPDATE chats SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );
        let refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let n = self.conn.execute(&sql, refs.as_slice())?;
        Ok(n > 0)
    }

    /// `delete` — drop the slim `chats` row + its `chat_messages` rows (v4 deletes
    /// both). Returns `Ok(false)` when no chat row matched. The participant-vault
    /// summary sweep is deferred (external subsystem; see module docs).
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let n = self
            .conn
            .execute("DELETE FROM chats WHERE id = ?1", params![id])?;
        if n == 0 {
            return Ok(false);
        }
        self.conn
            .execute("DELETE FROM chat_messages WHERE chatId = ?1", params![id])?;
        Ok(true)
    }

    /// The row's current `updatedAt`, or `None` if no such chat.
    fn existing_updated_at(&self, id: &str) -> Result<Option<String>, DbError> {
        self.conn
            .query_row(
                "SELECT updatedAt FROM chats WHERE id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(DbError::from)
    }

    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row("SELECT 1 FROM chats WHERE id = ?1", params![id], |r| {
                r.get::<_, i64>(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }
}

/// Serialize a value to compact JSON text (for a JSON column).
fn json_text<T: Serialize>(v: &T) -> Result<String, DbError> {
    serde_json::to_string(v).map_err(|e| DbError::Key(format!("json serialize: {e}")))
}

/// Serialize an optional JSON object column: `None` → SQL NULL.
fn opt_json_text(v: &Option<Value>) -> Result<Option<String>, DbError> {
    match v {
        Some(val) => Ok(Some(json_text(val)?)),
        None => Ok(None),
    }
}
