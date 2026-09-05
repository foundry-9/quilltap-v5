//! The help-chats dispatch family (P4.9I2A) — v4 `app/api/v1/help-chats/**`
//! (`route.ts`: list / `?action=eligibility` / create; `[id]/route.ts`: get /
//! rename / `?action=update-context` / delete; `[id]/messages/route.ts`:
//! messages / send). Nine verbs; eight are readiness-gated DB ops handled here,
//! the ninth (`HelpChatSend`) rides the host [`HelpChatSendDriver`] seam (the
//! orchestrator — `BrahmaConsoleSend`'s sibling).
//!
//! Help chats live in the shared `chats` table with `chatType = 'help'` and
//! `helpPageUrl` — both columns already round-trip through `db/chats.rs`; no
//! schema change. `helpPageUrl` is not a `ChatUpdate` setter, so update-context
//! writes it via a raw single-column `UPDATE chats` (the Brahma set-model
//! precedent).
//!
//! Response bodies mirror v4's route JSON name-for-name (the tier-2 differential
//! `help_chats_routes_equivalence` is the arbiter): `successResponse(data)`
//! returns `data` verbatim, `created(data)` the same at 201, `messageResponse` a
//! `{ message }`, and `verifyHelpChat`'s miss `notFound('Help chat')` →
//! `{ error: "Help chat not found" }` (404).
//!
//! ## Recorded divergences / v4 quirks reproduced
//!
//! - **`verifyHelpChat` never checks `userId`** despite its doc comment ("belongs
//!   to user") — a single-user instance; reproduced (no owner gate), recorded.
//! - v4's create returns the INPUT literal it built, so six explicit nulls
//!   survive in the 201 body where a re-read omits them (the Brahma precedent):
//!   re-injected here.
//! - The list sort is `new Date(b.updatedAt) - new Date(a.updatedAt)` — a NaN
//!   comparator on an unparseable stamp leaves that row where it was in v4;
//!   v5 sorts an unparseable `updatedAt` LAST (a total order is required). The
//!   fixture carries only parseable stamps; recorded.
//! - v4's eligibility avatar arm `img.tags?.includes('avatar')` is UNREACHABLE
//!   with valid data (`FileEntrySchema.tags` is `z.array(z.uuid())`, re-validated
//!   on read); reproduced literally — it falls through to `images[0]`.

use std::future::Future;
use std::pin::Pin;

use serde_json::{json, Value};

use crate::clock::{iso_to_ms, now_iso};
use crate::db::chats::{ChatCreate, ChatUpdate, CreateOptions};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_messages_read, chats_read, connection_profiles, DbError};
use crate::services::chat_enrichment::enrich_participant_summary;

use super::types::{CoreError, ErrorKind, Response};

// ===========================================================================
// The send driver seam (the orchestrator — `BrahmaConsoleSendDriver`'s sibling).
// ===========================================================================

/// The projected `HelpChatSend` a dispatch carries (the request fields, plus the
/// resolved single-user id). `file_ids` is accepted and then IGNORED, as v4's
/// orchestrator ignores it (`orchestrator.service.ts:84-92` saves the user
/// message with `attachments: []`) — pinned, not an attachment path.
#[derive(Debug, Clone, Default)]
pub struct HelpChatSendRequest {
    pub user_id: String,
    pub chat_id: String,
    pub content: String,
    pub file_ids: Vec<String>,
}

/// The boxed future a [`HelpChatSendDriver`] returns (the send reply body —
/// v4's send is pure SSE, so v5 returns `{ messageId }`, the LAST persisted
/// assistant message id or `null`).
pub type HelpChatSendFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, CoreError>> + Send + 'a>>;

/// One full help-chat send turn: dispatch → the orchestrator → frames on the
/// engine's `Event` broadcast (scope-tagged by `chatId`) → the typed result.
/// Only the composing host can construct the streaming/tool/cost bundle. The
/// implementation emits v4's transport-shell error frame itself on failure; the
/// engine only maps the returned error into the `Response` envelope. The
/// `help`-type gate is [`verify_help_chat`], upstream in the dispatch arm.
pub trait HelpChatSendDriver: Send + Sync {
    fn send(&self, req: HelpChatSendRequest) -> HelpChatSendFuture<'_>;
}

