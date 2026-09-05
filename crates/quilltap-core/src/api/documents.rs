//! The Document Mode server dispatch handlers (P4.6w) — the chat-scoped +
//! standalone route-logic backfill the Document Mode SPA vertical consumes,
//! composed over the [`crate::documents`] operator-doc-actions core, the
//! `chat_documents` repo, and the Librarian announcement writers.
//!
//! Each handler is a differential port of a v4 handler (the oracles:
//! `app/api/v1/chats/[id]/actions/documents.ts` and
//! `app/api/v1/documents/route.ts`) returning a [`Response`] directly. The
//! chat-scoped surface tracks `chat_documents` rows + posts Librarian
//! announcements; the standalone surface uses [`STANDALONE_CHAT_ID`], creates no
//! Librarian message, and restricts scope to `document_store`/`general`.
//!
//! Writes run inside a single `Db::write` closure that hands both writer
//! connections to the core; the Librarian announcement is threaded OUT of the
//! sync closure and posted by the async caller (the `run_doc_edit` precedent).

use std::collections::HashSet;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::db::chat_documents::{ChatDocumentFull, ChatDocumentsRepository, OpenDocumentInput};
use crate::db::chats::{ChatUpdate, ChatsRepository};
use crate::db::runtime::Db;
use crate::db::{chats_read, DbError};
use crate::doc_edit::DocEditScope;
use crate::documents::{
    self, compute_rename_target, DeleteOutcome, DocError, DocumentAccessContext,
    MountRefreshScheduler, OpenDocumentFileParams, RenameTarget, MAX_RECENT_DOCUMENTS,
    STANDALONE_CHAT_ID,
};
use crate::services::librarian_notifications::{
    content_hidden_from_characters, document_hidden_from_characters,
    post_librarian_delete_announcement, post_librarian_open_announcement,
    post_librarian_rename_announcement, post_librarian_save_announcement, LibrarianActorOrigin,
    LibrarianDeleteAnnouncement, LibrarianOpenAnnouncement, LibrarianOpenKind,
    LibrarianRenameAnnouncement, LibrarianSaveAnnouncement, LibrarianScope,
};

use super::types::{ErrorKind, Response};

// ===========================================================================
// Shared response helpers (v4 `lib/api/responses.ts` message shapes)
// ===========================================================================

fn ok(body: Value) -> Response {
    Response::Document(body)
}
fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
fn server_error(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Internal, msg)
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn conflict(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Conflict, msg)
}
/// v4 `notFound(resource)` → `{ error: "<resource> not found" }`, 404.
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
/// v4 `errorResponse(msg, 404)` — a 404 with the literal message.
fn not_found_msg(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::NotFound, msg)
}

/// The loud "recognized but not yet available" refusal (unused this round — the
/// documents surface has no refusal arms — but kept for the deferral idiom).
#[allow(dead_code)]
pub fn not_available(action: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{action}' documents action is recognized but not yet available."),
    )
}

fn scope_from_str(s: &str) -> DocEditScope {
    match s {
        "document_store" => DocEditScope::DocumentStore,
        "general" => DocEditScope::General,
        _ => DocEditScope::Project,
    }
}
fn librarian_scope(scope: DocEditScope) -> LibrarianScope {
    match scope {
        DocEditScope::DocumentStore => LibrarianScope::DocumentStore,
        DocEditScope::Project => LibrarianScope::Project,
        DocEditScope::General => LibrarianScope::General,
    }
}

/// v4 `getChatContext` — the chat's project + the unique participant character
/// ids. `None` when the chat is missing.
fn chat_context(
    db: &Db,
    chat_id: &str,
    files_dir: Option<std::path::PathBuf>,
) -> Result<Option<DocumentAccessContext>, DbError> {
    let cid = chat_id.to_string();
    db.read_main(move |main| {
        let Some(chat) = chats_read::find_by_id(main, &cid)? else {
            return Ok(None);
        };
        Ok(Some(access_context_from_chat(&chat, files_dir)))
    })
}

fn access_context_from_chat(
    chat: &Value,
    files_dir: Option<std::path::PathBuf>,
) -> DocumentAccessContext {
    let project_id = chat
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut seen = HashSet::new();
    let mut character_ids = Vec::new();
    if let Some(parts) = chat.get("participants").and_then(Value::as_array) {
        for p in parts {
            if let Some(cid) = p.get("characterId").and_then(Value::as_str) {
                if seen.insert(cid.to_string()) {
                    character_ids.push(cid.to_string());
                }
            }
        }
    }
    DocumentAccessContext {
        project_id,
        character_ids,
        // P4.6bg S2: the host files dir (the engine passes `Some(<base>/files)` at
        // the dispatch call sites); `None` preserves the FsSeam refusal.
        files_dir,
    }
}

/// The `mount_index` writer connection or a plain internal error (the doc
/// surface requires it).
fn require_mount(w: &crate::db::runtime::WriterSet) -> Result<&rusqlite::Connection, DbError> {
    w.mount_index().map(|m| m.connection()).ok_or_else(|| {
        DbError::PartitionUnavailable(crate::write_partition::WriteDbTarget::MountIndex)
    })
}

/// A `chat_documents` row → the compact `{id, filePath, scope, mountPoint?,
/// displayTitle?}` document DTO (v4's shape). A NULL `mountPoint`/`displayTitle`
/// deserializes to `undefined` (the schema's `.nullable().optional()`) so v4's
/// `JSON.stringify` OMITS the key — reads omit null nullables (the P4.6p rule).
fn doc_dto(doc: &ChatDocumentFull) -> Value {
    document_object(
        Some(&doc.id),
        &doc.file_path,
        &doc.scope,
        doc.mount_point.as_deref(),
        doc.display_title.as_deref(),
    )
}

/// Build a document DTO, OMITTING a null `mountPoint`/`displayTitle` (the
/// read-omit-null-nullables rule). `id` is included only when present.
fn document_object(
    id: Option<&str>,
    file_path: &str,
    scope: &str,
    mount_point: Option<&str>,
    display_title: Option<&str>,
) -> Value {
    let mut o = Map::new();
    if let Some(id) = id {
        o.insert("id".into(), json!(id));
    }
    o.insert("filePath".into(), json!(file_path));
    o.insert("scope".into(), json!(scope));
    if let Some(mp) = mount_point {
        o.insert("mountPoint".into(), json!(mp));
    }
    if let Some(dt) = display_title {
        o.insert("displayTitle".into(), json!(dt));
    }
    Value::Object(o)
}

// ===========================================================================
// Chat-scoped reads
// ===========================================================================

/// v4 `handleActiveDocument` — the earliest-opened active doc (legacy pane).
pub fn chat_active_document(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let doc: Result<Option<ChatDocumentFull>, DbError> =
        db.read_main(move |main| ChatDocumentsRepository::new(main).find_active_for_chat(&cid));
    match doc {
        Ok(Some(doc)) => ok(json!({ "document": doc_dto(&doc) })),
        Ok(None) => ok(json!({ "document": Value::Null })),
        Err(_e) => server_error("Failed to get active document"),
    }
}

