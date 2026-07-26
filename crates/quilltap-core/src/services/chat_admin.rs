//! The chat-administration dispatch handlers (P4.9E3A) — a differential port of
//! v4's chat-admin actions, each composed over already-ported pieces:
//!
//!   - `chatAddTag` / `chatRemoveTag` — v4 `actions/tags.ts`
//!     (`handleAddTag` :16, `handleRemoveTag` :39) over
//!     `TaggableBaseRepository.addTag`/`removeTag`'s exact semantics.
//!   - `chatUpdateToolSettings` — v4 `actions/tools.ts`
//!     (`handleUpdateToolSettings` :71).
//!   - `chatToggleAgentMode` — v4 `actions/agent-mode.ts`
//!     (`handleToggleAgentMode` :28) over
//!     [`resolve_agent_mode_setting`](crate::services::agent_mode).
//!   - `chatReclassifyDanger` — v4 `actions/danger-classification.ts`
//!     (`handleReclassifyDanger` :17) over
//!     [`enqueue_chat_danger_classification`].
//!   - `chatRenderConversation` — v4 `actions/render-conversation.ts`
//!     (`handleRenderConversation` :16) over [`enqueue_conversation_render`].
//!
//! ## Quirks reproduced deliberately
//!
//! - **`add-tag` verifies the tag but `remove-tag` does not.** v4's remove
//!   handler never reads the tag row (`tags.ts:44-47`), so removing a
//!   nonexistent tag id succeeds as a no-op. The asymmetry is v4's.
//! - **Both tag writes are conditional.** `TaggableBaseRepository.addTag` only
//!   calls `update` when the id is absent; `removeTag` only when the filter
//!   actually shortened the array. A no-op therefore does not touch `updatedAt`
//!   — which the differential's table dump sees.
//! - **`update-tool-settings` writes a third column the client never sends.**
//!   Alongside the two arrays it sets `forceToolsOnNextMessage = true`
//!   (`tools.ts:84`) so the next message announces the tool change, while the
//!   response echoes only the two arrays. Both halves are load-bearing.
//! - **`reclassify-danger` clears five columns and only THEN looks for a
//!   profile.** When no LLM participant carries a `connectionProfileId` the
//!   reset still happened; the response says so and carries no `jobId`.
//! - **The two enqueueing verbs dedupe on the chat**, so a second call returns
//!   the first job's id (`render-conversation` reports that as `isNew: false`).
//!
//! ## The §1 `toggle-agent-mode` narrowing (recorded, not silent)
//!
//! v4's `toggleAgentModeSchema` is `{ enabled: boolean | null | undefined }` —
//! three settable states plus "absent". The round's frozen §1 wire carries no
//! `enabled` field, so the dispatch verb can only express v4's **absent** arm
//! (which v4's `_update` skips, leaving the column alone). The service function
//! below takes the full tri-state and the differential covers all four arms, so
//! the port is proven for the day the wire grows the field; widening
//! `Request::ChatToggleAgentMode` is a cross-lane escalation, recorded in the
//! lane record.
//!
//! Pinned by `chat_admin_routes_equivalence` (tier 2) and
//! `chat_regenerate_title_tier3_equivalence` (tier 3, `regenerate-title`).

use serde_json::{json, Value};

use crate::api::types::{ErrorKind, Response};
use crate::cheap_llm::{get_cheap_llm_provider, CheapLlmProfile};
use crate::db::chats::ChatUpdate;
use crate::db::chats_messages::ChatEventInput;
use crate::db::connection_profiles;
use crate::db::runtime::Db;
use crate::db::{
    chat_settings, chats_messages_read, chats_read, memories_read, projects, tags, DbError,
};
use crate::services::agent_mode::{
    resolve_agent_mode_setting, AgentModeSettings, DEFAULT_AGENT_MODE_SETTINGS,
};
use crate::services::context_summary::tasks::{title_chat, title_help_chat};
use crate::services::image_job_common::{
    cheap_llm_config_from_settings, cheap_llm_profile_from_value,
};
use crate::services::memory_service::delete_memory_with_vector;
use crate::services::queue_service::{
    enqueue_chat_danger_classification, enqueue_conversation_render,
};