// ===========================================================================
// Helpers.
// ===========================================================================

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// JS truthiness over a JSON value (v4's `if (x)`).
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

fn not_found_help_chat() -> Response {
    // v4 `notFound('Help chat')` → `${resource} not found`.
    Response::error(ErrorKind::NotFound, "Help chat not found")
}
fn bad_request(msg: &str) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn internal(msg: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, msg.to_string())
}
/// v4's middleware sentence for any uncaught `ZodError`.
fn validation_error() -> Response {
    Response::error(
        ErrorKind::BadRequest,
        crate::services::chat_participants::VALIDATION_ERROR,
    )
}

/// `z.string().min(1, …)` on a raw body value (a wrong TYPE refuses too).
fn zod_min1_string(v: &Value) -> Result<String, Response> {
    match v {
        Value::String(s) if !s.is_empty() => Ok(s.clone()),
        _ => Err(validation_error()),
    }
}

/// Run `f` with BOTH a main and a mount-index connection (the character overlay
/// needs both).
fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// v4 `verifyHelpChat(id, context)`: the chat exists AND is `chatType === 'help'`
/// — else a 404. ⚠ No `userId` check (see the module doc). Returns the overlaid
/// chat `Value` on success.
pub fn verify_help_chat(db: &Db, chat_id: &str) -> Result<Value, Response> {
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(c) => c,
        Err(e) => return Err(internal(e)),
    };
    match chat {
        Some(chat) if s(&chat, "chatType").as_deref() == Some("help") => Ok(chat),
        _ => Err(not_found_help_chat()),
    }
}

fn message_count(db: &Db, chat_id: &str) -> Result<usize, Response> {
    let cid = chat_id.to_string();
    db.read_main(move |c| chats_messages_read::get_messages(c, &cid))
        .map(|m| m.len())
        .map_err(internal)
}

/// `chat.participants.map(p => enrichParticipantSummary(p, repos))`.
fn enrich_participants(db: &Db, chat: &Value) -> Result<Vec<Value>, Response> {
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    read_main_mount(db, |main, mount| {
        participants
            .iter()
            .map(|p| {
                enrich_participant_summary(main, mount, p).and_then(|e| {
                    serde_json::to_value(e).map_err(|e| DbError::Internal(e.to_string()))
                })
            })
            .collect::<Result<Vec<_>, _>>()
    })
    .map_err(internal)
}

/// Persist one SYSTEM message (v4's `repos.chats.addMessage(id, { type:
/// 'message', id: randomUUID(), role: 'SYSTEM', content, attachments: [],
/// createdAt })`).
async fn add_system_message(db: &Db, chat_id: &str, content: String) -> Result<(), Response> {
    let event: crate::db::chats_messages::ChatEventInput = serde_json::from_value(json!({
        "type": "message",
        "id": uuid::Uuid::new_v4().to_string(),
        "role": "SYSTEM",
        "content": content,
        "attachments": [],
        "createdAt": now_iso(),
    }))
    .map_err(|e| internal(format!("help system message marshal: {e}")))?;
    let cid = chat_id.to_string();
    db.write(move |ws| ws.main().chat_messages().add_message(&cid, &event))
        .await
        .map_err(internal)
}

// ===========================================================================
// The collection endpoint (v4 route.ts).
// ===========================================================================