/// v4 `handleOpenDocuments` — the full open set, oldest-first.
pub fn chat_open_documents(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let docs: Result<Vec<ChatDocumentFull>, DbError> =
        db.read_main(move |main| ChatDocumentsRepository::new(main).find_open_for_chat_full(&cid));
    match docs {
        Ok(docs) => ok(json!({
            "documents": docs.iter().map(doc_dto).collect::<Vec<_>>(),
        })),
        Err(_e) => server_error("Failed to list open documents"),
    }
}

/// v4 `handleRecentDocuments` — this-chat docs first (updatedAt-desc), then the
/// global window, deduped by `(scope, mountPoint??'', filePath)` with current-chat
/// rows winning, capped at `MAX_RECENT_DOCUMENTS`.
pub fn chat_recent_documents(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let fetch_limit = std::cmp::max(MAX_RECENT_DOCUMENTS * 5, 50);
    let result: Result<Vec<Value>, DbError> = db.read_main(move |main| {
        let repo = ChatDocumentsRepository::new(main);
        let mut this_chat = repo.find_by_chat_id_full(&cid)?;
        // v4 sorts this-chat by updatedAt DESC.
        this_chat.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let global = repo.find_recent_across_chats(fetch_limit)?; // newest-first
        let other: Vec<ChatDocumentFull> =
            global.into_iter().filter(|d| d.chat_id != cid).collect();

        let mut seen: HashSet<String> = HashSet::new();
        let mut ordered: Vec<ChatDocumentFull> = Vec::new();
        for doc in this_chat.into_iter().chain(other) {
            let key = format!(
                "{} {} {}",
                doc.scope,
                doc.mount_point.as_deref().unwrap_or(""),
                doc.file_path
            );
            if !seen.insert(key) {
                continue;
            }
            ordered.push(doc);
            if ordered.len() >= MAX_RECENT_DOCUMENTS {
                break;
            }
        }
        Ok(ordered
            .into_iter()
            .map(|doc| recent_dto(&doc, &cid))
            .collect())
    });
    match result {
        Ok(documents) => ok(json!({ "documents": documents })),
        Err(_e) => server_error("Failed to get recent documents"),
    }
}

/// A recent-document row DTO for the chat-scoped picker (v4 `handleRecentDocuments`).
/// A NULL `mountPoint`/`displayTitle` is OMITTED (reads omit null nullables).
fn recent_dto(doc: &ChatDocumentFull, chat_id: &str) -> Value {
    let from_current = doc.chat_id == chat_id;
    let mut o = Map::new();
    o.insert("id".into(), json!(doc.id));
    o.insert("chatId".into(), json!(doc.chat_id));
    o.insert("filePath".into(), json!(doc.file_path));
    o.insert("scope".into(), json!(doc.scope));
    if let Some(mp) = &doc.mount_point {
        o.insert("mountPoint".into(), json!(mp));
    }
    if let Some(dt) = &doc.display_title {
        o.insert("displayTitle".into(), json!(dt));
    }
    // "Continue editing" only for the doc active in THIS chat.
    o.insert("isActive".into(), json!(doc.is_active && from_current));
    o.insert("fromCurrentChat".into(), json!(from_current));
    o.insert("updatedAt".into(), json!(doc.updated_at));
    Value::Object(o)
}

// ===========================================================================
// Chat-scoped writes (open / close / write / rename / delete)
// ===========================================================================

/// v4 `openDocumentSchema` (chat-scoped). `scope` default `'project'`, `mode`
/// default `'split'`; the Zod prefaults inlined.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatOpenBag {
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default = "default_project_scope")]
    scope: String,
    #[serde(default)]
    mount_point: Option<String>,
    #[serde(default = "default_split_mode")]
    mode: String,
    #[serde(default)]
    target_folder: Option<String>,
}
fn default_project_scope() -> String {
    "project".to_string()
}
fn default_split_mode() -> String {
    "split".to_string()
}

/// The success payload the open write closure yields (the announcement is posted
/// after the closure returns). The response document DTO is built from these
/// known fields — no re-read needed (`openDocument` returns the row id; the
/// scope/filePath/mountPoint/displayTitle are exactly what we opened with).
struct OpenOk {
    doc_id: String,
    content: String,
    mtime: Option<i64>,
    is_new: bool,
    effective_scope: DocEditScope,
    display_title: String,
    mount_point: Option<String>,
    file_path: String,
}

/// v4 `handleOpenDocument`. (Open never schedules a store refresh — v4's
/// `openDocumentFile` create branch writes a blank without the refresh — so no
/// `MountRefreshScheduler` is threaded here.)
pub async fn chat_document_open(
    db: &Db,
    chat_id: &str,
    bag: Value,
    files_dir: Option<std::path::PathBuf>,
) -> Response {
    let bag: ChatOpenBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid open-document body: {e}")),
    };
    let ctx = match chat_context(db, chat_id, files_dir) {
        Ok(Some(c)) => c,
        Ok(None) => return bad_request("Chat not found"),
        Err(e) => return internal(e),
    };
    let requested_scope = scope_from_str(&bag.scope);
    let effective_scope = if bag.file_path.is_none()
        && requested_scope == DocEditScope::Project
        && ctx.project_id.is_none()
    {
        DocEditScope::General
    } else {
        requested_scope
    };

    let cid = chat_id.to_string();
    let mode = bag.mode.clone();
    let file_path_in = bag.file_path.clone();
    let title_in = bag.title.clone();
    let mount_in = bag.mount_point.clone();
    let folder_in = bag.target_folder.clone();
    let outcome: Result<Result<OpenOk, DocError>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            let params = OpenDocumentFileParams {
                file_path: file_path_in.as_deref(),
                title: title_in.as_deref(),
                scope: effective_scope,
                mount_point: mount_in.as_deref(),
                target_folder: folder_in.as_deref(),
            };
            let opened = match documents::open_document_file(main, mount, &ctx, &params) {
                Ok(o) => o,
                Err(e) => return Ok(Err(e)),
            };
            let repo = ChatDocumentsRepository::new(main);
            let doc_id = repo.open_document(
                &cid,
                &OpenDocumentInput {
                    file_path: opened.file_path.clone(),
                    scope: effective_scope.as_str().to_string(),
                    mount_point: mount_in.clone(),
                    display_title: Some(opened.display_title.clone()),
                },
            )?;
            ChatsRepository::new(main).update(
                &cid,
                &ChatUpdate {
                    document_mode: Some(mode.clone()),
                    ..Default::default()
                },
            )?;
            Ok(Ok(OpenOk {
                doc_id,
                content: opened.content,
                mtime: opened.mtime,
                is_new: opened.is_new,
                effective_scope,
                display_title: opened.display_title,
                mount_point: mount_in.clone(),
                file_path: opened.file_path,
            }))
        })
        .await;

    let open = match outcome {
        Ok(Ok(o)) => o,
        Ok(Err(DocError::Missing(m))) => return not_found_msg(m),
        Ok(Err(e)) => {
            return server_error(format!("Failed to create blank document: {}", e.message()))
        }
        Err(e) => return internal(e),
    };

    // Post the open announcement (origin: opened-by-user; suppressed for a
    // character_read:false document).
    let hidden = content_hidden_from_characters(&open.content);
    let librarian_message = post_librarian_open_announcement(
        db,
        &LibrarianOpenAnnouncement {
            chat_id: chat_id.to_string(),
            display_title: open.display_title.clone(),
            file_path: open.file_path.clone(),
            scope: librarian_scope(open.effective_scope),
            mount_point: open.mount_point.clone(),
            is_new: open.is_new,
            origin: LibrarianOpenKind::ByUser,
            hidden_from_characters: hidden,
        },
    )
    .await;

    ok(json!({
        "document": document_object(
            Some(&open.doc_id),
            &open.file_path,
            open.effective_scope.as_str(),
            open.mount_point.as_deref(),
            Some(&open.display_title),
        ),
        "content": open.content,
        "mtime": open.mtime,
        "librarianMessage": librarian_message.unwrap_or(Value::Null),
    }))
}

