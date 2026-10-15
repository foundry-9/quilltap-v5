//! The search & replace family (P4.9E3B) — v4's standalone
//! `POST /api/v1/search-replace` route (`app/api/v1/search-replace/route.ts`,
//! `execute` + `preview`) over
//! `lib/search-replace/search-replace-service.ts`, on the already-ported
//! leaves ([`crate::db::chats_search`] + the memories text ops).
//!
//! ## The search/replace case asymmetry (v4-faithful)
//!
//! The MEMORY count/find path matches case-INSENSITIVELY (v4's `$regex` with
//! the `i` flag → the ported `LIKE`), while the replace plan
//! ([`plan_memory_replacement`]) matches case-SENSITIVELY (`includes`, then
//! `split(searchText).join(replaceText)`). A memory matching only in the wrong
//! case is counted by preview and found by execute, but it plans nothing, is
//! skipped, and does NOT count toward `memoriesUpdated`. The
//! MESSAGE path is case-sensitive on both sides (`content.includes`). The
//! differential pins both.
//!
//! ## Every replaced memory goes through the edit chokepoint (bug 233)
//!
//! v4 `e0ac7493a` (P4.D298): "Each row goes through updateMemory, which embeds
//! the new text before it writes and writes text and vector together, then
//! invalidates the archive, publishes `memories` and re-mirrors a digest.
//! Rewriting the rows first and re-embedding afterwards compared the new text
//! with itself, so nothing was ever re-embedded (bug 233)." The plan
//! ([`plan_memory_replacement`]) carries ONLY the fields that changed; the
//! per-row [`update_memory`](crate::services::admission::update_memory) runs on
//! the CALLER thread, one await per row (v4's sequential `for` + `await` —
//! L-8), so its writes commit row by row and its lines are capturable. A row
//! whose new text cannot be embedded is left UNCHANGED and reported in
//! `errors`; `memoriesUpdated` counts the edits that landed.

use serde_json::{json, Value};

use crate::api::chat_outfits::is_zod_uuid;
use crate::api::types::{ErrorKind, Response};
use crate::db::chats_search::ChatSearchRepository;
use crate::db::runtime::Db;
use crate::db::{chats_read, memories_read};
use crate::model::embedding::EmbeddingProvider;
use crate::services::admission::{update_memory, MemoryEditPatch, UpdateMemoryResult};

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

impl Scope {
    /// The Zod OUTPUT object (schema key order) — what v4's `Executing
    /// search/replace` INFO logs as `scope`.
    fn to_json(&self) -> Value {
        match self {
            Scope::Chat(id) => json!({ "type": "chat", "chatId": id }),
            Scope::Character(id) => json!({ "type": "character", "characterId": id }),
        }
    }
}