/// v4 `handleList` (`route.ts:42-72`): the user's `help` chats, most recently
/// updated first, each with enriched participants, its message count and
/// `helpPageUrl || null`.
pub fn help_chat_list(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    let all = match db.read_main(move |c| chats_read::find_by_user_id(c, &uid)) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let mut help: Vec<Value> = all
        .into_iter()
        .filter(|c| s(c, "chatType").as_deref() == Some("help"))
        .collect();
    // `new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()` — a
    // stable sort; an unparseable stamp (JS NaN) sorts LAST here (module doc).
    let ms = |c: &Value| s(c, "updatedAt").and_then(|t| iso_to_ms(&t));
    help.sort_by_key(|c| std::cmp::Reverse(ms(c).unwrap_or(i64::MIN)));

    let mut enriched = Vec::with_capacity(help.len());
    for chat in &help {
        let id = s(chat, "id").unwrap_or_default();
        let participants = match enrich_participants(db, chat) {
            Ok(p) => p,
            Err(r) => return r,
        };
        let count = match message_count(db, &id) {
            Ok(n) => n,
            Err(r) => return r,
        };
        enriched.push(json!({
            "id": id,
            "title": chat.get("title").cloned().unwrap_or(Value::Null),
            "updatedAt": chat.get("updatedAt").cloned().unwrap_or(Value::Null),
            "participants": participants,
            "messageCount": count,
            // `(chat as any).helpPageUrl || null` — JS `||`, so '' → null.
            "helpPageUrl": if truthy(chat.get("helpPageUrl")) { chat["helpPageUrl"].clone() } else { Value::Null },
        }));
    }
    Response::HelpChat(json!({ "chats": enriched }))
}