/// v4 `handleCloseDocument` — close one open doc (by id, or the earliest-opened
/// active), then recompute `documentMode`.
pub async fn chat_document_close(
    db: &Db,
    chat_id: &str,
    chat_document_id: Option<String>,
) -> Response {
    let cid = chat_id.to_string();
    let result: Result<(), DbError> = db
        .write(move |w| {
            let main = w.main().connection();
            let repo = ChatDocumentsRepository::new(main);
            if let Some(id) = &chat_document_id {
                repo.close_document_by_id(&cid, id)?;
            } else {
                // v4 `closeDocument`: close the earliest-opened active row.
                if let Some(active) = repo.find_active_for_chat(&cid)? {
                    repo.close_document_by_id(&cid, &active.id)?;
                }
            }
            refresh_document_mode(main, &cid)?;
            Ok(())
        })
        .await;
    match result {
        Ok(()) => ok(json!({ "success": true })),
        Err(_e) => server_error("Failed to close document"),
    }
}

/// v4 `refreshDocumentMode` — recompute the coarse flag from the open set:
/// `'normal'` once the last doc closes; a `focus`/`split` chat keeps its mode
/// while docs remain (only bump to `'split'` from neither).
fn refresh_document_mode(main: &rusqlite::Connection, chat_id: &str) -> Result<(), DbError> {
    let repo = ChatDocumentsRepository::new(main);
    let open = repo.find_open_for_chat(chat_id)?;
    if open.is_empty() {
        ChatsRepository::new(main).update(
            chat_id,
            &ChatUpdate {
                document_mode: Some("normal".to_string()),
                ..Default::default()
            },
        )?;
        return Ok(());
    }
    let current = chats_read::find_by_id(main, chat_id)?.and_then(|c| {
        c.get("documentMode")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    if current.as_deref() != Some("split") && current.as_deref() != Some("focus") {
        ChatsRepository::new(main).update(
            chat_id,
            &ChatUpdate {
                document_mode: Some("split".to_string()),
                ..Default::default()
            },
        )?;
    }
    Ok(())
}

/// v4 `readDocumentSchema` / `writeDocumentSchema` shared fields.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadBag {
    file_path: String,
    #[serde(default = "default_project_scope")]
    scope: String,
    #[serde(default)]
    mount_point: Option<String>,
}

/// v4 `handleReadDocument`.
pub fn chat_document_read(
    db: &Db,
    chat_id: &str,
    bag: Value,
    files_dir: Option<std::path::PathBuf>,
) -> Response {
    let bag: ReadBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid read-document body: {e}")),
    };
    let ctx = match chat_context(db, chat_id, files_dir) {
        Ok(Some(c)) => c,
        Ok(None) => return bad_request("Chat not found"),
        Err(e) => return internal(e),
    };
    read_document(
        db,
        &ctx,
        scope_from_str(&bag.scope),
        &bag.file_path,
        bag.mount_point.as_deref(),
    )
}

/// The shared read body (chat + standalone). Resolve failure → 400; ENOENT →
/// 404; other → 500.
fn read_document(
    db: &Db,
    ctx: &DocumentAccessContext,
    scope: DocEditScope,
    file_path: &str,
    mount_point: Option<&str>,
) -> Response {
    let ctx = ctx.clone();
    let fp = file_path.to_string();
    let mp = mount_point.map(str::to_string);
    let result: Result<Result<Value, ReadFail>, DbError> = db.read_main(move |main| {
        db.read_mount_index(move |mount| {
            let resolved = match documents::resolve_operator_doc_path(
                main,
                mount,
                &ctx,
                scope,
                &fp,
                mp.as_deref(),
                None,
            ) {
                Ok(r) => r,
                Err(e) => {
                    return Ok(Err(ReadFail::Resolve(
                        documents_resolve_message(e),
                        fp.clone(),
                    )))
                }
            };
            match documents::read_route_document(mount, &resolved) {
                Ok((content, mtime)) => Ok(Ok(json!({ "content": content, "mtime": mtime }))),
                // NOT_FOUND (Missing) → v4's ENOENT branch (404).
                Err(DocError::Missing(_)) => Ok(Err(ReadFail::NotFound(fp.clone()))),
                Err(e) => Ok(Err(ReadFail::Server(e.message()))),
            }
        })
    });
    match result {
        Ok(Ok(body)) => ok(body),
        Ok(Err(ReadFail::Resolve(msg, fp))) => {
            bad_request(format!("Could not resolve {fp}: {msg}"))
        }
        Ok(Err(ReadFail::NotFound(fp))) => not_found_msg(format!("File not found: {fp}")),
        Ok(Err(ReadFail::Server(msg))) => server_error(format!("Failed to read document: {msg}")),
        Err(e) => internal(e),
    }
}

enum ReadFail {
    Resolve(String, String),
    NotFound(String),
    Server(String),
}

fn documents_resolve_message(e: crate::doc_edit::path_resolver::ResolveError) -> String {
    match e {
        crate::doc_edit::path_resolver::ResolveError::Path { message, .. } => message,
        crate::doc_edit::path_resolver::ResolveError::FsSeam => {
            "This location is served from the host filesystem, which is unavailable here."
                .to_string()
        }
    }
}

