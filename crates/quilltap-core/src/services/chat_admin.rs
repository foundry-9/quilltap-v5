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
use crate::services::agent_mode::{global_agent_mode_settings, resolve_agent_mode_setting};
use crate::services::auto_title::{apply_auto_title, AutoTitleExtraPatch, AutoTitleSource};
use crate::services::context_summary::tasks::{title_chat, title_help_chat};
use crate::services::image_job_common::{
    cheap_llm_config_from_settings, cheap_llm_profile_from_value,
};
use crate::services::memory_service::delete_memory_with_vector;
use crate::services::queue_service::{
    enqueue_chat_danger_classification, enqueue_context_summary, enqueue_conversation_render,
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
                        .map_err(|e| DbError::Internal(format!("project read failed: {e:?}")))
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
    match enqueue_conversation_render(db, user_id, chat_id, Some(true)).await {
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
/// runs the chat-metadata side effect (recount `messageCount`, bump `updatedAt`
/// for message-typed events and `lastMessageAt` for CHARACTER-AUTHORED ones —
/// v4 `735d9408c` — fold
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
            Err(e) => {
                return internal(DbError::Internal(format!("bulk re-attribute marshal: {e}")))
            }
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
/// transcript and store it through the auto-title chokepoint
/// ([`crate::services::auto_title`]), clearing `isManuallyRenamed`.
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

    // v4's ONE outer try/catch (`title.ts:92-94`) spans everything after the
    // chat lookup (which the dispatcher does before the handler): the settings
    // and profiles reads, `getMessages` and the chokepoint all land on the same
    // ERROR + 500 `Failed to regenerate title` — the error riding as the
    // logger's error argument, so it is v5's `error` field here. (The
    // `00c290c9a` unification's review: the lane had routed only the
    // chokepoint through it; the reads answered the raw `DbError` unlogged.)
    let regenerate_failed = |e: &dyn std::fmt::Display| {
        tracing::error!(
            chatId = chat_id,
            error = %e,
            "[Chats v1] Error regenerating title"
        );
        server_error("Failed to regenerate title")
    };

    let uid = user_id.to_string();
    let chat_settings = match db.read_main(move |c| chat_settings::find_by_user_id(c, &uid)) {
        Ok(v) => v,
        Err(e) => return regenerate_failed(&e),
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
        Err(e) => return regenerate_failed(&e),
    };
    if profiles.is_empty() {
        return bad_request("No connection profiles available");
    }

    let connection_profile = cast_or_first_profile(&chat, &profiles);

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
        Err(e) => return regenerate_failed(&e),
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
        // lands here too. v4 logs the task's `error` first (`undefined` when
        // absent — a JSON field that is simply missing, so no field here).
        (_, _) => {
            match result.error.as_deref() {
                Some(error) => tracing::error!(
                    chatId = chat_id,
                    error = error,
                    "[Chats v1] Title generation failed"
                ),
                None => tracing::error!(chatId = chat_id, "[Chats v1] Title generation failed"),
            }
            return server_error(
                result
                    .error
                    .unwrap_or_else(|| "Failed to generate title".to_string()),
            );
        }
    };

    // v4 `00c290c9a` (bug 163): through the auto-title chokepoint, so a
    // changed title also cues the Lantern. `clear_manual_rename` overrules a
    // hand rename and rides `isManuallyRenamed: false` along as the extra patch
    // — written even when the title comes back unchanged. The body is
    // `{success, title}` for EVERY outcome, `missing` included.
    let outcome = match apply_auto_title(
        db,
        user_id,
        chat_id,
        &new_title,
        chat_settings.as_ref(),
        AutoTitleExtraPatch::default(),
        true,
        AutoTitleSource::Regenerate,
        now_iso,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(e) => return regenerate_failed(&e),
    };

    tracing::info!(
        chatId = chat_id,
        newTitle = new_title.as_str(),
        outcome = outcome.as_str(),
        "[Chats v1] Title regenerated"
    );

    ok(json!({ "success": true, "title": new_title }))
}

/// v4's profile precedence for regenerate-title AND rebuild-summary: the first
/// CHARACTER participant's `connectionProfileId` when it names an available
/// profile, else the user's first profile. `profiles` must be non-empty (both
/// callers answer 400 first).
fn cast_or_first_profile<'a>(chat: &Value, profiles: &'a [Value]) -> &'a Value {
    let participant_profile_id =
        chat.get("participants")
            .and_then(Value::as_array)
            .and_then(|ps| {
                ps.iter()
                    .find(|p| p.get("type").and_then(Value::as_str) == Some("CHARACTER"))
                    .and_then(|p| p.get("connectionProfileId").and_then(Value::as_str))
            });
    participant_profile_id
        .and_then(|id| {
            profiles
                .iter()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(id))
        })
        .unwrap_or(&profiles[0])
}

