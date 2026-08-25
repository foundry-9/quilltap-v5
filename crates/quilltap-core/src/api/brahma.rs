//! The dedicated brahma-console CRUD dispatch family (P4.9I1A) — v4
//! `app/api/v1/brahma-console/**` (`route.ts` + `[id]/route.ts` +
//! `[id]/messages/route.ts` + `_shared.ts`). Eight verbs; seven are plain
//! readiness-gated DB ops handled here, the eighth (`BrahmaConsoleSend`) rides the
//! host `BrahmaConsoleSendDriver` seam (the orchestrator — `chat_send`'s sibling).
//!
//! Brahma chats are DEDICATED routes but drive the shared `repos.chats` repo
//! underneath, which v5 already round-trips generically (`chat_type` on create,
//! `console_connection_profile_id` a create column). No schema change; set-model
//! writes `consoleConnectionProfileId` via a raw single-column `UPDATE chats`
//! (the standalone-write precedent — `ChatUpdate` carries no such setter).
//!
//! Response bodies mirror v4's route JSON name-for-name (the tier-2 differential
//! `brahma_console_routes_equivalence` is the arbiter): `successResponse(data)`
//! returns `data` verbatim, `created(data)` the same at 201, `messageResponse` a
//! `{ message }`, and `verifyBrahmaChat`'s miss a `notFound('Brahma Console chat')`
//! → `{ error: "Brahma Console chat not found" }` (404).

use std::future::Future;
use std::pin::Pin;

use serde_json::{json, Value};

use crate::clock::now_iso;
use crate::db::chats::{ChatCreate, ChatUpdate, CreateOptions};
use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, connection_profiles};

use super::types::{CoreError, ErrorKind, Response};

// ===========================================================================
// The send driver seam (the orchestrator — `ChatSendDriver`'s sibling).
// ===========================================================================

/// The projected `BrahmaConsoleSend` a dispatch carries (the request fields, plus
/// the resolved single-user id).
#[derive(Debug, Clone, Default)]
pub struct BrahmaConsoleSendRequest {
    pub user_id: String,
    pub chat_id: String,
    pub content: String,
    pub file_ids: Vec<String>,
}

/// The boxed future a [`BrahmaConsoleSendDriver`] returns (the send reply body —
/// v4's send has no JSON body, so v5 returns `{ messageId }`).
pub type BrahmaConsoleSendFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, CoreError>> + Send + 'a>>;

/// One full Brahma send turn: dispatch → the orchestrator → frames on the engine's
/// `Event` broadcast → the typed result (the `ChatSendDriver` precedent — only the
/// composing host can construct the streaming/tool/cost provider bundle). The
/// implementation emits v4's transport-shell error frame itself on failure; the
/// engine only maps the returned error into the `Response` envelope. The
/// owner/`brahma`-type gate is [`verify_brahma_chat`], upstream in the dispatch arm.
pub trait BrahmaConsoleSendDriver: Send + Sync {
    fn send(&self, req: BrahmaConsoleSendRequest) -> BrahmaConsoleSendFuture<'_>;
}

/// The seed title a fresh Brahma chat carries (v4 `route.ts:96`) — exact bytes.
const SEED_TITLE: &str = "A Fresh Audience at the Console";

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn not_found() -> Response {
    // v4 `notFound('Brahma Console chat')` → `${resource} not found`.
    Response::error(ErrorKind::NotFound, "Brahma Console chat not found")
}
fn bad_request(msg: &str) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn internal(msg: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, msg.to_string())
}

/// v4 `sendMessageSchema` (`[id]/messages/route.ts:19-22`):
///
/// ```text
/// content: z.string().min(1, 'Message content is required')
/// fileIds: z.array(z.string().uuid()).optional()
/// ```
///
/// `handleSendMessage` calls `.parse` **uncaught**, so a failure is the
/// middleware's flat 400 `{error: 'Validation error'}` — the schema's own
/// `'Message content is required'` sentence lives only in the deferred
/// `details`. And the parse runs AFTER `verifyBrahmaChat`, so a bad body on a
/// chat that isn't a Brahma console answers **404**, not 400.
///
/// P4.60: the web edge used to read `content` with `and_then(Value::as_str)`
/// and `fileIds` with `as_array` + a `filter_map`, so `content: 123` answered a
/// sentence v4 never emits and `fileIds: "x"` / `fileIds: ["not-a-uuid"]` were
/// quietly emptied instead of refused.
pub fn parse_brahma_send_body(
    content: &Value,
    file_ids: Option<&Value>,
) -> Result<(String, Vec<String>), Response> {
    let invalid = validation_error;

    // content: z.string().min(1)
    let content = match content {
        Value::String(s) if !s.is_empty() => s.clone(),
        _ => return Err(invalid()),
    };

    // fileIds: z.array(z.string().uuid()).optional() — optional, NOT nullable,
    // so an explicit `null` is a ZodError where an absent key is not.
    let ids = match file_ids {
        None => Vec::new(),
        Some(Value::Array(a)) => {
            let mut out = Vec::with_capacity(a.len());
            for item in a {
                match item {
                    Value::String(s) if super::chat_outfits::is_zod_uuid(s) => out.push(s.clone()),
                    _ => return Err(invalid()),
                }
            }
            out
        }
        Some(_) => return Err(invalid()),
    };

    Ok((content, ids))
}