/// v4 `handleResolveDocument` — existence probe (never bytes). Any resolution
/// failure yields `{ exists:false, kind:'other' }`.
pub fn chat_document_resolve(
    db: &Db,
    chat_id: &str,
    bag: Value,
    files_dir: Option<std::path::PathBuf>,
) -> Response {
    let bag: ReadBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid resolve-document body: {e}")),
    };
    let ctx = match chat_context(db, chat_id, files_dir) {
        Ok(Some(c)) => c,
        Ok(None) => return bad_request("Chat not found"),
        Err(e) => return internal(e),
    };
    let scope = scope_from_str(&bag.scope);
    let fp = bag.file_path.clone();
    let mp = bag.mount_point.clone();
    // v4's `self` authority: document_store + mountPoint 'self' resolves only when
    // the chat has exactly one character participant (its vault).
    let is_self = scope == DocEditScope::DocumentStore
        && mp.as_deref().map(|m| m.to_lowercase()) == Some("self".to_string());
    let self_character_id = if is_self && ctx.character_ids.len() == 1 {
        Some(ctx.character_ids[0].clone())
    } else {
        None
    };
    let result: Result<Value, DbError> = db.read_main(move |main| {
        db.read_mount_index(move |mount| {
            let resolved = match documents::resolve_operator_doc_path(
                main,
                mount,
                &ctx,
                scope,
                &fp,
                mp.as_deref(),
                self_character_id.as_deref(),
            ) {
                Ok(r) => r,
                Err(_) => return Ok(json!({ "exists": false, "kind": "other" })),
            };
            let exists = documents::resolved_path_exists(mount, &resolved).unwrap_or(false);
            let kind = if exists {
                documents::classify_resolved_target(mount, &resolved).unwrap_or("other")
            } else {
                "other"
            };
            Ok(json!({ "exists": exists, "kind": kind }))
        })
    });
    match result {
        Ok(body) => ok(body),
        Err(_e) => ok(json!({ "exists": false, "kind": "other" })),
    }
}

/// v4 `writeDocumentSchema` (chat-scoped, + `content`/`mtime?`/`diffContent?`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatWriteBag {
    file_path: String,
    #[serde(default = "default_project_scope")]
    scope: String,
    #[serde(default)]
    mount_point: Option<String>,
    content: String,
    #[serde(default)]
    mtime: Option<i64>,
    #[serde(default)]
    diff_content: Option<String>,
}

/// v4 `handleWriteDocument`.
pub async fn chat_document_write(
    db: &Db,
    chat_id: &str,
    bag: Value,
    scheduler: Option<Arc<dyn MountRefreshScheduler>>,
    files_dir: Option<std::path::PathBuf>,
) -> Response {
    let bag: ChatWriteBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid write-document body: {e}")),
    };
    let ctx = match chat_context(db, chat_id, files_dir) {
        Ok(Some(c)) => c,
        Ok(None) => return bad_request("Chat not found"),
        Err(e) => return internal(e),
    };
    let scope = scope_from_str(&bag.scope);
    let write = write_document(
        db,
        &ctx,
        scope,
        &bag.file_path,
        bag.mount_point.as_deref(),
        &bag.content,
        bag.mtime,
        scheduler,
    )
    .await;
    let mtime = match write {
        Ok(m) => m,
        Err(DocError::MtimeMismatch(_)) => {
            return conflict("Document changed elsewhere. Reload it and try again.")
        }
        Err(e) => return server_error(format!("Failed to write document: {}", e.message())),
    };

    let librarian_message = match &bag.diff_content {
        Some(diff) => {
            let hidden = content_hidden_from_characters(&bag.content);
            post_librarian_save_announcement(
                db,
                &LibrarianSaveAnnouncement {
                    chat_id: chat_id.to_string(),
                    diff_content: diff.clone(),
                    hidden_from_characters: hidden,
                },
            )
            .await
        }
        None => None,
    };

    ok(json!({
        "success": true,
        "mtime": mtime,
        "librarianMessage": librarian_message.unwrap_or(Value::Null),
    }))
}

/// The shared write body (chat + standalone). Returns the fresh mtime or a
/// [`DocError`] the caller maps.
#[allow(clippy::too_many_arguments)]
async fn write_document(
    db: &Db,
    ctx: &DocumentAccessContext,
    scope: DocEditScope,
    file_path: &str,
    mount_point: Option<&str>,
    content: &str,
    expected_mtime: Option<i64>,
    scheduler: Option<Arc<dyn MountRefreshScheduler>>,
) -> Result<i64, DocError> {
    let ctx = ctx.clone();
    let fp = file_path.to_string();
    let mp = mount_point.map(str::to_string);
    let content = content.to_string();
    let outcome: Result<Result<i64, DocError>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            let sched = scheduler
                .as_ref()
                .map(|a| &**a as &dyn MountRefreshScheduler);
            Ok(documents::write_document_file(
                main,
                mount,
                &ctx,
                scope,
                &fp,
                mp.as_deref(),
                &content,
                expected_mtime,
                sched,
            ))
        })
        .await;
    match outcome {
        Ok(inner) => inner,
        Err(e) => Err(DocError::Store(e.to_string())),
    }
}

/// v4 `renameDocumentSchema`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatRenameBag {
    new_title: String,
    #[serde(default)]
    chat_document_id: Option<String>,
}