/// v4 `handleEligibility` (`route.ts:77-120`): `{ eligible, characters, reasons }`.
pub fn help_chat_eligibility(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    let uid2 = user_id.to_string();
    let out = read_main_mount(db, |main, mount| {
        let characters = characters_read::find_by_user_id(main, mount, &uid)?;
        let profiles = connection_profiles::find_by_user_id(main, &uid2)?;
        // `c.defaultHelpToolsEnabled === true` — strict.
        let help_characters: Vec<&Value> = characters
            .iter()
            .filter(|c| c.get("defaultHelpToolsEnabled") == Some(&Value::Bool(true)))
            .collect();
        // `p.allowToolUse !== false` — absent / null / true all count.
        let tool_capable: Vec<&Value> = profiles
            .iter()
            .filter(|p| p.get("allowToolUse") != Some(&Value::Bool(false)))
            .collect();

        let mut eligible_characters = Vec::with_capacity(help_characters.len());
        for ch in &help_characters {
            let char_id = s(ch, "id").unwrap_or_default();
            let default_profile = ch.get("defaultConnectionProfileId");
            let has_tool_capable = if truthy(default_profile) {
                let want = default_profile.and_then(Value::as_str).unwrap_or("");
                tool_capable
                    .iter()
                    .any(|p| s(p, "id").as_deref() == Some(want))
            } else {
                !tool_capable.is_empty()
            };

            // `charAny.avatarUrl || null`, else the linked files.
            let mut avatar_url: Value = if truthy(ch.get("avatarUrl")) {
                ch["avatarUrl"].clone()
            } else {
                Value::Null
            };
            if avatar_url.is_null() {
                // v4 `repos.files.findByLinkedTo(char.id)` — every file whose
                // `linkedTo` JSON array contains the id, rowid order (v4's
                // unordered scan); then the first tagged `avatar`, else `images[0]`.
                let mut stmt = main.prepare(
                    "SELECT id, tags FROM files \
                     WHERE EXISTS (SELECT 1 FROM json_each(files.linkedTo) WHERE value = ?1)",
                )?;
                let images: Vec<(String, Vec<String>)> = stmt
                    .query_map(rusqlite::params![char_id], |r| {
                        let id: String = r.get(0)?;
                        let tags: Option<String> = r.get(1)?;
                        Ok((id, tags))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|(id, tags)| {
                        let tags: Vec<String> = tags
                            .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
                            .unwrap_or_default();
                        (id, tags)
                    })
                    .collect();
                let avatar_img = images
                    .iter()
                    .find(|(_, tags)| tags.iter().any(|t| t == "avatar"))
                    .or(images.first());
                if let Some((id, _)) = avatar_img {
                    avatar_url = Value::String(format!("/api/v1/files/{id}"));
                }
            }

            eligible_characters.push(json!({
                "id": char_id,
                "name": ch.get("name").cloned().unwrap_or(Value::Null),
                "avatarUrl": avatar_url,
                "defaultHelpToolsEnabled": true,
                // `charAny.defaultConnectionProfileId || null`.
                "connectionProfileId": if truthy(default_profile) { default_profile.cloned().unwrap() } else { Value::Null },
                "hasToolCapableProfile": has_tool_capable,
            }));
        }

        let eligible = eligible_characters
            .iter()
            .any(|c| c["hasToolCapableProfile"] == Value::Bool(true));
        let mut reasons: Vec<&str> = Vec::new();
        if help_characters.is_empty() {
            reasons.push("No characters have help tools enabled");
        }
        if !eligible && !help_characters.is_empty() {
            reasons.push("No tool-capable connection profiles available");
        }
        Ok(json!({ "eligible": eligible, "characters": eligible_characters, "reasons": reasons }))
    });
    match out {
        Ok(v) => Response::HelpChat(v),
        Err(e) => internal(e),
    }
}

/// v4 `createHelpChatSchema.parse(body)` — `characterIds: z.array(z.string()
/// .uuid()).min(1)`, `pageUrl: z.string()` — uncaught, so any failure is the
/// flat 400 `Validation error`. Runs BEFORE any lookup.
fn parse_create_body(
    character_ids: &Value,
    page_url: &Value,
) -> Result<(Vec<String>, String), Response> {
    let ids = match character_ids {
        Value::Array(a) if !a.is_empty() => {
            let mut out = Vec::with_capacity(a.len());
            for item in a {
                match item {
                    Value::String(s) if super::chat_outfits::is_zod_uuid(s) => out.push(s.clone()),
                    _ => return Err(validation_error()),
                }
            }
            out
        }
        _ => return Err(validation_error()),
    };
    let page_url = match page_url {
        Value::String(s) => s.clone(),
        _ => return Err(validation_error()),
    };
    Ok((ids, page_url))
}

/// v4 `handleCreate` (`route.ts:125-213`): parse → every character must exist
/// (`notFound('Character')` on the FIRST miss, BEFORE the help check) → at least
/// one help-enabled (else 400) → participants in order → `chats.create` with
/// `chatType: 'help'` + `helpPageUrl` → the SYSTEM `Help chat initiated…` row →
/// `created({ chat: {...chat, participants: enriched} })` (201 at the edge).
pub async fn help_chat_create(
    db: &Db,
    user_id: &str,
    character_ids: &Value,
    page_url: &Value,
) -> Response {
    let (ids, page_url) = match parse_create_body(character_ids, page_url) {
        Ok(x) => x,
        Err(r) => return r,
    };

    // Validate all characters exist and at least one has help tools enabled.
    let ids_for_read = ids.clone();
    let characters: Vec<Value> = match read_main_mount(db, |main, mount| {
        let mut out = Vec::with_capacity(ids_for_read.len());
        for id in &ids_for_read {
            match characters_read::find_by_id(main, mount, id)? {
                Some(c) => out.push(c),
                // Sentinel for "the first miss" — mapped to the 404 below.
                None => return Ok(Vec::new()),
            }
        }
        Ok(out)
    }) {
        Ok(v) if v.len() == ids.len() => v,
        Ok(_) => return Response::error(ErrorKind::NotFound, "Character not found"),
        Err(e) => return internal(e),
    };
    // `if ((character as any).defaultHelpToolsEnabled)` — JS truthiness.
    if !characters
        .iter()
        .any(|c| truthy(c.get("defaultHelpToolsEnabled")))
    {
        return bad_request("At least one character must have help tools enabled");
    }

    // Build participants.
    let now = now_iso();
    let participants: Vec<Value> = characters
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "type": "CHARACTER",
                "characterId": ch.get("id").cloned().unwrap_or(Value::Null),
                "controlledBy": "llm",
                // `char.defaultConnectionProfileId || null`.
                "connectionProfileId": if truthy(ch.get("defaultConnectionProfileId")) { ch["defaultConnectionProfileId"].clone() } else { Value::Null },
                "imageProfileId": null,
                "displayOrder": i,
                "isActive": true,
                "createdAt": now,
                "updatedAt": now,
            })
        })
        .collect();

    // Create the chat (v4's `handleCreate` object literal; `ChatCreate`'s field
    // defaults fill the rest — the Brahma create idiom).
    let first_name = s(&characters[0], "name").unwrap_or_default();
    let new_id = uuid::Uuid::new_v4().to_string();
    let data: ChatCreate = match serde_json::from_value(json!({
        "userId": user_id,
        "participants": participants,
        "title": format!("Help: {first_name}"),
        "contextSummary": null,
        "tags": [],
        "roleplayTemplateId": null,
        "timestampConfig": null,
        "messageCount": 0,
        "lastMessageAt": null,
        "lastRenameCheckInterchange": 0,
        "projectId": null,
        "disabledTools": [],
        "disabledToolGroups": [],
        "imageProfileId": null,
        "chatType": "help",
        "helpPageUrl": page_url,
    })) {
        Ok(d) => d,
        Err(e) => return internal(format!("help chat create marshal: {e}")),
    };
    let opts = CreateOptions {
        id: new_id.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    if let Err(e) = db
        .write(move |ws| ws.main().chats().create(&data, &opts))
        .await
    {
        return internal(e);
    }
    // v4 reads the created chat back through `create`'s return (the INPUT
    // literal) — re-read here, THEN the system row (v4's order: create → the
    // system message → the response built from the create return).
    let cid = new_id.clone();
    let mut chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return internal("created help chat vanished"),
        Err(e) => return internal(e),
    };

    // Add initial system message.
    if let Err(r) = add_system_message(
        db,
        &new_id,
        format!("Help chat initiated for page: {page_url}"),
    )
    .await
    {
        return r;
    }

    // v4's `handleCreate` returns the input object it built — which carries the
    // caller's EXPLICIT nulls (`contextSummary` / `roleplayTemplateId` /
    // `timestampConfig` / `lastMessageAt` / `projectId` / `imageProfileId`) —
    // NOT a re-read. `find_by_id` OMITS null columns, so re-inject exactly those
    // six to reproduce the create-return body (the Brahma precedent).
    if let Value::Object(o) = &mut chat {
        for k in [
            "contextSummary",
            "roleplayTemplateId",
            "timestampConfig",
            "lastMessageAt",
            "projectId",
            "imageProfileId",
        ] {
            o.entry(k.to_string()).or_insert(Value::Null);
        }
    }
    let enriched = match enrich_participants(db, &chat) {
        Ok(p) => p,
        Err(r) => return r,
    };
    if let Value::Object(o) = &mut chat {
        o.insert("participants".to_string(), Value::Array(enriched));
    }
    tracing::info!(
        target: "quilltap::help",
        chat_id = %new_id,
        user_id = %user_id,
        character_count = characters.len(),
        page_url = %page_url,
        "Help chat created"
    );
    Response::HelpChat(json!({ "chat": chat }))
}