/// `z.string().min(1, …)` on a raw body value, refused as v4's uncaught-ZodError
/// 400. P4.60: the edge used to read these with `and_then(Value::as_str)`, which
/// answered the schema's own sentence (a string that only ever appears inside
/// the deferred `details`) and did not refuse a wrong TYPE at all.
fn zod_min1_string(v: &Value) -> Result<String, Response> {
    match v {
        Value::String(s) if !s.is_empty() => Ok(s.clone()),
        _ => Err(validation_error()),
    }
}

/// `z.string().uuid(…)` on a raw body value.
fn zod_uuid_string(v: &Value) -> Result<String, Response> {
    match v {
        Value::String(s) if super::chat_outfits::is_zod_uuid(s) => Ok(s.clone()),
        _ => Err(validation_error()),
    }
}

/// v4's middleware sentence for any uncaught `ZodError`.
fn validation_error() -> Response {
    Response::error(
        ErrorKind::BadRequest,
        crate::services::chat_participants::VALIDATION_ERROR,
    )
}

/// v4 `handleSendMessage`'s prologue, in its ORDER: `verifyBrahmaChat` first,
/// `sendMessageSchema.parse` second. Keeping the two together is what stops the
/// pair drifting apart — a body validated at the transport edge would answer a
/// 400 where v4's find-first answers 404.
pub fn brahma_send_prepare(
    db: &Db,
    chat_id: &str,
    user_id: &str,
    content: &Value,
    file_ids: Option<&Value>,
) -> Result<(String, Vec<String>), Response> {
    verify_brahma_chat(db, chat_id, user_id)?;
    parse_brahma_send_body(content, file_ids)
}

/// v4 `verifyBrahmaChat(id, context)` (`_shared.ts`): the chat exists, is owned by
/// the user, AND is `chatType === 'brahma'` — else a 404. Returns the overlaid
/// chat `Value` on success.
pub fn verify_brahma_chat(db: &Db, chat_id: &str, user_id: &str) -> Result<Value, Response> {
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(c) => c,
        Err(e) => return Err(internal(e)),
    };
    match chat {
        Some(chat)
            if s(&chat, "userId").as_deref() == Some(user_id)
                && s(&chat, "chatType").as_deref() == Some("brahma") =>
        {
            Ok(chat)
        }
        _ => Err(not_found()),
    }
}

fn message_count(db: &Db, chat_id: &str) -> Result<usize, Response> {
    let cid = chat_id.to_string();
    db.read_main(move |c| chats_messages_read::get_messages(c, &cid))
        .map(|m| m.len())
        .map_err(internal)
}

// ===========================================================================
// The collection endpoint (v4 route.ts).
// ===========================================================================

/// v4 `handleList` (`route.ts:37-60`): the user's brahma chats, most-recent first
/// (`lastMessageAt || updatedAt` desc), each enriched with its message count.
pub fn brahma_console_list(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    let all = match db.read_main(move |c| chats_read::find_by_user_id(c, &uid)) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let mut brahma: Vec<Value> = all
        .into_iter()
        .filter(|c| s(c, "chatType").as_deref() == Some("brahma"))
        .collect();
    // v4 sorts by `new Date(lastMessageAt || updatedAt).getTime()` desc; ISO-Z
    // strings sort chronologically, and the sort is stable (matching v4's
    // stable Array.sort over the same `findByUserId` order).
    let effective = |c: &Value| -> String {
        s(c, "lastMessageAt")
            .filter(|v| !v.is_empty())
            .or_else(|| s(c, "updatedAt"))
            .unwrap_or_default()
    };
    // Stable sort, descending by the effective timestamp (v4's `getTime()` desc
    // over ISO-Z strings, and a stable Array.sort over the same `findByUserId`
    // order for ties).
    brahma.sort_by_key(|c| std::cmp::Reverse(effective(c)));

    let mut enriched = Vec::with_capacity(brahma.len());
    for chat in &brahma {
        let id = s(chat, "id").unwrap_or_default();
        let count = match message_count(db, &id) {
            Ok(n) => n,
            Err(r) => return r,
        };
        enriched.push(json!({
            "id": id,
            "title": chat.get("title").cloned().unwrap_or(Value::Null),
            "updatedAt": chat.get("updatedAt").cloned().unwrap_or(Value::Null),
            "lastMessageAt": chat.get("lastMessageAt").cloned().unwrap_or(Value::Null),
            "messageCount": count,
            "consoleConnectionProfileId": chat.get("consoleConnectionProfileId").cloned().unwrap_or(Value::Null),
        }));
    }
    Response::BrahmaConsole(json!({ "chats": enriched }))
}