/// v4 `handleRenameDocument`.
pub async fn chat_document_rename(
    db: &Db,
    chat_id: &str,
    bag: Value,
    scheduler: Option<Arc<dyn MountRefreshScheduler>>,
    files_dir: Option<std::path::PathBuf>,
) -> Response {
    let bag: ChatRenameBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid rename-document body: {e}")),
    };
    if bag.new_title.is_empty() {
        return bad_request("Invalid rename-document body: newTitle must be at least 1 character");
    }
    // Resolve the target row (named or the earliest-opened active).
    let cid = chat_id.to_string();
    let target_id = bag.chat_document_id.clone();
    let doc: Result<Option<ChatDocumentFull>, DbError> =
        db.read_main(move |main| resolve_target_document(main, &cid, target_id.as_deref()));
    let doc = match doc {
        Ok(Some(d)) => d,
        Ok(None) => return bad_request("No active document to rename"),
        Err(e) => return internal(e),
    };

    let target = compute_rename_target(&doc.file_path, &bag.new_title);
    let (new_file_path, new_display_title) = match target {
        RenameTarget::Ok {
            new_file_path,
            new_display_title,
        } => (new_file_path, new_display_title),
        RenameTarget::Err { reason } => return bad_request(reason),
    };

    if new_file_path == doc.file_path {
        return ok(json!({ "document": doc_dto(&doc) }));
    }

    let ctx = match chat_context(db, chat_id, files_dir) {
        Ok(Some(c)) => c,
        Ok(None) => return bad_request("Chat not found"),
        Err(e) => return internal(e),
    };
    let doc_scope = scope_from_str(&doc.scope);

    // Capture read-policy BEFORE the rename moves the link (resolve old → hidden).
    let hidden = {
        let ctx2 = ctx.clone();
        let fp = doc.file_path.clone();
        let mp = doc.mount_point.clone();
        let resolved_old: Result<Result<(Option<String>, String), String>, DbError> =
            db.read_main(move |main| {
                db.read_mount_index(move |mount| {
                    match documents::resolve_operator_doc_path(
                        main,
                        mount,
                        &ctx2,
                        doc_scope,
                        &fp,
                        mp.as_deref(),
                        None,
                    ) {
                        Ok(r) => Ok(Ok((r.mount_point_id.clone(), r.relative_path.clone()))),
                        Err(e) => Ok(Err(documents_resolve_message(e))),
                    }
                })
            });
        match resolved_old {
            Ok(Ok((mp_id, rel))) => {
                document_hidden_from_characters(db, mp_id.as_deref(), Some(&rel)).await
            }
            Ok(Err(msg)) => return bad_request(format!("Could not resolve path: {msg}")),
            Err(e) => return internal(e),
        }
    };

    // The rename + row update + recents sweep, in one write closure.
    let ctx_move = ctx.clone();
    let old_fp = doc.file_path.clone();
    let new_fp = new_file_path.clone();
    let new_title2 = new_display_title.clone();
    let mp2 = doc.mount_point.clone();
    let scope_str = doc.scope.clone();
    let doc_id = doc.id.clone();
    let move_outcome: Result<Result<(), DocError>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            let sched = scheduler
                .as_ref()
                .map(|a| &**a as &dyn MountRefreshScheduler);
            if let Err(e) = documents::rename_document_file(
                main,
                mount,
                &ctx_move,
                doc_scope,
                mp2.as_deref(),
                &old_fp,
                &new_fp,
                sched,
            ) {
                return Ok(Err(e));
            }
            let repo = ChatDocumentsRepository::new(main);
            repo.update(
                &doc_id,
                &crate::db::chat_documents::CdUpdate {
                    file_path: Some(new_fp.clone()),
                    display_title: Some(new_title2.clone()),
                    updated_at: crate::clock::now_iso(),
                    ..Default::default()
                },
            )?;
            // Best-effort recents sweep (the rename already succeeded on disk).
            let _ = repo.rename_file_path_in_store(
                &scope_str,
                mp2.as_deref(),
                &old_fp,
                &new_fp,
                &new_title2,
            );
            Ok(Ok(()))
        })
        .await;
    match move_outcome {
        Ok(Ok(())) => {}
        Ok(Err(DocError::Conflict(_))) => return conflict("A file already exists at that name."),
        Ok(Err(DocError::Unsupported(m))) => return bad_request(m),
        Ok(Err(e)) => return server_error(format!("Failed to rename document: {}", e.message())),
        Err(e) => return internal(e),
    }

    let old_display_title = if doc.display_title.as_deref().unwrap_or("").is_empty() {
        documents_basename(&doc.file_path)
    } else {
        doc.display_title.clone().unwrap_or_default()
    };
    let librarian_message = post_librarian_rename_announcement(
        db,
        &LibrarianRenameAnnouncement {
            chat_id: chat_id.to_string(),
            old_display_title,
            new_display_title: new_display_title.clone(),
            old_file_path: doc.file_path.clone(),
            new_file_path: new_file_path.clone(),
            scope: librarian_scope(doc_scope),
            mount_point: doc.mount_point.clone(),
            hidden_from_characters: hidden,
        },
    )
    .await;

    ok(json!({
        "document": document_object(
            Some(&doc.id),
            &new_file_path,
            &doc.scope,
            doc.mount_point.as_deref(),
            Some(&new_display_title),
        ),
        "librarianMessage": librarian_message.unwrap_or(Value::Null),
    }))
}

/// v4 `resolveTargetDocument` — the named row (verified to belong to the chat) or
/// the earliest-opened active row.
fn resolve_target_document(
    main: &rusqlite::Connection,
    chat_id: &str,
    chat_document_id: Option<&str>,
) -> Result<Option<ChatDocumentFull>, DbError> {
    let repo = ChatDocumentsRepository::new(main);
    if let Some(id) = chat_document_id {
        let doc = repo.find_by_id(id)?;
        return Ok(doc.filter(|d| d.chat_id == chat_id));
    }
    repo.find_active_for_chat(chat_id)
}

/// v4 `handleDeleteDocument`.
pub async fn chat_document_delete(
    db: &Db,
    chat_id: &str,
    chat_document_id: Option<String>,
    files_dir: Option<std::path::PathBuf>,
) -> Response {
    let cid = chat_id.to_string();
    let target_id = chat_document_id.clone();
    let doc: Result<Option<ChatDocumentFull>, DbError> =
        db.read_main(move |main| resolve_target_document(main, &cid, target_id.as_deref()));
    let doc = match doc {
        Ok(Some(d)) => d,
        Ok(None) => return bad_request("No active document to delete"),
        Err(e) => return internal(e),
    };

    let ctx = match chat_context(db, chat_id, files_dir) {
        Ok(Some(c)) => c,
        Ok(None) => return bad_request("Chat not found"),
        Err(e) => return internal(e),
    };
    let doc_scope = scope_from_str(&doc.scope);

    // Resolve to capture the read-policy BEFORE the delete removes the link.
    let (mp_id, rel) = {
        let ctx2 = ctx.clone();
        let fp = doc.file_path.clone();
        let mp = doc.mount_point.clone();
        let resolved: Result<Result<(Option<String>, String), String>, DbError> =
            db.read_main(move |main| {
                db.read_mount_index(move |mount| {
                    match documents::resolve_operator_doc_path(
                        main,
                        mount,
                        &ctx2,
                        doc_scope,
                        &fp,
                        mp.as_deref(),
                        None,
                    ) {
                        Ok(r) => Ok(Ok((r.mount_point_id.clone(), r.relative_path.clone()))),
                        Err(e) => Ok(Err(documents_resolve_message(e))),
                    }
                })
            });
        match resolved {
            Ok(Ok(pair)) => pair,
            Ok(Err(msg)) => return bad_request(format!("Could not resolve path: {msg}")),
            Err(e) => return internal(e),
        }
    };
    let hidden = document_hidden_from_characters(db, mp_id.as_deref(), Some(&rel)).await;

    // Delete the underlying file, deactivate the row, recompute documentMode.
    let ctx_del = ctx.clone();
    let fp = doc.file_path.clone();
    let mp = doc.mount_point.clone();
    let doc_id = doc.id.clone();
    let cid = chat_id.to_string();
    let del_outcome: Result<Result<DeleteOutcome, DocError>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            let outcome = match documents::delete_document_file(
                main,
                mount,
                &ctx_del,
                doc_scope,
                mp.as_deref(),
                &fp,
            ) {
                Ok(o) => o,
                Err(e) => return Ok(Err(e)),
            };
            if outcome == DeleteOutcome::Deleted {
                let repo = ChatDocumentsRepository::new(main);
                repo.close_document_by_id(&cid, &doc_id)?;
                refresh_document_mode(main, &cid)?;
            }
            Ok(Ok(outcome))
        })
        .await;
    match del_outcome {
        Ok(Ok(DeleteOutcome::Deleted)) => {}
        // v4 `0506517d3` correction (c): `notFound('File')` renders "File not
        // found". The pre-fix `notFound('File not found')` doubled the suffix.
        Ok(Ok(DeleteOutcome::NotFound)) => return not_found("File"),
        Ok(Ok(DeleteOutcome::NotAFile)) => {
            return bad_request(format!("Path is not a file: {}", doc.file_path))
        }
        Ok(Err(e)) => return server_error(format!("Failed to delete document: {}", e.message())),
        Err(e) => return internal(e),
    }

    let display_title = if doc.display_title.as_deref().unwrap_or("").is_empty() {
        documents_basename(&doc.file_path)
    } else {
        doc.display_title.clone().unwrap_or_default()
    };
    let librarian_message = post_librarian_delete_announcement(
        db,
        &LibrarianDeleteAnnouncement {
            chat_id: chat_id.to_string(),
            display_title,
            file_path: doc.file_path.clone(),
            scope: librarian_scope(doc_scope),
            mount_point: doc.mount_point.clone(),
            origin: LibrarianActorOrigin::ByUser,
            hidden_from_characters: hidden,
        },
    )
    .await;

    ok(json!({
        "success": true,
        "librarianMessage": librarian_message.unwrap_or(Value::Null),
    }))
}

