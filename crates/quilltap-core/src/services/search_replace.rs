//! The search & replace family (P4.9E3B) — v4's standalone
//! `POST /api/v1/search-replace` route (`app/api/v1/search-replace/route.ts`,
//! `execute` + `preview`) over
//! `lib/search-replace/search-replace-service.ts`, on the already-ported
//! leaves ([`crate::db::chats_search`] + the memories text ops).
//!
//! ## The search/replace case asymmetry (v4-faithful)
//!
//! The MEMORY count/find path matches case-INSENSITIVELY (v4's `$regex` with
//! the `i` flag → the ported `LIKE`), while `replaceInMemories` replaces
//! case-SENSITIVELY (`split(searchText).join(replaceText)`). A memory matching
//! only in the wrong case is counted by preview and found by execute, but its
//! replace is a no-op and it does NOT count toward `memoriesUpdated`. The
//! MESSAGE path is case-sensitive on both sides (`content.includes`). The
//! differential pins both.
//!
//! ## The embedding-regeneration branch is structurally unreachable — v4's own
//!
//! v4's execute calls `updateMemoryWithEmbedding(id, {content, summary})` with
//! the values `replaceInMemories` JUST persisted, so its `contentChanged`
//! comparison is always false and the embed never fires — the only observable
//! effect is a second content/summary write (another `updatedAt` mint). The
//! port reproduces exactly that; if the impossible branch is ever reached it
//! logs a named warning (v4's embed-failure path is warn-and-continue, so the
//! row outcome is identical).

use serde_json::{json, Value};

use crate::api::chat_outfits::is_zod_uuid;
use crate::api::types::{ErrorKind, Response};
use crate::db::chats_search::ChatSearchRepository;
use crate::db::memories::MemUpdate;
use crate::db::runtime::Db;
use crate::db::{chats_read, memories_read};

/// The two dispatched actions (v4's `withCollectionActionDispatch` arms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Preview,
    Execute,
}

// v4's missing-action (`route.ts:104`, `Action parameter required: execute or
// preview`) and unknown-action (`withActionDispatch`'s `Unknown action: …` +
// `availableActions`) arms have NO v5 counterpart: v5 carries no action string
// — the §1 verbs ARE the action selection, and no REST edge exists (the
// message-op precedent). The differential records both v4 arms and asserts
// their shape so upstream copy drift is caught.

/// The parsed scope (v4's `z.discriminatedUnion('type', …)`).
enum Scope {
    Chat(String),
    Character(String),
}

/// v4 `searchReplaceSchema.safeParse` — failure → `badRequest('Invalid
/// request')`. The §1 wire already types `searchText`/`replaceText`/the two
/// flags; what remains here is the scope union (uuid checks included) and the
/// `searchText` min-1.
fn parse_scope(scope: &Value, search_text: &str) -> Result<Scope, Response> {
    let invalid = || Response::error(ErrorKind::BadRequest, "Invalid request");
    if search_text.is_empty() {
        return Err(invalid());
    }
    let obj = scope.as_object().ok_or_else(invalid)?;
    match obj.get("type").and_then(Value::as_str) {
        Some("chat") => {
            let id = obj
                .get("chatId")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            if !is_zod_uuid(id) {
                return Err(invalid());
            }
            Ok(Scope::Chat(id.to_string()))
        }
        Some("character") => {
            let id = obj
                .get("characterId")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            if !is_zod_uuid(id) {
                return Err(invalid());
            }
            Ok(Scope::Character(id.to_string()))
        }
        _ => Err(invalid()),
    }
}

/// v4 `getChatIdsForScope` — chat: verify ownership (else `[]`); character:
/// every chat the character participates in, filtered to the user's.
fn chat_ids_for_scope(
    db: &Db,
    scope: &Scope,
    user_id: &str,
) -> Result<Vec<String>, crate::db::DbError> {
    match scope {
        Scope::Chat(chat_id) => {
            let cid = chat_id.clone();
            let chat = db.read_main(move |c| chats_read::find_by_id(c, &cid))?;
            match chat {
                Some(c) if c.get("userId").and_then(Value::as_str) == Some(user_id) => {
                    Ok(vec![chat_id.clone()])
                }
                _ => Ok(Vec::new()),
            }
        }
        Scope::Character(character_id) => {
            let chid = character_id.clone();
            let chats = db.read_main(move |c| chats_read::find_by_character_id(c, &chid))?;
            Ok(chats
                .iter()
                .filter(|c| c.get("userId").and_then(Value::as_str) == Some(user_id))
                .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
                .collect())
        }
    }
}

/// v4 `getMemoryQueryForScope` — `(characterId, chatId)`.
fn memory_query_for_scope(scope: &Scope) -> (Option<String>, Option<String>) {
    match scope {
        Scope::Chat(chat_id) => (None, Some(chat_id.clone())),
        Scope::Character(character_id) => (Some(character_id.clone()), None),
    }
}

/// The dispatched entry point: both v4 handlers share the parse; `Action`
/// selects `getSearchReplacePreview` or `executeSearchReplace`.
#[allow(clippy::too_many_arguments)]
pub async fn search_replace(
    db: &Db,
    user_id: &str,
    action: Action,
    scope: &Value,
    search_text: &str,
    replace_text: &str,
    include_messages: Option<bool>,
    include_memories: Option<bool>,
) -> Response {
    let scope = match parse_scope(scope, search_text) {
        Ok(s) => s,
        Err(r) => return r,
    };
    // v4 `.prefault(true)` — absent means true.
    let include_messages = include_messages.unwrap_or(true);
    let include_memories = include_memories.unwrap_or(true);

    match action {
        Action::Preview => preview(
            db,
            user_id,
            &scope,
            search_text,
            include_messages,
            include_memories,
        ),
        Action::Execute => {
            execute(
                db,
                user_id,
                &scope,
                search_text,
                replace_text,
                include_messages,
                include_memories,
            )
            .await
        }
    }
}