// ===========================================================================
// The item endpoint (v4 [id]/route.ts).
// ===========================================================================

/// v4 `handleGet` (`[id]/route.ts:69-93`): `{ chat: {...chat, participants:
/// enriched, messageCount} }`.
pub fn help_chat_get(db: &Db, chat_id: &str) -> Response {
    let mut chat = match verify_help_chat(db, chat_id) {
        Ok(c) => c,
        Err(r) => return r,
    };
    let enriched = match enrich_participants(db, &chat) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let count = match message_count(db, chat_id) {
        Ok(n) => n,
        Err(r) => return r,
    };
    if let Value::Object(o) = &mut chat {
        o.insert("participants".to_string(), Value::Array(enriched));
        o.insert("messageCount".to_string(), json!(count));
    }
    Response::HelpChat(json!({ "chat": chat }))
}

/// v4 `handleRename` (`[id]/route.ts:98-123`): verify FIRST, then
/// `renameSchema.parse` (`title: z.string().min(1)`); `chats.update(id, {title,
/// isManuallyRenamed: true})`; null → `serverError('Failed to update help chat')`;
/// → `{ chat: updated }`.
pub async fn help_chat_rename(db: &Db, chat_id: &str, title: &Value) -> Response {
    if let Err(r) = verify_help_chat(db, chat_id) {
        return r;
    }
    let title = match zod_min1_string(title) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let cid = chat_id.to_string();
    let update = ChatUpdate {
        title: Some(title.clone()),
        is_manually_renamed: Some(true),
        ..Default::default()
    };
    let cid_w = cid.clone();
    match db
        .write(move |ws| ws.main().chats().update(&cid_w, &update))
        .await
    {
        Ok(true) => {}
        Ok(false) => return internal("Failed to update help chat"),
        Err(e) => return internal(e),
    }
    tracing::info!(target: "quilltap::help", chat_id = %chat_id, title = %title, "Help chat renamed");
    match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(chat)) => Response::HelpChat(json!({ "chat": chat })),
        _ => internal("Failed to update help chat"),
    }
}