fn documents_basename(p: &str) -> String {
    p.rsplit('/').next().unwrap_or(p).to_string()
}

// ===========================================================================
// Accessible stores (both modes)
// ===========================================================================

/// v4 `handleAccessibleStores`.
pub fn chat_accessible_stores(db: &Db, chat_id: &str, all: bool) -> Response {
    let cid = chat_id.to_string();
    let result: Result<Option<Value>, DbError> = db.read_main(move |main| {
        db.read_mount_index(move |mount| {
            let Some(chat) = chats_read::find_by_id(main, &cid)? else {
                return Ok(None);
            };
            let body = accessible_stores_body(main, mount, &chat, all)?;
            Ok(Some(body))
        })
    });
    match result {
        Ok(Some(body)) => ok(body),
        Ok(None) => not_found("Chat"),
        Err(_e) => server_error("Failed to resolve accessible document stores"),
    }
}

fn accessible_stores_body(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    chat: &Value,
    all: bool,
) -> Result<Value, DbError> {
    use crate::db::doc_mount_points::DocMountPointsRepository;
    let mp_repo = DocMountPointsRepository::new(mount);

    // The project-official mount is surfaced separately (left-column button).
    let mut seen: HashSet<String> = HashSet::new();
    let mut project_library: Value = Value::Null;
    if let Some(project_id) = chat.get("projectId").and_then(Value::as_str) {
        if let Some(official_id) =
            crate::db::projects::find_official_mount_point_id_raw(main, project_id)?.flatten()
        {
            seen.insert(official_id.clone());
            if let Some(mp) = mp_repo.find_full_json_by_id(&official_id)? {
                if mp.get("enabled").and_then(Value::as_bool) == Some(true) {
                    project_library = json!({
                        "mountPointId": mp.get("id"),
                        "name": mp.get("name"),
                        "mountType": mp.get("mountType"),
                    });
                }
            }
        }
    }

    // Group stores reachable from this chat (union across CHARACTER participants).
    let mut group_mount_ids: HashSet<String> = HashSet::new();
    if let Some(parts) = chat.get("participants").and_then(Value::as_array) {
        for p in parts {
            if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                continue;
            }
            if p.get("status").and_then(Value::as_str) == Some("removed") {
                continue;
            }
            let Some(cid) = p.get("characterId").and_then(Value::as_str) else {
                continue;
            };
            let ids = crate::db::tiered_mount_pool::resolve_group_mount_point_ids_for_character(
                main, mount, cid,
            );
            for id in ids {
                group_mount_ids.insert(id);
            }
        }
    }

    let stores: Vec<Value> = if all {
        // Look-everywhere: every enabled store (holding back the project mount).
        documents::list_all_enabled_stores(main, mount, &mut seen, &group_mount_ids)?
            .into_iter()
            .map(|s| serde_json::to_value(s).unwrap_or(Value::Null))
            .collect()
    } else {
        chat_reachable_stores(main, mount, chat, &mut seen, &group_mount_ids)?
    };

    Ok(json!({ "stores": stores, "projectLibrary": project_library }))
}

/// The default (chat-reach) accessible-store set: participant vaults, group
/// stores, project-linked stores, then Quilltap General.
fn chat_reachable_stores(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    chat: &Value,
    seen: &mut HashSet<String>,
    group_mount_ids: &HashSet<String>,
) -> Result<Vec<Value>, DbError> {
    use crate::db::characters_read;
    use crate::db::doc_mount_points::DocMountPointsRepository;
    let mp_repo = DocMountPointsRepository::new(mount);
    let mut stores: Vec<Value> = Vec::new();

    // 1. Participant character vaults.
    if let Some(parts) = chat.get("participants").and_then(Value::as_array) {
        for p in parts {
            if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                continue;
            }
            if p.get("status").and_then(Value::as_str) == Some("removed") {
                continue;
            }
            let Some(cid) = p.get("characterId").and_then(Value::as_str) else {
                continue;
            };
            // getCharacterVaultStore → the character's characterDocumentMountPointId.
            let vault_id = characters_read::find_by_id_raw(main, cid)?.and_then(|c| {
                c.get("characterDocumentMountPointId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
            let Some(vault_id) = vault_id else { continue };
            if seen.contains(&vault_id) {
                continue;
            }
            let Some(mp) = mp_repo.find_full_json_by_id(&vault_id)? else {
                continue;
            };
            let character = characters_read::find_by_id_raw(main, cid)?;
            let label = character
                .as_ref()
                .and_then(|c| c.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    mp.get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                });
            stores.push(store_entry(&mp, &label, "character", Some(cid), false));
            seen.insert(vault_id);
        }
    }

    // 2. Group stores (official + linked of every group a participant belongs to).
    for id in group_mount_ids {
        if seen.contains(id) {
            continue;
        }
        let Some(mp) = mp_repo.find_full_json_by_id(id)? else {
            continue;
        };
        if mp.get("enabled").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let name = mp
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        stores.push(store_entry(&mp, &name, "document-store", None, true));
        seen.insert(id.clone());
    }

    // 3. Document stores linked to the chat's project.
    if let Some(project_id) = chat.get("projectId").and_then(Value::as_str) {
        let links = crate::db::project_doc_mount_links::ProjectDocMountLinksRepository::new(mount)
            .find_by_project_id(project_id)?;
        for mount_point_id in links {
            if seen.contains(&mount_point_id) {
                continue;
            }
            let Some(mp) = mp_repo.find_full_json_by_id(&mount_point_id)? else {
                continue;
            };
            let name = mp
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            stores.push(store_entry(&mp, &name, "document-store", None, false));
            seen.insert(mount_point_id);
        }
    }

    // 4. Quilltap General — always accessible.
    if let Some(general_id) = crate::db::instance_settings::get_general_mount_point_id(main)? {
        if !seen.contains(&general_id) {
            if let Some(mp) = mp_repo.find_full_json_by_id(&general_id)? {
                if mp.get("enabled").and_then(Value::as_bool) == Some(true) {
                    let name = mp
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    stores.push(store_entry(&mp, &name, "document-store", None, false));
                    seen.insert(general_id);
                }
            }
        }
    }

    Ok(stores)
}

/// Build an `AccessibleStoreOption` JSON from a full mount + the derived fields.
fn store_entry(
    mp: &Value,
    label: &str,
    kind: &str,
    character_id: Option<&str>,
    is_group_store: bool,
) -> Value {
    let mut obj = Map::new();
    obj.insert(
        "mountPointId".into(),
        mp.get("id").cloned().unwrap_or(Value::Null),
    );
    obj.insert(
        "name".into(),
        mp.get("name").cloned().unwrap_or(Value::Null),
    );
    obj.insert("label".into(), json!(label));
    obj.insert("kind".into(), json!(kind));
    obj.insert(
        "mountType".into(),
        mp.get("mountType").cloned().unwrap_or(Value::Null),
    );
    obj.insert(
        "storeType".into(),
        mp.get("storeType").cloned().unwrap_or(Value::Null),
    );
    if let Some(cid) = character_id {
        obj.insert("characterId".into(), json!(cid));
    }
    if is_group_store {
        obj.insert("isGroupStore".into(), json!(true));
    }
    Value::Object(obj)
}

// ===========================================================================
// Standalone surface (no chat rows in the response, no Librarian)
// ===========================================================================

/// v4 standalone `handleAccessibleStores` — always look-everywhere, no
/// projectLibrary.
pub fn document_stores(db: &Db) -> Response {
    let result: Result<Vec<Value>, DbError> = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let mut seen = HashSet::new();
            let empty = HashSet::new();
            Ok(
                documents::list_all_enabled_stores(main, mount, &mut seen, &empty)?
                    .into_iter()
                    .map(|s| serde_json::to_value(s).unwrap_or(Value::Null))
                    .collect(),
            )
        })
    });
    match result {
        Ok(stores) => ok(json!({ "stores": stores, "projectLibrary": Value::Null })),
        Err(_e) => server_error("Failed to resolve accessible document stores"),
    }
}