/// v4 `planMemoryReplacement` (`search-replace-service.ts:25-41`): "The fields
/// of one memory a search and replace would rewrite: content, summary and
/// keywords. Null when the search text appears in none of them. Pure, so the
/// replacement is computed from the row as read and the write can go through
/// `updateMemory` with the old row as its baseline (bug 233)."
///
/// `text.split(s).join(r)` with a non-empty `s` is [`str::replace`] (left to
/// right, non-overlapping); `includes` is case-SENSITIVE, so a row the
/// case-insensitive find matched only in another case plans nothing.
pub fn plan_memory_replacement(
    content: &str,
    summary: &str,
    keywords: Option<&[String]>,
    search_text: &str,
    replace_text: &str,
) -> Option<MemoryEditPatch> {
    if search_text.is_empty() {
        return None;
    }
    let replace = |t: &str| t.replace(search_text, replace_text);
    let mut patch = MemoryEditPatch::default();
    if content.contains(search_text) {
        patch.content = Some(replace(content));
    }
    if summary.contains(search_text) {
        patch.summary = Some(replace(summary));
    }
    if let Some(keywords) = keywords {
        if keywords.iter().any(|k| k.contains(search_text)) {
            patch.keywords = Some(keywords.iter().map(|k| replace(k)).collect());
        }
    }
    (!patch.keys().is_empty()).then_some(patch)
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
/// selects `getSearchReplacePreview` or `executeSearchReplace`. `provider` is
/// the OPTIONAL memory embedding seam (L-2): `None` drives the memory arm's
/// edits with [`NoEmbeddingProvider`](crate::model::embedding::NoEmbeddingProvider),
/// so a text-changing replace leaves each memory unchanged with v4's per-row
/// error and a messages-only replace is untouched. `host_zone_fallback` is
/// `update_memory`'s memory-calendar host zone.
#[allow(clippy::too_many_arguments)]
pub async fn search_replace<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: Option<&P>,
    user_id: &str,
    host_zone_fallback: &str,
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
                provider,
                user_id,
                host_zone_fallback,
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

/// v4 `executeSearchReplace` (`search-replace-service.ts:155-261`).
#[allow(clippy::too_many_arguments)]
async fn execute<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: Option<&P>,
    user_id: &str,
    host_zone_fallback: &str,
    scope: &Scope,
    search_text: &str,
    replace_text: &str,
    include_messages: bool,
    include_memories: bool,
) -> Response {
    tracing::info!(
        scopeJson = %scope.to_json(),
        searchTextLength = crate::jsstr::utf16_len(search_text),
        replaceTextLength = crate::jsstr::utf16_len(replace_text),
        includeMessages = include_messages,
        includeMemories = include_memories,
        "Executing search/replace"
    );
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
                Err(e) => {
                    let error_msg = format!(
                        "Failed to update messages in chat {chat_id}: {}",
                        crate::db::fallback::error_text(&e)
                    );
                    tracing::error!("{error_msg}");
                    errors.push(error_msg);
                }
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
        for memory in &to_update {
            let str_of = |k: &str| memory.get(k).and_then(Value::as_str).unwrap_or_default();
            let (memory_id, memory_character) = (str_of("id"), str_of("characterId"));
            let keywords: Option<Vec<String>> =
                memory.get("keywords").and_then(Value::as_array).map(|a| {
                    a.iter()
                        .filter_map(|k| k.as_str().map(str::to_string))
                        .collect()
                });
            let Some(patch) = plan_memory_replacement(
                str_of("content"),
                str_of("summary"),
                keywords.as_deref(),
                search_text,
                replace_text,
            ) else {
                continue;
            };
            let fields = patch.keys();
            let outcome = match provider {
                Some(p) => {
                    update_memory(
                        db,
                        p,
                        memory_character,
                        memory_id,
                        patch,
                        user_id,
                        host_zone_fallback,
                    )
                    .await
                }
                None => {
                    update_memory(
                        db,
                        &crate::model::embedding::NoEmbeddingProvider,
                        memory_character,
                        memory_id,
                        patch,
                        user_id,
                        host_zone_fallback,
                    )
                    .await
                }
            };
            match outcome {
                Ok(UpdateMemoryResult::Ok { .. }) => {
                    memories_updated += 1;
                    tracing::debug!(
                        memoryId = memory_id,
                        characterId = memory_character,
                        fieldsJson = %serde_json::to_string(&fields).unwrap_or_default(),
                        "Replaced text in memory and re-embedded it"
                    );
                }
                Ok(UpdateMemoryResult::EmbeddingFailed { message }) => {
                    let error_msg = format!(
                        "Memory {memory_id} left unchanged: its new text could not be embedded ({message})"
                    );
                    tracing::warn!(
                        memoryId = memory_id,
                        characterId = memory_character,
                        "{error_msg}"
                    );
                    errors.push(error_msg);
                }
                Ok(other) => {
                    let reason = match other {
                        UpdateMemoryResult::NotFound => "not-found",
                        _ => "invalid-related",
                    };
                    tracing::debug!(
                        memoryId = memory_id,
                        reason = reason,
                        "Memory skipped during replace"
                    );
                }
                Err(e) => {
                    let error_msg = format!(
                        "Failed to update memory {memory_id}: {}",
                        crate::db::fallback::error_text(&e)
                    );
                    tracing::error!("{error_msg}");
                    errors.push(error_msg);
                }
            }
        }
    }

    tracing::info!(
        messagesUpdated = messages_updated,
        memoriesUpdated = memories_updated,
        chatsAffected = chats_affected,
        errorsJson = %serde_json::to_string(&errors).unwrap_or_default(),
        "Search/replace execution complete"
    );
    Response::ChatDialog(json!({
        "messagesUpdated": messages_updated,
        "memoriesUpdated": memories_updated,
        "chatsAffected": chats_affected,
        "errors": errors,
    }))
}