// ===========================================================================
// rebuild-summary (v4 `actions/rebuild-summary.ts`, `e7821606f` — P4.D212)
// ===========================================================================

/// v4 `POST /api/v1/chats/[id]?action=rebuild-summary` — discard the chat's
/// running context summary and let the ordinary fold cadence rebuild it from
/// turn 1. The operator's remedy for a summary that has gone wrong — most
/// notably a speaker name the fold model invented and then carried forward
/// (bug 161).
///
/// Deliberately *not* the single-shot `forceRegenerate` path: that puts every
/// turn up to the tail floor into one request and will not fit a cheap model's
/// window on a long chat. Clearing the anchor instead means the existing
/// cadence folds `FOLD_TURN_BATCH` turns at a time, one bounded call per fire,
/// until it is back within `FOLD_TRIGGER_DELTA` of the head.
///
/// Refusals in v4's order: 404 (the chat read — v4's `handlePost`, before any
/// action runs), 409 on an autonomous room whose `runState` is `running`, 400
/// when the user has no connection profiles. ⚠ `e7821606f`'s commit message
/// names only the 409; the HUNK also carries the 400 — and does NOT check the
/// cheap-LLM settings the way regenerate-title does, so that sibling arm is not
/// carried here. Any failure after the refusals is v4's catch: an error log and
/// a 500 `Failed to rebuild the summary`; when the clearing update fails, the
/// enqueue and the publish never run.
///
/// Dispatch-only on v5 (`Request::ChatRebuildSummary`), the regenerate-title
/// precedent: the REST edge serves only equip / regenerate-avatar / inform /
/// cancel-inform, and its pointer sentence already sends every other chat
/// action to `POST /api/dispatch`. Answers through `Response::ChatAdmin`, so no
/// new `Response` variant and no REST unwrapper change.
pub async fn chat_rebuild_summary(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    now_iso: &str,
) -> Response {
    let chat = match load_chat(db, chat_id) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    match rebuild_summary(db, user_id, chat_id, &chat, now_iso).await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!(
                chatId = chat_id,
                error = %e,
                "[Chats v1] Failed to rebuild context summary"
            );
            server_error("Failed to rebuild the summary")
        }
    }
}

/// v4 `handleRebuildSummary`'s `try` body; an `Err` is its catch arm.
async fn rebuild_summary(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    chat: &Value,
    now_iso: &str,
) -> Result<Response, DbError> {
    // An autonomous room in flight owns its own summary cadence; pulling the
    // anchor out from under a running turn loop is the operator's call to make
    // from a paused room, not ours to make mid-run.
    if chat.get("chatType").and_then(Value::as_str) == Some("autonomous")
        && chat.get("runState").and_then(Value::as_str) == Some("running")
    {
        return Ok(Response::error(
            ErrorKind::Conflict,
            "Pause the room before rebuilding its summary.",
        ));
    }

    let uid = user_id.to_string();
    let profiles = db.read_main(move |c| connection_profiles::find_by_user_id(c, &uid))?;
    if profiles.is_empty() {
        return Ok(bad_request("No connection profiles available"));
    }

    // Same profile precedence as regenerate-title: the cast's own profile when
    // it has one, otherwise whatever is first. The cheap-LLM resolver derives
    // the summariser from it.
    let connection_profile_id = cast_or_first_profile(chat, &profiles)
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // One update: summary, its anchor set, and the fold cursor go together, so
    // there is no window where a fold could advance a cursor over a summary
    // that is already gone.
    //
    // `lastFullRebuildTurn` is deliberately left where it is. Zeroing it would
    // put the next gate evaluation over T_HARD_TURN_THRESHOLD on any chat past
    // turn 50 and route the rebuild straight into the single-shot path this
    // action exists to avoid.
    let cid = chat_id.to_string();
    let patch = ChatUpdate {
        context_summary: Some(None),
        summary_anchor_message_ids: Some(Vec::new()),
        last_summary_turn: Some(0.0),
        updated_at: Some(now_iso.to_string()),
        ..Default::default()
    };
    db.write(move |w| w.main().chats().update(&cid, &patch).map(|_| ()))
        .await?;

    // v4 passes no options, so `enqueueJob`'s defaults apply: priority 0,
    // maxAttempts 3, no dedupe.
    let job_id =
        enqueue_context_summary(db, user_id, chat_id, &connection_profile_id, false, 0.0).await?;

    crate::realtime::bus::publish_realtime(
        crate::realtime::types::RealtimeTopic::Chats,
        Some(chat_id),
    );

    tracing::info!(
        chatId = chat_id,
        jobId = job_id.as_str(),
        connectionProfileId = connection_profile_id.as_str(),
        "[Chats v1] Context summary cleared for rebuild"
    );

    Ok(ok(json!({ "success": true, "jobId": job_id })))
}