/// v4 standalone `handleRecentDocuments` — cross-chat recents, project-scope rows
/// filtered out.
pub fn documents_recent(db: &Db) -> Response {
    let fetch_limit = std::cmp::max(MAX_RECENT_DOCUMENTS * 5, 50);
    let result: Result<Vec<Value>, DbError> = db.read_main(move |main| {
        let global = ChatDocumentsRepository::new(main).find_recent_across_chats(fetch_limit)?;
        let mut seen: HashSet<String> = HashSet::new();
        let mut ordered: Vec<Value> = Vec::new();
        for doc in global {
            if doc.scope == "project" {
                continue;
            }
            let key = format!(
                "{} {} {}",
                doc.scope,
                doc.mount_point.as_deref().unwrap_or(""),
                doc.file_path
            );
            if !seen.insert(key) {
                continue;
            }
            let mut o = Map::new();
            o.insert("id".into(), json!(doc.id));
            o.insert("chatId".into(), json!(doc.chat_id));
            o.insert("filePath".into(), json!(doc.file_path));
            o.insert("scope".into(), json!(doc.scope));
            if let Some(mp) = &doc.mount_point {
                o.insert("mountPoint".into(), json!(mp));
            }
            if let Some(dt) = &doc.display_title {
                o.insert("displayTitle".into(), json!(dt));
            }
            o.insert("isActive".into(), json!(false));
            o.insert("fromCurrentChat".into(), json!(false));
            o.insert("updatedAt".into(), json!(doc.updated_at));
            ordered.push(Value::Object(o));
            if ordered.len() >= MAX_RECENT_DOCUMENTS {
                break;
            }
        }
        Ok(ordered)
    });
    match result {
        Ok(documents) => ok(json!({ "documents": documents })),
        Err(_e) => server_error("Failed to get recent documents"),
    }
}

/// v4 standalone `openDocumentSchema` — scope enum `['document_store','general']`,
/// default `'general'` (no `project`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StandaloneOpenBag {
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default = "default_general_scope")]
    scope: String,
    #[serde(default)]
    mount_point: Option<String>,
    #[serde(default)]
    target_folder: Option<String>,
}
fn default_general_scope() -> String {
    "general".to_string()
}
fn standalone_scope(s: &str) -> Result<DocEditScope, ()> {
    match s {
        "document_store" => Ok(DocEditScope::DocumentStore),
        "general" => Ok(DocEditScope::General),
        _ => Err(()),
    }
}

/// The standalone open payload the write closure yields.
struct StandaloneOpenOk {
    file_path: String,
    display_title: String,
    content: String,
    mtime: Option<i64>,
    is_new: bool,
}

/// v4 standalone `handleOpenDocument` — records the open under STANDALONE_CHAT_ID
/// (best-effort), no Librarian.
pub async fn document_open(db: &Db, bag: Value, files_dir: Option<std::path::PathBuf>) -> Response {
    let bag: StandaloneOpenBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid open-document body: {e}")),
    };
    let Ok(scope) = standalone_scope(&bag.scope) else {
        return bad_request("Invalid open-document body: scope must be document_store or general");
    };

    let file_path_in = bag.file_path.clone();
    let title_in = bag.title.clone();
    let mount_in = bag.mount_point.clone();
    let folder_in = bag.target_folder.clone();
    let scope_str = scope.as_str().to_string();
    let outcome: Result<Result<StandaloneOpenOk, DocError>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            let ctx = DocumentAccessContext::standalone(files_dir);
            let params = OpenDocumentFileParams {
                file_path: file_path_in.as_deref(),
                title: title_in.as_deref(),
                scope,
                mount_point: mount_in.as_deref(),
                target_folder: folder_in.as_deref(),
            };
            let opened = match documents::open_document_file(main, mount, &ctx, &params) {
                Ok(o) => o,
                Err(e) => return Ok(Err(e)),
            };
            // Best-effort recents tracking under STANDALONE_CHAT_ID.
            let _ = ChatDocumentsRepository::new(main).open_document(
                STANDALONE_CHAT_ID,
                &OpenDocumentInput {
                    file_path: opened.file_path.clone(),
                    scope: scope_str.clone(),
                    mount_point: mount_in.clone(),
                    display_title: Some(opened.display_title.clone()),
                },
            );
            Ok(Ok(StandaloneOpenOk {
                file_path: opened.file_path,
                display_title: opened.display_title,
                content: opened.content,
                mtime: opened.mtime,
                is_new: opened.is_new,
            }))
        })
        .await;

    let StandaloneOpenOk {
        file_path,
        display_title,
        content,
        mtime,
        is_new,
    } = match outcome {
        Ok(Ok(v)) => v,
        Ok(Err(DocError::Missing(m))) => return not_found_msg(m),
        Ok(Err(e)) => return server_error(format!("Failed to open document: {}", e.message())),
        Err(e) => return internal(e),
    };
    ok(json!({
        "document": {
            "filePath": file_path,
            "scope": scope.as_str(),
            "mountPoint": bag.mount_point,
            "displayTitle": display_title,
        },
        "content": content,
        "mtime": mtime,
        "isNew": is_new,
    }))
}