// ===========================================================================
// Response helpers (v4 `lib/api/responses.ts` semantics)
// ===========================================================================

pub(crate) fn ok(body: Value) -> Response {
    Response::ChatAdmin(body)
}
pub(crate) fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
/// The shape v4's middleware turns a thrown `ZodError` into. v5's error envelope
/// carries no `details` array — the standing, named P4.6bb deferral, asserted in
/// both directions by `chat_admin_routes_equivalence`.
pub(crate) fn validation_error() -> Response {
    Response::error(ErrorKind::BadRequest, "Validation error")
}
/// v4 `notFound(resource)` → `` `${resource} not found` `` at 404.
pub(crate) fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn server_error(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Internal, msg)
}
pub(crate) fn internal(e: DbError) -> Response {
    Response::error(ErrorKind::Internal, format!("{e}"))
}

/// The chat's `tags` array as `Vec<String>` (absent / non-array → empty).
fn tag_ids(chat: &Value) -> Vec<String> {
    chat.get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// v4's route-level ownership gate: every chat action runs behind
/// `repos.chats.findById(chatId)` → `notFound('Chat')` (`handlers/post.ts:115`).
pub(crate) fn load_chat(db: &Db, chat_id: &str) -> Result<Option<Value>, DbError> {
    let cid = chat_id.to_string();
    db.read_main(move |c| chats_read::find_by_id(c, &cid))
}

// ===========================================================================
// add-tag / remove-tag (v4 `actions/tags.ts`)
// ===========================================================================

/// v4 `?action=add-tag` — verify the tag row exists, then push its id onto the
/// chat's `tags` array (only writing when it was absent). `{ success: true, tag }`
/// at **201**; the dispatch boundary answers 200 (the standing `ChatCreate`
/// precedent) and the differential asserts the status difference in both
/// directions.
pub async fn chat_add_tag(db: &Db, chat_id: &str, tag_id: &str) -> Response {
    let chat = match load_chat(db, chat_id) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let tid = tag_id.to_string();
    let tag = match db.read_main(move |c| tags::find_full_by_id(c, &tid)) {
        Ok(Some(t)) => t,
        Ok(None) => return not_found("Tag"),
        Err(e) => return internal(e),
    };

    // v4 `TaggableBaseRepository.addTag`: push + update ONLY when absent.
    let mut current = tag_ids(&chat);
    if !current.iter().any(|t| t == tag_id) {
        current.push(tag_id.to_string());
        let cid = chat_id.to_string();
        let patch = ChatUpdate {
            tags: Some(current),
            ..Default::default()
        };
        if let Err(e) = db
            .write(move |w| w.main().chats().update(&cid, &patch).map(|_| ()))
            .await
        {
            return internal(e);
        }
    }
    ok(json!({ "success": true, "tag": tag }))
}

/// v4 `?action=remove-tag` — filter the id out of the chat's `tags` array,
/// writing only when the array actually shortened. `{ success: true }`.
///
/// v4 does NOT verify the tag exists on this path (`tags.ts:44-47`), so an
/// unknown id is a successful no-op.
pub async fn chat_remove_tag(db: &Db, chat_id: &str, tag_id: &str) -> Response {
    let chat = match load_chat(db, chat_id) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let current = tag_ids(&chat);
    let filtered: Vec<String> = current.iter().filter(|t| *t != tag_id).cloned().collect();
    if filtered.len() != current.len() {
        let cid = chat_id.to_string();
        let patch = ChatUpdate {
            tags: Some(filtered),
            ..Default::default()
        };
        if let Err(e) = db
            .write(move |w| w.main().chats().update(&cid, &patch).map(|_| ()))
            .await
        {
            return internal(e);
        }
    }
    ok(json!({ "success": true }))
}

// ===========================================================================
// update-tool-settings (v4 `actions/tools.ts:71`)
// ===========================================================================

/// v4 `?action=update-tool-settings` — replace the chat's two disabled-tool sets
/// AND set `forceToolsOnNextMessage = true` (so the next message announces the
/// change). `successResponse({ disabledTools, disabledToolGroups })` echoes only
/// the client-sent arrays — v4's `successResponse(data)` is `NextResponse.json(
/// data)`, so the body has no `success` wrapper.
pub async fn chat_update_tool_settings(
    db: &Db,
    chat_id: &str,
    disabled_tools: Vec<String>,
    disabled_tool_groups: Vec<String>,
) -> Response {
    if let Err(r) = require_chat(db, chat_id) {
        return r;
    }
    let cid = chat_id.to_string();
    let patch = ChatUpdate {
        disabled_tools: Some(disabled_tools.clone()),
        disabled_tool_groups: Some(disabled_tool_groups.clone()),
        force_tools_on_next_message: Some(true),
        ..Default::default()
    };
    if let Err(e) = db
        .write(move |w| w.main().chats().update(&cid, &patch).map(|_| ()))
        .await
    {
        return internal(e);
    }
    ok(json!({
        "disabledTools": disabled_tools,
        "disabledToolGroups": disabled_tool_groups,
    }))
}

// ===========================================================================
// toggle-agent-mode (v4 `actions/agent-mode.ts:28`)
// ===========================================================================

/// v4 `?action=toggle-agent-mode` — write the request's tri-state
/// `agentModeEnabled` through, then resolve the Global → Character → Project →
/// Chat cascade and return the effective state.
///
/// `enabled` mirrors v4's `z.boolean().nullable().optional()` exactly:
/// `Some(Some(b))` sets, `Some(None)` clears to null ("inherit"), `None` is
/// v4's absent key (the column is left alone). See the module header for why the
/// §1 wire can currently only reach the `None` arm.
///
/// v4 re-reads the chat AFTER the update (`agent-mode.ts:44`) and resolves the
/// cascade over the refreshed row — so `agentModeEnabled` in the body is the
/// STORED value, not the request's.
pub async fn chat_toggle_agent_mode(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    enabled: Option<Option<bool>>,
) -> Response {
    if let Err(r) = require_chat(db, chat_id) {
        return r;
    }
    let cid = chat_id.to_string();
    let patch = ChatUpdate {
        agent_mode_enabled: enabled,
        ..Default::default()
    };
    match db
        .write(move |w| w.main().chats().update(&cid, &patch))
        .await
    {
        // v4 `if (!updatedChat) return serverError('Failed to update chat')` —
        // `_update` returns null only when the row vanished mid-flight.
        Ok(false) => return server_error("Failed to update chat"),
        Ok(true) => {}
        Err(e) => return internal(e),
    }

    let updated = match load_chat(db, chat_id) {
        Ok(Some(c)) => c,
        // v4 reads through `repos.chats.update`'s own return value, which cannot
        // be null here; a vanished row lands on the same serverError.
        Ok(None) => return server_error("Failed to update chat"),
        Err(e) => return internal(e),
    };

    // v4 picks the FIRST CHARACTER participant carrying a characterId (no
    // active/removed filter) and swallows a failed character read.
    let character_default = first_character_id(&updated).and_then(|cid| {
        db.read_main(|main| {
            db.read_mount_index(|mount| crate::db::characters_read::find_by_id(main, mount, &cid))
        })
        .ok()
        .flatten()
        .and_then(|c| c.get("defaultAgentModeEnabled").and_then(Value::as_bool))
    });

    let project_default = updated
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .and_then(|pid| {
            db.read_main(|main| {
                db.read_mount_index(|mount| {
                    let repo = projects::ProjectsRepository::new(main, mount);
                    repo.find_by_id(pid)
                        .map_err(|e| DbError::Key(format!("project read failed: {e:?}")))
                })
            })
            .ok()
            .flatten()
            .and_then(|p| p.get("defaultAgentModeEnabled").and_then(Value::as_bool))
        });

    let uid = user_id.to_string();
    let settings = db
        .read_main(move |c| chat_settings::find_by_user_id(c, &uid))
        .ok()
        .flatten();
    let global = global_agent_mode_settings(settings.as_ref());

    let stored = updated.get("agentModeEnabled").and_then(Value::as_bool);
    let resolved = resolve_agent_mode_setting(stored, project_default, character_default, global);

    // v4's message branches on the REQUEST's tri-state, not the stored value.
    let message = match enabled {
        Some(None) => "Agent mode set to inherit",
        Some(Some(true)) => "Agent mode enabled",
        // `enabled === undefined` is falsy in v4's ternary chain, so an absent
        // key produces the "disabled" wording even though nothing changed.
        _ => "Agent mode disabled",
    };

    // v4 answers over `{...existing, ...data}` validated by Zod, NOT a re-read:
    // when the request carried the key, its value is echoed verbatim (including
    // `null`); when it did not, the EXISTING value shows through — and a NULL
    // column is dropped by `.nullable().optional()`, so the key is ABSENT from
    // the JSON. Both shapes are reproduced here (v5's read omits a NULL column
    // the same way).
    let mut body = serde_json::Map::new();
    match enabled {
        Some(Some(b)) => {
            body.insert("agentModeEnabled".into(), Value::Bool(b));
        }
        Some(None) => {
            body.insert("agentModeEnabled".into(), Value::Null);
        }
        None => {
            if let Some(v) = updated.get("agentModeEnabled") {
                body.insert("agentModeEnabled".into(), v.clone());
            }
        }
    }
    body.insert("resolvedAgentModeEnabled".into(), json!(resolved.enabled));
    body.insert(
        "agentModeSource".into(),
        json!(resolved.enabled_source.as_str()),
    );
    body.insert("message".into(), json!(message));
    ok(Value::Object(body))
}

/// v4 `updatedChat.participants.find(p => p.type === 'CHARACTER' && p.characterId)`.
fn first_character_id(chat: &Value) -> Option<String> {
    chat.get("participants")
        .and_then(Value::as_array)?
        .iter()
        .find(|p| {
            p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                && p.get("characterId").and_then(Value::as_str).is_some()
        })
        .and_then(|p| p.get("characterId").and_then(Value::as_str))
        .map(str::to_string)
}

/// v4 `globalSettings?.agentModeSettings ?? DEFAULT_AGENT_MODE_SETTINGS`. The
/// stored block is Zod-defaulted by v4's read, so a present block always carries
/// both keys; a missing key falls back per-field to the same defaults.
fn global_agent_mode_settings(settings: Option<&Value>) -> AgentModeSettings {
    let block = settings.and_then(|s| s.get("agentModeSettings"));
    match block {
        Some(b) if !b.is_null() => AgentModeSettings {
            max_turns: b
                .get("maxTurns")
                .and_then(Value::as_i64)
                .unwrap_or(DEFAULT_AGENT_MODE_SETTINGS.max_turns),
            default_enabled: b
                .get("defaultEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(DEFAULT_AGENT_MODE_SETTINGS.default_enabled),
        },
        _ => DEFAULT_AGENT_MODE_SETTINGS,
    }
}

// ===========================================================================
// reclassify-danger (v4 `actions/danger-classification.ts:17`)
// ===========================================================================

/// v4 `?action=reclassify-danger` — clear the five danger columns, then look for
/// an LLM participant with a connection profile and enqueue a fresh
/// classification. When none exists the reset still stands and the response says
/// so (with no `jobId`).
pub async fn chat_reclassify_danger(db: &Db, user_id: &str, chat_id: &str) -> Response {
    let chat = match load_chat(db, chat_id) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };

    let cid = chat_id.to_string();
    let patch = ChatUpdate {
        is_dangerous_chat: Some(None),
        danger_score: Some(None),
        danger_categories: Some(Vec::new()),
        danger_classified_at: Some(None),
        danger_classified_at_message_count: Some(None),
        ..Default::default()
    };
    if let Err(_e) = db
        .write(move |w| w.main().chats().update(&cid, &patch).map(|_| ()))
        .await
    {
        // v4 wraps the whole handler in try/catch → serverError.
        return server_error("Failed to reset danger classification");
    }

    // v4: the first CHARACTER participant that is NOT user-controlled and
    // carries a connectionProfileId.
    let profile = chat
        .get("participants")
        .and_then(Value::as_array)
        .and_then(|ps| {
            ps.iter()
                .find(|p| {
                    p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                        && p.get("controlledBy").and_then(Value::as_str) != Some("user")
                        && p.get("connectionProfileId")
                            .and_then(Value::as_str)
                            .is_some()
                })
                .and_then(|p| p.get("connectionProfileId").and_then(Value::as_str))
                .map(str::to_string)
        });

    match profile {
        Some(profile_id) => {
            match enqueue_chat_danger_classification(db, user_id, chat_id, &profile_id).await {
                Ok(job_id) => ok(json!({
                    "message": "Danger classification reset and re-queued",
                    "jobId": job_id,
                })),
                Err(_) => server_error("Failed to reset danger classification"),
            }
        }
        None => ok(json!({
            "message": "Danger classification reset (no active connection profile to re-queue)",
        })),
    }
}

// ===========================================================================
// render-conversation (v4 `actions/render-conversation.ts:16`)
// ===========================================================================

/// v4 `?action=render-conversation` — queue a Scriptorium conversation render
/// with `fullReembed: true`. Deduped on the chat, so a second call reports the
/// first job with `isNew: false`.
pub async fn chat_render_conversation(db: &Db, user_id: &str, chat_id: &str) -> Response {
    if let Err(r) = require_chat(db, chat_id) {
        return r;
    }
    match enqueue_conversation_render(db, user_id, chat_id, true).await {
        Ok((job_id, is_new)) => ok(json!({
            "message": "Conversation rendering queued",
            "jobId": job_id,
            "isNew": is_new,
        })),
        Err(_) => server_error("Failed to queue conversation rendering"),
    }
}

/// The route-level `notFound('Chat')` gate for handlers that don't otherwise
/// need the row.
pub(crate) fn require_chat(db: &Db, chat_id: &str) -> Result<(), Response> {
    match load_chat(db, chat_id) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(not_found("Chat")),
        Err(e) => Err(internal(e)),
    }
}