/// v4 `handleCreate` (`route.ts:65-113`): create a brahma chat on the requested
/// profile (else the user's default), seeded with the exact title. Body
/// `{ chat }` at 201.
pub async fn brahma_console_create(
    db: &Db,
    user_id: &str,
    requested_profile_id: Option<&Value>,
) -> Response {
    // `createBrahmaChatSchema.parse(body ?? {})` — uncaught, and BEFORE any
    // lookup. `connectionProfileId` is `z.string().uuid().optional()`: optional
    // but not nullable, so an explicit null (or an empty string, or a non-uuid)
    // is a 400 `Validation error` rather than "fall back to the default".
    let requested_profile_id = match requested_profile_id {
        // `.optional()` — an ABSENT key is the only thing that falls back.
        None => None,
        Some(v) => match zod_uuid_string(v) {
            Ok(pid) => Some(pid),
            Err(r) => return r,
        },
    };
    // Resolve the starting profile: the requested one (must exist + be owned),
    // else the user's default.
    let profile_id: String = match requested_profile_id.as_deref() {
        Some(pid) => {
            let p = pid.to_string();
            match db.read_main(move |c| connection_profiles::find_by_id(c, &p)) {
                Ok(Some(prof)) if s(&prof, "userId").as_deref() == Some(user_id) => pid.to_string(),
                Ok(_) => return bad_request("Connection profile not found"),
                Err(e) => return internal(e),
            }
        }
        None => {
            let uid = user_id.to_string();
            match db.read_main(move |c| connection_profiles::find_default(c, &uid)) {
                Ok(Some(def)) => s(&def, "id").unwrap_or_default(),
                Ok(None) => return bad_request(
                    "No connection profile available — establish one before opening the Console.",
                ),
                Err(e) => return internal(e),
            }
        }
    };

    let new_id = uuid::Uuid::new_v4().to_string();
    let now = now_iso();
    // Build the create payload as v4's `handleCreate` object literal and marshal
    // via serde (`ChatCreate`'s field defaults fill the rest — the chat-create
    // driver's idiom; `ChatCreate` is not `Default`).
    let data: ChatCreate = match serde_json::from_value(json!({
        "userId": user_id,
        "participants": [],
        "title": SEED_TITLE,
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
        "chatType": "brahma",
        "consoleConnectionProfileId": profile_id,
    })) {
        Ok(d) => d,
        Err(e) => return internal(format!("brahma create marshal: {e}")),
    };
    let opts = CreateOptions {
        id: new_id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    if let Err(e) = db
        .write(move |ws| ws.main().chats().create(&data, &opts))
        .await
    {
        return internal(e);
    }
    match db.read_main(move |c| chats_read::find_by_id(c, &new_id)) {
        Ok(Some(mut chat)) => {
            // v4's `handleCreate` returns the input object it built — which carries
            // the caller's EXPLICIT nulls (`contextSummary` / `roleplayTemplateId` /
            // `timestampConfig` / `lastMessageAt` / `projectId` / `imageProfileId`)
            // — NOT a re-read. `find_by_id` OMITS null columns (matching v4's
            // update-return shape), so re-inject exactly those six explicit nulls
            // to reproduce the create-return body byte-for-byte.
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
            Response::BrahmaConsole(json!({ "chat": chat }))
        }
        Ok(None) => internal("created brahma chat vanished"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// The item endpoint (v4 [id]/route.ts).
// ===========================================================================

/// v4 `handleGet` (`[id]/route.ts:42`): the chat detail projection + message count.
pub fn brahma_console_get(db: &Db, user_id: &str, chat_id: &str) -> Response {
    let chat = match verify_brahma_chat(db, chat_id, user_id) {
        Ok(c) => c,
        Err(r) => return r,
    };
    let count = match message_count(db, chat_id) {
        Ok(n) => n,
        Err(r) => return r,
    };
    Response::BrahmaConsole(json!({
        "chat": {
            "id": chat.get("id").cloned().unwrap_or(Value::Null),
            "title": chat.get("title").cloned().unwrap_or(Value::Null),
            "chatType": chat.get("chatType").cloned().unwrap_or(Value::Null),
            "consoleConnectionProfileId": chat.get("consoleConnectionProfileId").cloned().unwrap_or(Value::Null),
            "messageCount": count,
            "createdAt": chat.get("createdAt").cloned().unwrap_or(Value::Null),
            "updatedAt": chat.get("updatedAt").cloned().unwrap_or(Value::Null),
            "lastMessageAt": chat.get("lastMessageAt").cloned().unwrap_or(Value::Null),
        }
    }))
}

/// v4 `handleRename` (`[id]/route.ts:69`): PATCH (no action) — set `title` +
/// `isManuallyRenamed`. Body `{ chat: updated }`.
pub async fn brahma_console_rename(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    title: &Value,
) -> Response {
    if let Err(r) = verify_brahma_chat(db, chat_id, user_id) {
        return r;
    }
    // `renameSchema.parse` runs AFTER the verify (v4 `[id]/route.ts:78-80`).
    let title = match zod_min1_string(title) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let title = title.as_str();
    let cid = chat_id.to_string();
    let update = ChatUpdate {
        title: Some(title.to_string()),
        is_manually_renamed: Some(true),
        ..Default::default()
    };
    let cid_w = cid.clone();
    match db
        .write(move |ws| ws.main().chats().update(&cid_w, &update))
        .await
    {
        Ok(true) => {}
        Ok(false) => return internal("Failed to rename Brahma Console chat"),
        Err(e) => return internal(e),
    }
    match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(chat)) => Response::BrahmaConsole(json!({ "chat": chat })),
        _ => internal("Failed to rename Brahma Console chat"),
    }
}

/// v4 `handleSetModel` (`[id]/route.ts:100`): PATCH `?action=set-model` — switch
/// the console profile (the same conversation continues). Body `{ chat: updated }`.
/// `consoleConnectionProfileId` is not a `ChatUpdate` setter, so this writes it via
/// a raw single-column `UPDATE chats` (+ `updatedAt`, matching v4's repo update).
pub async fn brahma_console_set_model(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    connection_profile_id: &Value,
) -> Response {
    if let Err(r) = verify_brahma_chat(db, chat_id, user_id) {
        return r;
    }
    // `setModelSchema.parse` runs AFTER the verify (v4 `[id]/route.ts:109-111`).
    let connection_profile_id = match zod_uuid_string(connection_profile_id) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let connection_profile_id = connection_profile_id.as_str();
    // The profile must exist and belong to this user.
    let pid = connection_profile_id.to_string();
    match db.read_main(move |c| connection_profiles::find_by_id(c, &pid)) {
        Ok(Some(prof)) if s(&prof, "userId").as_deref() == Some(user_id) => {}
        Ok(_) => return bad_request("Connection profile not found"),
        Err(e) => return internal(e),
    }
    let cid = chat_id.to_string();
    let profile = connection_profile_id.to_string();
    let now = now_iso();
    let cid_r = cid.clone();
    let out = db
        .write(move |ws| {
            let n = ws.main().connection().execute(
                "UPDATE chats SET consoleConnectionProfileId = ?1, updatedAt = ?2 WHERE id = ?3",
                rusqlite::params![profile, now, cid],
            )?;
            Ok::<usize, crate::db::DbError>(n)
        })
        .await;
    match out {
        Ok(n) if n > 0 => {}
        Ok(_) => return internal("Failed to switch the Console model"),
        Err(e) => return internal(e),
    }
    match db.read_main(move |c| chats_read::find_by_id(c, &cid_r)) {
        Ok(Some(chat)) => Response::BrahmaConsole(json!({ "chat": chat })),
        _ => internal("Failed to switch the Console model"),
    }
}

/// v4 `handleDelete` (`[id]/route.ts:132`): delete the chat. Body
/// `{ message: 'Brahma Console chat deleted successfully' }`.
pub async fn brahma_console_delete(db: &Db, user_id: &str, chat_id: &str) -> Response {
    if let Err(r) = verify_brahma_chat(db, chat_id, user_id) {
        return r;
    }
    let cid = chat_id.to_string();
    match db.write(move |ws| ws.main().chats().delete(&cid)).await {
        Ok(true) => Response::BrahmaConsole(
            json!({ "message": "Brahma Console chat deleted successfully" }),
        ),
        Ok(false) => internal("Failed to delete Brahma Console chat"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// The messages GET endpoint (v4 [id]/messages/route.ts:55).
// ===========================================================================

/// v4 `handleGetMessages` (`[id]/messages/route.ts:55`): the chat's full message
/// list. Body `{ messages }`.
pub fn brahma_console_messages(db: &Db, user_id: &str, chat_id: &str) -> Response {
    if let Err(r) = verify_brahma_chat(db, chat_id, user_id) {
        return r;
    }
    let cid = chat_id.to_string();
    match db.read_main(move |c| chats_messages_read::get_messages(c, &cid)) {
        Ok(messages) => Response::BrahmaConsole(json!({ "messages": messages })),
        Err(e) => internal(e),
    }
}