/// The `RawMessage` view `extract_visible_conversation` consumes.
fn raw_messages(events: &[Value]) -> Vec<crate::chat_tasks::RawMessage> {
    events
        .iter()
        .map(|e| crate::chat_tasks::RawMessage {
            type_: e.get("type").and_then(Value::as_str).map(str::to_string),
            role: e.get("role").and_then(Value::as_str).map(str::to_string),
            content: e.get("content").and_then(Value::as_str).map(str::to_string),
            // === P4.D205 (v4 `e7d77bb60`) — required-field spill ===
            system_kind: e
                .get("systemKind")
                .and_then(Value::as_str)
                .map(str::to_string),
            // === end P4.D205 ===
        })
        .collect()
}

#[cfg(test)]
mod rebuild_summary_tests {
    //! P4.D212: the rebuild-summary verb's non-DB observables — its two log
    //! lines (each with a silence leg), its realtime publish, and v4's catch
    //! arm: when the clearing update fails, the enqueue and the publish never
    //! run. The DB half is `chat_rebuild_summary_equivalence` (v4's REAL route)
    //! and `chat_rebuild_summary_dispatch_wire` (the serde trip).
    //!
    //! The update failure is INDUCED, not argued: a `BEFORE UPDATE ON chats …
    //! RAISE(ABORT)` trigger fails the one write while leaving the chat read
    //! and the profile read that precede it working — the one shape that
    //! reaches the catch arm past both refusals.

    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::realtime::publish_sites::HintCapture;
    use std::sync::{Arc, Mutex};

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c1000000-0000-4000-8000-0000000000d2";
    const SEAT: &str = "e1000000-0000-4000-8000-0000000000d2";
    const PROFILE: &str = "b1000000-0000-4000-8000-0000000000d2";
    const NOW: &str = "2026-09-22T00:00:00.000Z";