/// v4 `handleUpdateContext` (`[id]/route.ts:128-164`): verify FIRST, then
/// `updateContextSchema.parse` (`pageUrl: z.string().min(1)`); `chats.update(id,
/// {helpPageUrl})` (a raw single-column UPDATE + `updatedAt`, `helpPageUrl` not
/// being a `ChatUpdate` setter); null → `serverError('Failed to update help chat
/// context')`; then the SYSTEM `[System: User navigated to …]` row; → `{ chat:
/// updated }` — the update's return, read BEFORE the system row lands (v4's
/// order).
pub async fn help_chat_update_context(db: &Db, chat_id: &str, page_url: &Value) -> Response {
    if let Err(r) = verify_help_chat(db, chat_id) {
        return r;
    }
    let page_url = match zod_min1_string(page_url) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let cid = chat_id.to_string();
    let url = page_url.clone();
    let now = now_iso();
    let cid_r = cid.clone();
    let out = db
        .write(move |ws| {
            let n = ws.main().connection().execute(
                "UPDATE chats SET helpPageUrl = ?1, updatedAt = ?2 WHERE id = ?3",
                rusqlite::params![url, now, cid],
            )?;
            Ok::<usize, DbError>(n)
        })
        .await;
    match out {
        Ok(n) if n > 0 => {}
        Ok(_) => return internal("Failed to update help chat context"),
        Err(e) => return internal(e),
    }
    let updated = match db.read_main(move |c| chats_read::find_by_id(c, &cid_r)) {
        Ok(Some(chat)) => chat,
        _ => return internal("Failed to update help chat context"),
    };
    // Inject a system message noting the navigation.
    if let Err(r) = add_system_message(
        db,
        chat_id,
        format!("[System: User navigated to {page_url}]"),
    )
    .await
    {
        return r;
    }
    tracing::info!(target: "quilltap::help", chat_id = %chat_id, page_url = %page_url, "Help chat context updated");
    Response::HelpChat(json!({ "chat": updated }))
}