/// v4 standalone `readDocumentSchema`/`writeDocumentSchema`/etc.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StandaloneReadBag {
    file_path: String,
    #[serde(default = "default_general_scope")]
    scope: String,
    #[serde(default)]
    mount_point: Option<String>,
}

/// v4 standalone `handleReadDocument`.
pub fn document_read(db: &Db, bag: Value, files_dir: Option<std::path::PathBuf>) -> Response {
    let bag: StandaloneReadBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid read-document body: {e}")),
    };
    let Ok(scope) = standalone_scope(&bag.scope) else {
        return bad_request("Invalid read-document body: scope must be document_store or general");
    };
    read_document(
        db,
        &DocumentAccessContext::standalone(files_dir),
        scope,
        &bag.file_path,
        bag.mount_point.as_deref(),
    )
}

/// v4 standalone `writeDocumentSchema`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StandaloneWriteBag {
    file_path: String,
    #[serde(default = "default_general_scope")]
    scope: String,
    #[serde(default)]
    mount_point: Option<String>,
    content: String,
    #[serde(default)]
    mtime: Option<i64>,
}

/// v4 standalone `handleWriteDocument` (mtime-checked, no Librarian).
pub async fn document_write(
    db: &Db,
    bag: Value,
    scheduler: Option<Arc<dyn MountRefreshScheduler>>,
    files_dir: Option<std::path::PathBuf>,
) -> Response {
    let bag: StandaloneWriteBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid write-document body: {e}")),
    };
    let Ok(scope) = standalone_scope(&bag.scope) else {
        return bad_request("Invalid write-document body: scope must be document_store or general");
    };
    let write = write_document(
        db,
        &DocumentAccessContext::standalone(files_dir),
        scope,
        &bag.file_path,
        bag.mount_point.as_deref(),
        &bag.content,
        bag.mtime,
        scheduler,
    )
    .await;
    match write {
        Ok(mtime) => ok(json!({ "success": true, "mtime": mtime })),
        Err(DocError::MtimeMismatch(_)) => {
            conflict("Document changed elsewhere. Reload it and try again.")
        }
        Err(e) => server_error(format!("Failed to write document: {}", e.message())),
    }
}

/// v4 standalone `renameDocumentSchema`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StandaloneRenameBag {
    file_path: String,
    #[serde(default = "default_general_scope")]
    scope: String,
    #[serde(default)]
    mount_point: Option<String>,
    new_title: String,
}

/// v4 standalone `handleRenameDocument`.
pub async fn document_rename(
    db: &Db,
    bag: Value,
    scheduler: Option<Arc<dyn MountRefreshScheduler>>,
    files_dir: Option<std::path::PathBuf>,
) -> Response {
    let bag: StandaloneRenameBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid rename-document body: {e}")),
    };
    if bag.new_title.is_empty() {
        return bad_request("Invalid rename-document body: newTitle must be at least 1 character");
    }
    let Ok(scope) = standalone_scope(&bag.scope) else {
        return bad_request(
            "Invalid rename-document body: scope must be document_store or general",
        );
    };
    let target = compute_rename_target(&bag.file_path, &bag.new_title);
    let (new_file_path, new_display_title) = match target {
        RenameTarget::Ok {
            new_file_path,
            new_display_title,
        } => (new_file_path, new_display_title),
        RenameTarget::Err { reason } => return bad_request(reason),
    };
    if new_file_path == bag.file_path {
        return ok(json!({
            "document": {
                "filePath": bag.file_path,
                "scope": bag.scope,
                "mountPoint": bag.mount_point,
                "displayTitle": new_display_title,
            }
        }));
    }

    let old_fp = bag.file_path.clone();
    let new_fp = new_file_path.clone();
    let new_title2 = new_display_title.clone();
    let mp = bag.mount_point.clone();
    let scope_str = bag.scope.clone();
    let move_outcome: Result<Result<(), DocError>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            let ctx = DocumentAccessContext::standalone(files_dir);
            let sched = scheduler
                .as_ref()
                .map(|a| &**a as &dyn MountRefreshScheduler);
            if let Err(e) = documents::rename_document_file(
                main,
                mount,
                &ctx,
                scope,
                mp.as_deref(),
                &old_fp,
                &new_fp,
                sched,
            ) {
                return Ok(Err(e));
            }
            // Best-effort recents sweep.
            let _ = ChatDocumentsRepository::new(main).rename_file_path_in_store(
                &scope_str,
                mp.as_deref(),
                &old_fp,
                &new_fp,
                &new_title2,
            );
            Ok(Ok(()))
        })
        .await;
    match move_outcome {
        Ok(Ok(())) => {}
        Ok(Err(DocError::Conflict(_))) => return conflict("A file already exists at that name."),
        Ok(Err(DocError::Unsupported(m))) => return bad_request(m),
        Ok(Err(e)) => return server_error(format!("Failed to rename document: {}", e.message())),
        Err(e) => return internal(e),
    }

    ok(json!({
        "document": {
            "filePath": new_file_path,
            "scope": bag.scope,
            "mountPoint": bag.mount_point,
            "displayTitle": new_display_title,
        }
    }))
}

/// v4 standalone `deleteDocumentSchema`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StandaloneDeleteBag {
    file_path: String,
    #[serde(default = "default_general_scope")]
    scope: String,
    #[serde(default)]
    mount_point: Option<String>,
}

/// v4 standalone `handleDeleteDocument`.
pub async fn document_delete(
    db: &Db,
    bag: Value,
    files_dir: Option<std::path::PathBuf>,
) -> Response {
    let bag: StandaloneDeleteBag = match serde_json::from_value(bag) {
        Ok(b) => b,
        Err(e) => return bad_request(format!("Invalid delete-document body: {e}")),
    };
    let Ok(scope) = standalone_scope(&bag.scope) else {
        return bad_request(
            "Invalid delete-document body: scope must be document_store or general",
        );
    };
    let fp = bag.file_path.clone();
    let mp = bag.mount_point.clone();
    let outcome: Result<Result<DeleteOutcome, DocError>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            let ctx = DocumentAccessContext::standalone(files_dir);
            Ok(documents::delete_document_file(
                main,
                mount,
                &ctx,
                scope,
                mp.as_deref(),
                &fp,
            ))
        })
        .await;
    match outcome {
        Ok(Ok(DeleteOutcome::Deleted)) => ok(json!({ "success": true })),
        Ok(Ok(DeleteOutcome::NotFound)) => not_found("File"),
        Ok(Ok(DeleteOutcome::NotAFile)) => {
            bad_request(format!("Path is not a file: {}", bag.file_path))
        }
        Ok(Err(e)) => server_error(format!("Failed to delete document: {}", e.message())),
        Err(e) => internal(e),
    }
}