// ===========================================================================
// bulk-reattribute (v4 `actions/bulk.ts:18`)
// ===========================================================================

/// v4 `?action=bulk-reattribute` — move every matching message from one
/// participant to another, deleting the memories those messages produced.
///
/// `source_participant_id` is v4-`nullable`: an explicit `null` selects the
/// UNATTRIBUTED messages (`participantId` null or absent). `role_filter` is v4's
/// `z.enum(['ASSISTANT','USER','both']).prefault('both')` — an absent value
/// defaults to `both`, and an unrecognized one is a 400 the same way Zod's is.
///
/// ## The rewrite is a clear-and-replay, not an UPDATE
///
/// v4 rebuilds the whole transcript: `clearMessages` then `addMessage` for every
/// event in order (`bulk.ts:103-106`). That is not incidental — each `addMessage`
/// runs the chat-metadata side effect (recount `messageCount`, bump
/// `lastMessageAt`/`updatedAt` for message-typed events, fold
/// `spokenThisCycleParticipantIds`), so the final chat row is the product of N
/// sequential writes, not one. The port replays the same way, one event at a
/// time, so the metadata lands identically.
///
/// The trailing `repos.chats.update(chatId, {})` is a genuine no-op: v4's chats
/// repository PRESERVES `updatedAt` when the patch omits it, so despite its own
/// comment ("Update chat's updatedAt timestamp") that call changes nothing. It is
/// reproduced anyway — an empty patch still rewrites `updatedAt` to its existing
/// value.
///
/// ## Memory deletion is best-effort and counted
///
/// For every affected message, each memory whose `sourceMessageId` matches is
/// deleted through [`delete_memory_with_vector`] (which re-checks ownership
/// against the memory's own `characterId`, so the count is "actually deleted",
/// not "found"). A single failure is logged and skipped — one bad memory must not
/// abort the re-attribution.
pub async fn chat_bulk_reattribute(
    db: &Db,
    chat_id: &str,
    source_participant_id: Option<&str>,
    target_participant_id: &str,
    role_filter: Option<&str>,
) -> Response {
    let chat = match load_chat(db, chat_id) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };

    // v4's Zod `.enum([...]).prefault('both')`: absent → 'both', unknown → 400.
    let role_filter = role_filter.unwrap_or("both");
    if !matches!(role_filter, "ASSISTANT" | "USER" | "both") {
        return validation_error();
    }

    if source_participant_id == Some(target_participant_id) {
        return bad_request("Source and target participants must be different");
    }

    let participant_exists = |id: &str| -> bool {
        chat.get("participants")
            .and_then(Value::as_array)
            .is_some_and(|ps| {
                ps.iter()
                    .any(|p| p.get("id").and_then(Value::as_str) == Some(id))
            })
    };
    if let Some(src) = source_participant_id {
        if !participant_exists(src) {
            return bad_request("Source participant not found in chat");
        }
    }
    if !participant_exists(target_participant_id) {
        return bad_request("Target participant not found in chat");
    }

    let cid = chat_id.to_string();
    let all_messages = match db.read_main(move |c| chats_messages_read::get_messages(c, &cid)) {
        Ok(m) => m,
        Err(e) => return internal(e),
    };

    // v4's filter, predicate for predicate.
    let affected: Vec<&Value> = all_messages
        .iter()
        .filter(|msg| {
            if msg.get("type").and_then(Value::as_str) != Some("message") {
                return false;
            }
            let pid = msg.get("participantId").and_then(Value::as_str);
            match source_participant_id {
                // Explicit null selects the unattributed (null OR absent).
                None => {
                    if pid.is_some() {
                        return false;
                    }
                }
                Some(src) => {
                    if pid != Some(src) {
                        return false;
                    }
                }
            }
            if role_filter == "both" {
                return true;
            }
            msg.get("role").and_then(Value::as_str) == Some(role_filter)
        })
        .collect();

    if affected.is_empty() {
        return ok(json!({
            "success": true,
            "messagesUpdated": 0,
            "memoriesDeleted": 0,
        }));
    }

    let affected_ids: std::collections::HashSet<String> = affected
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();

    // Delete the memories those messages produced (best effort, counted).
    let mut memories_deleted = 0usize;
    for msg_id in affected
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str))
    {
        let mid = msg_id.to_string();
        let from_message = match db
            .read_main(move |c| memories_read::find_by_source_message_id(c, &mid))
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!(error = %e, "[Chats v1] Failed to read memories during bulk re-attribution");
                continue;
            }
        };
        for memory in from_message {
            let (Some(memory_id), Some(character_id)) = (
                memory.get("id").and_then(Value::as_str),
                memory.get("characterId").and_then(Value::as_str),
            ) else {
                continue;
            };
            match delete_memory_with_vector(db, character_id, memory_id).await {
                Ok(true) => memories_deleted += 1,
                Ok(false) => {}
                Err(e) => {
                    // v4 logs and continues — best-effort cleanup.
                    tracing::error!(
                        memory_id,
                        error = %e,
                        "[Chats v1] Failed to delete memory during bulk re-attribution"
                    );
                }
            }
        }
    }

    // Rewrite the whole transcript with the affected rows re-attributed.
    let mut rewritten: Vec<ChatEventInput> = Vec::with_capacity(all_messages.len());
    for msg in &all_messages {
        let mut event = msg.clone();
        if msg.get("type").and_then(Value::as_str) == Some("message") {
            if let Some(id) = msg.get("id").and_then(Value::as_str) {
                if affected_ids.contains(id) {
                    if let Some(o) = event.as_object_mut() {
                        o.insert(
                            "participantId".into(),
                            Value::String(target_participant_id.to_string()),
                        );
                    }
                }
            }
        }
        match serde_json::from_value::<ChatEventInput>(event) {
            Ok(e) => rewritten.push(e),
            Err(e) => return internal(DbError::Key(format!("bulk re-attribute marshal: {e}"))),
        }
    }

    let cid = chat_id.to_string();
    let write = db
        .write(move |w| {
            let msgs = w.main().chat_messages();
            msgs.clear_messages(&cid)?;
            // One `add_message` per event, exactly like v4 — the per-message
            // chat-metadata side effect is part of the observable result.
            for e in &rewritten {
                msgs.add_message(&cid, e)?;
            }
            // v4's trailing `repos.chats.update(chatId, {})` — a no-op that
            // rewrites `updatedAt` to its existing value.
            w.main().chats().update(&cid, &ChatUpdate::default())?;
            Ok(())
        })
        .await;
    if let Err(e) = write {
        return internal(e);
    }

    ok(json!({
        "success": true,
        "messagesUpdated": affected.len(),
        "memoriesDeleted": memories_deleted,
    }))
}