/// v4 `getSearchReplacePreview`. A thrown repo error rethrows → the middleware
/// 500 (`Internal server error`).
fn preview(
    db: &Db,
    user_id: &str,
    scope: &Scope,
    search_text: &str,
    include_messages: bool,
    include_memories: bool,
) -> Response {
    let inner = || -> Result<Response, crate::db::DbError> {
        let mut message_matches: i64 = 0;
        let mut memory_matches: i64 = 0;
        let mut affected_chats: i64 = 0;
        let mut affected_memories: i64 = 0;

        if include_messages {
            let chat_ids = chat_ids_for_scope(db, scope, user_id)?;
            for chat_id in &chat_ids {
                let cid = chat_id.clone();
                let st = search_text.to_string();
                let count = db.read_main(move |c| {
                    ChatSearchRepository::new(c).count_messages_with_text(&cid, &st)
                })?;
                if count > 0 {
                    message_matches += count;
                    affected_chats += 1;
                }
            }
        }

        if include_memories {
            let (character_id, chat_id) = memory_query_for_scope(scope);
            let st = search_text.to_string();
            let count = db.read_main(move |c| {
                memories_read::count_memories_with_text(
                    c,
                    character_id.as_deref(),
                    chat_id.as_deref(),
                    &st,
                )
            })?;
            memory_matches = count;
            affected_memories = count;
        }

        Ok(Response::ChatDialog(json!({
            "messageMatches": message_matches,
            "memoryMatches": memory_matches,
            "affectedChats": affected_chats,
            "affectedMemories": affected_memories,
        })))
    };
    match inner() {
        Ok(r) => r,
        Err(_) => Response::error(ErrorKind::Internal, "Internal server error"),
    }
}

/// v4 `executeSearchReplace`.
async fn execute(
    db: &Db,
    user_id: &str,
    scope: &Scope,
    search_text: &str,
    replace_text: &str,
    include_messages: bool,
    include_memories: bool,
) -> Response {
    let mut messages_updated: i64 = 0;
    let mut memories_updated: i64 = 0;
    let mut chats_affected: i64 = 0;
    let mut errors: Vec<String> = Vec::new();

    if include_messages {
        let chat_ids = match chat_ids_for_scope(db, scope, user_id) {
            Ok(ids) => ids,
            Err(_) => return Response::error(ErrorKind::Internal, "Internal server error"),
        };
        for chat_id in &chat_ids {
            let cid = chat_id.clone();
            let st = search_text.to_string();
            let rt = replace_text.to_string();
            let updated = db
                .write(move |w| {
                    ChatSearchRepository::new(w.main().connection())
                        .replace_in_messages(&cid, &st, &rt)
                })
                .await;
            match updated {
                Ok(n) if n > 0 => {
                    messages_updated += n;
                    chats_affected += 1;
                }
                Ok(_) => {}
                // v4 catches per chat, logs, and pushes the error string.
                Err(e) => errors.push(format!("Failed to update messages in chat {chat_id}: {e}")),
            }
        }
    }

    if include_memories {
        let (character_id, chat_id) = memory_query_for_scope(scope);
        let st = search_text.to_string();
        let to_update = db.read_main(move |c| {
            memories_read::find_memories_with_text(
                c,
                character_id.as_deref(),
                chat_id.as_deref(),
                &st,
            )
        });
        let to_update = match to_update {
            Ok(rows) => rows,
            Err(_) => return Response::error(ErrorKind::Internal, "Internal server error"),
        };
        if !to_update.is_empty() {
            let memory_ids: Vec<String> = to_update
                .iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
                .collect();
            let st = search_text.to_string();
            let rt = replace_text.to_string();
            let updated = db
                .write(move |w| {
                    let repo = w.main().memories();
                    let updated_ids = repo.replace_in_memories(&memory_ids, &st, &rt)?;
                    // v4's per-memory `updateMemoryWithEmbedding(characterId, id,
                    // {content, summary})` over the JUST-replaced values — the
                    // second content/summary write; the embed branch is
                    // structurally unreachable (module header).
                    // v4 wraps each in try/catch (best-effort — a failure logs
                    // and does NOT fail the request or shrink the count).
                    for id in &updated_ids {
                        let Ok(Some(row)) = memories_read::find_by_id(w.main().connection(), id)
                        else {
                            continue;
                        };
                        let character_id = row
                            .get("characterId")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let patch = MemUpdate {
                            content: row
                                .get("content")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            summary: row
                                .get("summary")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            ..Default::default()
                        };
                        if let Err(e) =
                            w.main()
                                .memories()
                                .update_for_character(&character_id, id, &patch)
                        {
                            tracing::warn!(memory_id = %id, error = %e,
                                "[SearchReplace] post-replace memory rewrite failed (best-effort)");
                        }
                    }
                    Ok(updated_ids.len() as i64)
                })
                .await;
            match updated {
                Ok(n) => memories_updated = n,
                Err(_) => return Response::error(ErrorKind::Internal, "Internal server error"),
            }
        }
    }

    Response::ChatDialog(json!({
        "messagesUpdated": messages_updated,
        "memoriesUpdated": memories_updated,
        "chatsAffected": chats_affected,
        "errors": errors,
    }))
}