    fn provisioned() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("rs");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    async fn seed(db: &Db) {
        let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
            "userId": crate::api::SINGLE_USER_ID,
            "title": "The Rebuild Room",
            "participants": [
                { "id": SEAT, "type": "CHARACTER", "controlledBy": "llm",
                  "characterId": "a1000000-0000-4000-8000-0000000000d2",
                  "createdAt": NOW, "updatedAt": NOW }
            ],
        }))
        .expect("a ChatCreate");
        let opts = crate::db::chats::CreateOptions {
            id: CHAT.to_string(),
            created_at: NOW.to_string(),
            updated_at: NOW.to_string(),
        };
        let profile = connection_profiles::CpCreate {
            user_id: crate::api::SINGLE_USER_ID.to_string(),
            name: "Summariser".to_string(),
            provider: "OPENAI".to_string(),
            transport: "direct".to_string(),
            courier_delta_mode: false,
            api_key_id: None,
            base_url: None,
            model_name: "gpt-4o-mini".to_string(),
            parameters: json!({}),
            is_default: true,
            is_cheap: false,
            allow_web_search: false,
            use_native_web_search: false,
            allow_tool_use: false,
            pseudo_tool_mode: "auto".to_string(),
            multi_character_prefill: None,
            model_class: None,
            fallback_profile_id: None,
            allow_tier_fallback: false,
            max_context: None,
            max_tokens: None,
            is_dangerous_compatible: false,
            supports_image_upload: false,
            tags: vec![],
            sort_index: 0.0,
            total_tokens: 0.0,
            total_prompt_tokens: 0.0,
            total_completion_tokens: 0.0,
            message_count: 0.0,
        };
        let popts = connection_profiles::CreateOptions {
            id: PROFILE.to_string(),
            created_at: NOW.to_string(),
            updated_at: NOW.to_string(),
        };
        db.write(move |w| {
            crate::db::chats::ChatsRepository::new(w.main().connection()).create(&create, &opts)?;
            connection_profiles::ConnectionProfilesRepository::new(w.main().connection())
                .create(&profile, &popts)
        })
        .await
        .expect("seed the chat + profile");
    }

    fn summary_job_count(db: &Db) -> i64 {
        db.read_main(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM background_jobs WHERE type = 'CONTEXT_SUMMARY'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap()
    }

    /// Run the verb under a THREAD-SCOPED capturing subscriber (the guard
    /// spans the awaits on this current-thread runtime).
    async fn rebuild_captured(db: &Db) -> (Response, Vec<String>) {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber =
            tracing_subscriber::registry().with(crate::test_support::CaptureLayer(logs.clone()));
        let guard = tracing::subscriber::set_default(subscriber);
        let resp = chat_rebuild_summary(db, crate::api::SINGLE_USER_ID, CHAT, NOW).await;
        drop(guard);
        let lines = logs.lock().unwrap().clone();
        (resp, lines)
    }

    fn lines_with<'a>(lines: &'a [String], needle: &str) -> Vec<&'a String> {
        lines.iter().filter(|l| l.contains(needle)).collect()
    }

    #[tokio::test]
    async fn a_rebuild_logs_its_info_line_and_publishes_the_chats_topic() {
        let mut cap = HintCapture::start();
        let (_dir, db) = provisioned();
        seed(&db).await;

        let (resp, lines) = rebuild_captured(&db).await;
        let Response::ChatAdmin(body) = &resp else {
            panic!("expected the 200 body, got {resp:?}");
        };
        let job_id = body["jobId"].as_str().expect("a jobId").to_string();

        let info = lines_with(&lines, "[Chats v1] Context summary cleared for rebuild");
        assert_eq!(info.len(), 1, "exactly one info line: {lines:#?}");
        let info = info[0];
        assert!(
            info.starts_with("INFO quilltap_core::services::chat_admin"),
            "{info}"
        );
        for field in [
            format!(" chatId={CHAT}"),
            format!(" jobId={job_id}"),
            format!(" connectionProfileId={PROFILE}"),
        ] {
            assert!(info.contains(&field), "missing `{field}` in {info}");
        }
        assert!(
            lines_with(&lines, "Failed to rebuild context summary").is_empty(),
            "the success path says nothing of failure: {lines:#?}"
        );

        let hints = cap.drain_sorted().await;
        assert!(
            hints.contains(&("chats".to_string(), Some(CHAT.to_string()))),
            "v4 `publishRealtime('chats', chatId)` so the summary panel re-reads: {hints:?}"
        );
    }

    #[tokio::test]
    async fn a_refusal_logs_nothing_and_publishes_nothing() {
        let mut cap = HintCapture::start();
        let (_dir, db) = provisioned();
        seed(&db).await;
        db.write(|w| {
            w.main().connection().execute(
                "UPDATE chats SET chatType = 'autonomous', runState = 'running' WHERE id = ?1",
                [CHAT],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        let _ = cap.drain().await; // the seed's own hints

        let (resp, lines) = rebuild_captured(&db).await;
        assert!(
            matches!(&resp, Response::Error(e) if matches!(e.kind, ErrorKind::Conflict)),
            "{resp:?}"
        );
        assert!(
            lines_with(&lines, "[Chats v1]").is_empty(),
            "a refusal is not a rebuild and not a failure: {lines:#?}"
        );
        assert_eq!(summary_job_count(&db), 0);
        assert_eq!(cap.drain().await, vec![], "a refusal announces nothing");
    }

    #[tokio::test]
    async fn a_failed_update_answers_500_and_neither_enqueues_nor_publishes() {
        let mut cap = HintCapture::start();
        let (_dir, db) = provisioned();
        seed(&db).await;
        db.write(|w| {
            w.main().connection().execute_batch(
                "CREATE TRIGGER poison_chats_update BEFORE UPDATE ON chats \
                 BEGIN SELECT RAISE(ABORT, 'poisoned chats update'); END;",
            )?;
            Ok(())
        })
        .await
        .unwrap();
        let _ = cap.drain().await;

        let (resp, lines) = rebuild_captured(&db).await;
        match &resp {
            Response::Error(e) => {
                assert!(matches!(e.kind, ErrorKind::Internal), "{resp:?}");
                assert_eq!(e.message, "Failed to rebuild the summary");
            }
            other => panic!("expected v4's 500, got {other:?}"),
        }
        let err = lines_with(&lines, "[Chats v1] Failed to rebuild context summary");
        assert_eq!(err.len(), 1, "exactly one error line: {lines:#?}");
        assert!(
            err[0].starts_with("ERROR quilltap_core::services::chat_admin"),
            "{}",
            err[0]
        );
        assert!(err[0].contains(&format!(" chatId={CHAT}")), "{}", err[0]);
        assert!(
            err[0].contains("poisoned chats update"),
            "the error rides along: {}",
            err[0]
        );
        assert!(
            lines_with(&lines, "Context summary cleared for rebuild").is_empty(),
            "{lines:#?}"
        );

        assert_eq!(summary_job_count(&db), 0, "the enqueue never ran");
        assert!(
            !cap.drain()
                .await
                .contains(&("chats".to_string(), Some(CHAT.to_string()))),
            "the publish never ran"
        );
    }
}