/// v4 `handleDelete` (`[id]/route.ts:169-187`): verify; `chats.delete` false →
/// `serverError('Failed to delete help chat')`; → `{ message: 'Help chat deleted
/// successfully' }`.
pub async fn help_chat_delete(db: &Db, chat_id: &str) -> Response {
    if let Err(r) = verify_help_chat(db, chat_id) {
        return r;
    }
    let cid = chat_id.to_string();
    match db.write(move |ws| ws.main().chats().delete(&cid)).await {
        Ok(true) => {
            tracing::info!(target: "quilltap::help", chat_id = %chat_id, "Help chat deleted");
            Response::HelpChat(json!({ "message": "Help chat deleted successfully" }))
        }
        Ok(false) => internal("Failed to delete help chat"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// The messages endpoint (v4 [id]/messages/route.ts).
// ===========================================================================

/// v4 `handleGetMessages` (`messages/route.ts:89-102`): verify; `{ messages }`.
pub fn help_chat_messages(db: &Db, chat_id: &str) -> Response {
    if let Err(r) = verify_help_chat(db, chat_id) {
        return r;
    }
    let cid = chat_id.to_string();
    match db.read_main(move |c| chats_messages_read::get_messages(c, &cid)) {
        Ok(messages) => Response::HelpChat(json!({ "messages": messages })),
        Err(e) => internal(e),
    }
}

/// v4 `handleSendMessage`'s prologue, in its ORDER: `verifyHelpChat` first,
/// `sendMessageSchema.parse` second (`content: z.string().min(1)`, `fileIds:
/// z.array(z.string().uuid()).optional()` — the SAME schema as the Brahma
/// console's, whose parser is reused here). A bad body on a chat that is not a
/// help chat is a **404**, not a 400.
pub fn help_chat_send_prepare(
    db: &Db,
    chat_id: &str,
    content: &Value,
    file_ids: Option<&Value>,
) -> Result<(String, Vec<String>), Response> {
    verify_help_chat(db, chat_id)?;
    super::brahma::parse_brahma_send_body(content, file_ids)
}

/// Capture-layer pins for the four v4 route log lines this module carries
/// (`Help chat created` / `Help chat renamed` / `Help chat context updated` /
/// `Help chat deleted`), over a fresh copy of the committed `help-chat-*` fixture.
#[cfg(test)]
mod log_context_tests {
    use super::*;
    use crate::db::runtime::DbPaths;
    use std::sync::{Arc, Mutex};

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const USER_A: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
    const C1: &str = "b0000002-0000-4000-8000-000000000001";
    const H2: &str = "c1000002-0000-4000-8000-000000000002";
    const H3: &str = "c1000002-0000-4000-8000-000000000003";

    struct FieldVisitor(String);
    impl tracing::field::Visit for FieldVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0.push_str(&format!(" {}={:?}", field.name(), value));
        }
    }
    struct CaptureLayer(Arc<Mutex<Vec<String>>>);
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _c: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let meta = event.metadata();
            let mut v = FieldVisitor(format!("{} {}", meta.level(), meta.target()));
            event.record(&mut v);
            self.0.lock().unwrap().push(v.0);
        }
    }
    fn fixture_db() -> (tempfile::TempDir, Db) {
        let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../quilltap-web/tests/fixtures");
        let dir = tempfile::tempdir().unwrap();
        std::fs::copy(src.join("help-chat-main.db"), dir.path().join("main.db")).unwrap();
        std::fs::copy(src.join("help-chat-mount.db"), dir.path().join("mount.db")).unwrap();
        let db = Db::open(
            DbPaths {
                main: dir.path().join("main.db"),
                mount_index: Some(dir.path().join("mount.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    #[tokio::test]
    async fn the_four_route_lines_fire() {
        use tracing_subscriber::layer::SubscriberExt;
        let (_dir, db) = fixture_db();
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let sub = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        let _g = tracing::subscriber::set_default(sub);

        let created = help_chat_create(&db, USER_A, &json!([C1]), &json!("/salon")).await;
        assert!(matches!(created, Response::HelpChat(_)), "{created:?}");
        let _ = help_chat_rename(&db, H2, &json!("Renamed")).await;
        let _ = help_chat_update_context(&db, H2, &json!("/files")).await;
        let _ = help_chat_delete(&db, H3).await;

        let lines = logs.lock().unwrap().clone();
        for needle in [
            "Help chat created",
            "Help chat renamed",
            "Help chat context updated",
            "Help chat deleted",
        ] {
            assert!(
                lines
                    .iter()
                    .any(|l| l.contains("INFO") && l.contains(needle)),
                "missing {needle:?} in {lines:?}"
            );
        }
        assert!(lines.iter().any(|l| l.contains("Help chat created")
            && l.contains("character_count=1")
            && l.contains("page_url=/salon")));
    }
}