// ===========================================================================
// regenerate-title (v4 `actions/title.ts:18`) — the MANUAL entrance
// ===========================================================================

/// The host seam for one manual title regeneration: only the composing host
/// holds the completion provider + the LOGGING cheap executor the call rides
/// (the `announcement_preview` / `recall_replay` precedent). `Err(message)` is
/// the not-assembled refusal.
pub trait RegenerateTitleDriver: Send + Sync {
    fn run<'a>(
        &'a self,
        chat_id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send + 'a>>;
}

/// v4 `?action=regenerate-title` — generate a fresh title from the visible
/// transcript and store it, clearing `isManuallyRenamed`.
///
/// ## This is NOT the `TITLE_UPDATE` job
///
/// The order's tier-2 item 7 asked whether this and
/// [`title_update_job`](super::title_update_job) share one implementation. **They
/// do not — and neither do they in v4.** The job asks "does this need a new
/// title?" through `considerTitleUpdate` (a verdict + suggestion over
/// `CHAT_TITLE_CONSIDERATION_PROMPT`, gated on a checkpoint cursor); this manual
/// entrance asks "title this" outright through
/// [`title_chat`](super::context_summary::tasks::title_chat) (`CHAT_TITLE_PROMPT`,
/// a different transcript weighting and a different clamp) and always writes.
/// The port keeps both, matching v4.
///
/// ## Two details worth naming
///
/// - v4 passes `undefined` for `existingTitle`, so the "Current title / update
///   only if…" rider is NEVER appended from this entrance even though
///   `titleChat` supports it. Reproduced (`None`).
/// - The connection profile is the FIRST of the user's profiles unless the
///   chat's first CHARACTER participant carries one that still resolves —
///   `getCheapLLMProvider`'s own priority order then runs on top of that.
/// - Unlike the job, this path does NOT route dangerous chats to the uncensored
///   provider; v4's handler has no such step.
///
/// `completion`/`executor` are the cheap-LLM boundary (the tier-3 differential
/// injects the same canned reply both sides); `now_iso` is the stamped
/// `updatedAt` — this is one of the few chat writes that DOES bump it.
pub async fn chat_regenerate_title<C: crate::model::completion::CompletionProvider>(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    completion: &C,
    executor: &crate::services::cheap_llm_exec::CheapLlmTaskExecutor,
    now_iso: &str,
) -> Response {
    let chat = match load_chat(db, chat_id) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };

    let uid = user_id.to_string();
    let chat_settings = match db.read_main(move |c| chat_settings::find_by_user_id(c, &uid)) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let cheap_settings = chat_settings
        .as_ref()
        .and_then(|s| s.get("cheapLLMSettings"))
        .filter(|v| !v.is_null());
    if cheap_settings.is_none() {
        return bad_request("Cheap LLM settings not configured");
    }

    let uid = user_id.to_string();
    let profiles = match db.read_main(move |c| connection_profiles::find_by_user_id(c, &uid)) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    if profiles.is_empty() {
        return bad_request("No connection profiles available");
    }

    // v4: the first CHARACTER participant's profile when it resolves, else the
    // user's first profile.
    let participant_profile_id =
        chat.get("participants")
            .and_then(Value::as_array)
            .and_then(|ps| {
                ps.iter()
                    .find(|p| p.get("type").and_then(Value::as_str) == Some("CHARACTER"))
                    .and_then(|p| p.get("connectionProfileId").and_then(Value::as_str))
            });
    let connection_profile = participant_profile_id
        .and_then(|id| {
            profiles
                .iter()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(id))
        })
        .unwrap_or(&profiles[0]);

    let available: Vec<CheapLlmProfile> =
        profiles.iter().map(cheap_llm_profile_from_value).collect();
    // `ollama_available: false` + `registry_cheapest_for_current: None` follow the
    // job precedent; both feed only the priority-4/5 fallbacks. v4's
    // `if (!cheapLLM) return badRequest(…)` arm is DEAD for the same reason it is
    // dead in the job — priority 5 always yields the current profile — so the
    // port has no arm to carry and no way to exercise one on either side.
    let selection = get_cheap_llm_provider(
        &cheap_llm_profile_from_value(connection_profile),
        &cheap_llm_config_from_settings(cheap_settings),
        &available,
        false,
        None,
    );

    let cid = chat_id.to_string();
    let raw = match db.read_main(move |c| chats_messages_read::get_messages(c, &cid)) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let visible = crate::chat_tasks::extract_visible_conversation(&raw_messages(&raw));
    if visible.is_empty() {
        return bad_request("No messages in chat to generate title from");
    }
    // The two `ChatMessage` shapes are the same pair of fields; the title tasks
    // live in `context_summary`, whose type carries the optional `createdAt` the
    // fold task renders.
    let conversation: Vec<crate::services::context_summary::tasks::ChatMessage> = visible
        .into_iter()
        .map(|m| crate::services::context_summary::tasks::ChatMessage {
            role: m.role,
            content: m.content,
            created_at: None,
        })
        .collect();

    let is_help = crate::chat_predicates::is_help_like_chat_type(
        chat.get("chatType").and_then(Value::as_str),
    );
    // v4 passes `undefined` for `existingTitle` on BOTH arms.
    let result = if is_help {
        title_help_chat(
            executor,
            completion,
            &conversation,
            None,
            &selection,
            Some(chat_id),
        )
        .await
    } else {
        title_chat(
            executor,
            completion,
            &conversation,
            None,
            &selection,
            Some(chat_id),
        )
        .await
    };

    let new_title = match (result.success, result.result) {
        (true, Some(t)) if !t.is_empty() => t,
        // v4: `!result.success || !result.result` → serverError(result.error ||
        // 'Failed to generate title'). An empty string is falsy in JS, so it
        // lands here too.
        (_, _) => {
            return server_error(
                result
                    .error
                    .unwrap_or_else(|| "Failed to generate title".to_string()),
            )
        }
    };

    let cid = chat_id.to_string();
    let patch = ChatUpdate {
        title: Some(new_title.clone()),
        is_manually_renamed: Some(false),
        updated_at: Some(now_iso.to_string()),
        ..Default::default()
    };
    if let Err(_e) = db
        .write(move |w| w.main().chats().update(&cid, &patch).map(|_| ()))
        .await
    {
        // v4's outer try/catch.
        return server_error("Failed to regenerate title");
    }

    ok(json!({ "success": true, "title": new_title }))
}

/// The `RawMessage` view `extract_visible_conversation` consumes.
fn raw_messages(events: &[Value]) -> Vec<crate::chat_tasks::RawMessage> {
    events
        .iter()
        .map(|e| crate::chat_tasks::RawMessage {
            type_: e.get("type").and_then(Value::as_str).map(str::to_string),
            role: e.get("role").and_then(Value::as_str).map(str::to_string),
            content: e.get("content").and_then(Value::as_str).map(str::to_string),
        })
        .collect()
}
