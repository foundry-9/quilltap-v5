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
use crate::doc_edit::path_resolver::ResolvedPath;
use crate::doc_edit::DocEditScope;
use crate::documents::core::{self as doc_core, Refresh, RenameOutcome, SweepStore};
use crate::documents::{
    self, compute_rename_target, DeleteOutcome, DocError, DocumentAccessContext,
    MountRefreshScheduler, OpenDocumentFileParams, OpenedDocumentFile, RenameTarget,
    MAX_RECENT_DOCUMENTS, STANDALONE_CHAT_ID,
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
// === P4.D380 === The operator HTTP layer (v4 `lib/documents/operator-doc-http.ts`)
// ===========================================================================
//
// v4's helpers map the document core's outcomes onto the statuses both routes
// share — a resolution failure is a 400, a missing file a 404, a stale mtime or
// a taken rename target a 409, a read-only write a 400, anything else a 500 —
// and log each failure under the route's SUBJECT (`document` + `{chatId}` on the
// chat route, `standalone document` alone). Every line here is emitted on the
// CALLER's thread, after the read / write closure hands its outcome back (the
// core's own lines run inside the closure, on the writer thread).

/// v4 `DocumentHttpLog` — how a route's lines name the file and what they carry.
#[derive(Clone)]
struct DocLog {
    subject: &'static str,
    chat_id: Option<String>,
}

impl DocLog {
    fn chat(chat_id: &str) -> Self {
        DocLog {
            subject: "document",
            chat_id: Some(chat_id.to_string()),
        }
    }
    fn standalone() -> Self {
        DocLog {
            subject: "standalone document",
            chat_id: None,
        }
    }
    /// v4 `capitalize(subject)`.
    fn capitalized(&self) -> String {
        let mut c = self.subject.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    }
}

/// The message a resolution failure carries (the resolver's, or the host-fs
/// seam's sentence).
fn resolve_failure_message(e: crate::doc_edit::path_resolver::ResolveError) -> String {
    documents_resolve_message(e)
}

/// v4 `resolveDocumentForRoute`'s failure arm: WARN `Failed to resolve
/// <subject> path for <verb>` `{...fields, filePath, scope, mountPoint,
/// error}` → 400 `Could not resolve <filePath>: <message>`.
fn resolve_failed(
    log: &DocLog,
    verb: &str,
    file_path: &str,
    scope: &str,
    mount_point: Option<&str>,
    message: &str,
) -> Response {
    tracing::warn!(
        target: "quilltap::documents",
        chatId = log.chat_id.as_deref(),
        filePath = file_path,
        scope = scope,
        mountPoint = mount_point,
        error = message,
        "Failed to resolve {} path for {}",
        log.subject,
        verb
    );
    bad_request(format!("Could not resolve {file_path}: {message}"))
}

/// Resolve a route's document with the operator override (a read closure).
fn resolve_for_route(
    db: &Db,
    ctx: &DocumentAccessContext,
    scope: DocEditScope,
    file_path: &str,
    mount_point: Option<&str>,
) -> Result<Result<ResolvedPath, String>, DbError> {
    let ctx = ctx.clone();
    let fp = file_path.to_string();
    let mp = mount_point.map(str::to_string);
    db.read_main(move |main| {
        db.read_mount_index(move |mount| {
            Ok(documents::resolve_operator_doc_path(
                main,
                mount,
                &ctx,
                scope,
                &fp,
                mp.as_deref(),
                None,
            )
            .map_err(resolve_failure_message))
        })
    })
}

/// The Node `error.code` a v5 message carries as its `E<CODE>:` prefix (v4's
/// read 500 logs `code`); `None` is v4's `undefined` (left out).
fn node_code_of(message: &str) -> Option<&str> {
    let (head, _) = message.split_once(':')?;
    (head.len() > 1 && head.starts_with('E') && head.chars().all(|c| c.is_ascii_uppercase()))
        .then_some(head)
}

/// v4 `openDocumentResponse`'s failure arm: a [`DocError::Missing`] is a 404
/// with its message; anything else logs ERROR `Failed to open <subject>`
/// `{...fields, filePath, scope, mountPoint, error}` → 500 `Failed to open
/// document: <message>`.
fn open_failed(
    log: &DocLog,
    file_path: Option<&str>,
    scope: DocEditScope,
    mount_point: Option<&str>,
    e: DocError,
) -> Response {
    if let DocError::Missing(m) = e {
        return not_found_msg(m);
    }
    let message = e.message();
    tracing::error!(
        target: "quilltap::documents",
        chatId = log.chat_id.as_deref(),
        filePath = file_path,
        scope = scope.as_str(),
        mountPoint = mount_point,
        error = %message,
        "Failed to open {}",
        log.subject
    );
    server_error(format!("Failed to open document: {message}"))
}

/// v4 `openedDocumentPayload` — `{document, content, mtime?, viewer?}` (the
/// `viewer` key only when present; `mtime` left out for a viewed file).
fn opened_document_payload(opened: &OpenedDocumentFile, document: Value) -> Map<String, Value> {
    let mut o = Map::new();
    o.insert("document".into(), document);
    o.insert("content".into(), json!(opened.content));
    if let Some(m) = opened.mtime {
        o.insert("mtime".into(), json!(m));
    }
    if let Some(v) = &opened.viewer {
        o.insert(
            "viewer".into(),
            serde_json::to_value(v).unwrap_or(Value::Null),
        );
    }
    o
}

/// v4 `writeDocumentResponse`'s failure arms.
fn write_failed(log: &DocLog, file_path: &str, scope: DocEditScope, e: DocError) -> Response {
    match e {
        DocError::ReadOnly(message) => {
            tracing::warn!(
                target: "quilltap::documents",
                chatId = log.chat_id.as_deref(),
                filePath = file_path,
                "Refused an editor write to a read-only {}",
                log.subject
            );
            bad_request(message)
        }
        DocError::MtimeMismatch(message) => {
            tracing::warn!(
                target: "quilltap::documents",
                chatId = log.chat_id.as_deref(),
                filePath = file_path,
                error = %message,
                "{} save conflict detected",
                log.capitalized()
            );
            conflict("Document changed elsewhere. Reload it and try again.")
        }
        other => {
            let message = other.message();
            tracing::error!(
                target: "quilltap::documents",
                chatId = log.chat_id.as_deref(),
                filePath = file_path,
                scope = scope.as_str(),
                error = %message,
                "Failed to write {}",
                log.subject
            );
            server_error(format!("Failed to write document: {message}"))
        }
    }
}

/// v4 `renameDocumentResponse`'s outcome / failure arms.
fn rename_failed(
    log: &DocLog,
    scope: DocEditScope,
    old_file_path: &str,
    new_file_path: &str,
    outcome: Result<RenameOutcome, DocError>,
) -> Option<Response> {
    match outcome {
        Ok(RenameOutcome::Moved) => None,
        Ok(RenameOutcome::DestinationTaken) => {
            Some(conflict("A file already exists at that name."))
        }
        Ok(RenameOutcome::SourceMissing) => {
            Some(not_found_msg(format!("File not found: {old_file_path}")))
        }
        Ok(RenameOutcome::SourceNotAFile) => {
            Some(bad_request(format!("Path is not a file: {old_file_path}")))
        }
        Err(DocError::Unsupported(m)) => Some(bad_request(m)),
        Err(e) => {
            let message = e.message();
            tracing::error!(
                target: "quilltap::documents",
                chatId = log.chat_id.as_deref(),
                from = old_file_path,
                to = new_file_path,
                scope = scope.as_str(),
                error = %message,
                "Failed to rename {}",
                log.subject
            );
            Some(server_error(format!(
                "Failed to rename document: {message}"
            )))
        }
    }
}

/// v4 `deleteDocumentResponse`'s outcome / failure arms.
fn delete_failed(
    log: &DocLog,
    file_path: &str,
    scope: DocEditScope,
    outcome: Result<DeleteOutcome, DocError>,
) -> Option<Response> {
    match outcome {
        Ok(DeleteOutcome::Deleted) => None,
        // v4 `notFound('File')` → "File not found".
        Ok(DeleteOutcome::NotFound) => Some(not_found("File")),
        Ok(DeleteOutcome::NotAFile) => {
            Some(bad_request(format!("Path is not a file: {file_path}")))
        }
        Err(e) => {
            let message = e.message();
            tracing::error!(
                target: "quilltap::documents",
                chatId = log.chat_id.as_deref(),
                filePath = file_path,
                scope = scope.as_str(),
                error = %message,
                "Failed to delete {}",
                log.subject
            );
            Some(server_error(format!(
                "Failed to delete document: {message}"
            )))
        }
    }
}

/// The scheduler as the core's [`Refresh`] cadence.
fn refresh_of(scheduler: &Option<Arc<dyn MountRefreshScheduler>>) -> Refresh<'_> {
    Refresh::Scheduled(
        scheduler
            .as_ref()
            .map(|a| &**a as &dyn MountRefreshScheduler),
    )
}
// === end P4.D380 ===

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
    opened: OpenedDocumentFile,
}

/// v4 `handleOpenDocument`. A blank create writes through the core's
/// `ensureDocument`, so it is followed by the store refresh (P4.D380, #137 /
/// #133): the scheduler is threaded here.
pub async fn chat_document_open(
    db: &Db,
    chat_id: &str,
    bag: Value,
    scheduler: Option<Arc<dyn MountRefreshScheduler>>,
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
    let effective_scope = if bag.file_path.as_deref().unwrap_or("").is_empty()
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
            let opened = match documents::open_document_file(
                main,
                mount,
                &ctx,
                &params,
                refresh_of(&scheduler),
            ) {
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
            Ok(Ok(OpenOk { doc_id, opened }))
        })
        .await;

    let OpenOk { doc_id, opened } = match outcome {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return open_failed(
                &DocLog::chat(chat_id),
                bag.file_path.as_deref(),
                effective_scope,
                bag.mount_point.as_deref(),
                e,
            )
        }
        Err(e) => return internal(e),
    };

    // Post the open announcement (origin: opened-by-user; suppressed for a
    // character_read:false document).
    let hidden = content_hidden_from_characters(&opened.content);
    let librarian_message = post_librarian_open_announcement(
        db,
        &LibrarianOpenAnnouncement {
            chat_id: chat_id.to_string(),
            display_title: opened.display_title.clone(),
            file_path: opened.file_path.clone(),
            scope: librarian_scope(effective_scope),
            mount_point: bag.mount_point.clone(),
            is_new: opened.is_new,
            origin: LibrarianOpenKind::ByUser,
            hidden_from_characters: hidden,
        },
    )
    .await;

    let mut body = opened_document_payload(
        &opened,
        document_object(
            Some(&doc_id),
            &opened.file_path,
            effective_scope.as_str(),
            bag.mount_point.as_deref(),
            Some(&opened.display_title),
        ),
    );
    body.insert(
        "librarianMessage".into(),
        librarian_message.unwrap_or(Value::Null),
    );
    ok(Value::Object(body))
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
        DocLog::chat(chat_id),
    )
}

/// The shared read body (chat + standalone) — v4 `readDocumentResponse`
/// (`operator-doc-http.ts`): resolve (verb `read`, a failure → WARN + 400); a
/// file Document Mode only views answers `{content: '', viewer}` (its bytes
/// are never read as text — C14); else the core's read with the one blob
/// fallback (#134) → `{content, mtime}`; `ENOENT` → 404 `File not found:
/// <filePath>`; anything else → ERROR `Failed to read <subject>` `{...fields,
/// filePath, scope, code, error}` → 500.
fn read_document(
    db: &Db,
    ctx: &DocumentAccessContext,
    scope: DocEditScope,
    file_path: &str,
    mount_point: Option<&str>,
    log: DocLog,
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
                Err(e) => return Ok(Err(ReadFail::Resolve(resolve_failure_message(e)))),
            };
            match doc_core::view_target_for(mount, &resolved) {
                Ok(Some(viewer)) => {
                    return Ok(Ok(json!({
                        "content": "",
                        "viewer": serde_json::to_value(viewer).unwrap_or(Value::Null),
                    })))
                }
                Ok(None) => {}
                Err(e) => return Ok(Err(ReadFail::Server(e.message()))),
            }
            match doc_core::read_document(mount, &resolved) {
                Ok(r) => Ok(Ok(json!({ "content": r.content, "mtime": r.mtime_ms }))),
                Err(e) if e.is_enoent() => Ok(Err(ReadFail::NotFound)),
                Err(e) => Ok(Err(ReadFail::Server(e.message()))),
            }
        })
    });
    match result {
        Ok(Ok(body)) => ok(body),
        Ok(Err(ReadFail::Resolve(msg))) => {
            resolve_failed(&log, "read", file_path, scope.as_str(), mount_point, &msg)
        }
        Ok(Err(ReadFail::NotFound)) => not_found_msg(format!("File not found: {file_path}")),
        Ok(Err(ReadFail::Server(msg))) => {
            tracing::error!(
                target: "quilltap::documents",
                chatId = log.chat_id.as_deref(),
                filePath = file_path,
                scope = scope.as_str(),
                code = node_code_of(&msg),
                error = %msg,
                "Failed to read {}",
                log.subject
            );
            server_error(format!("Failed to read document: {msg}"))
        }
        Err(e) => internal(e),
    }
}

enum ReadFail {
    Resolve(String),
    NotFound,
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
            // v4 `documentExists` (P4.D380 — R-4's absent class; any failure is
            // the route's catch, `{exists:false, kind:'other'}`).
            let exists = doc_core::document_exists(mount, &resolved).unwrap_or(false);
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
        Err(e) => return write_failed(&DocLog::chat(chat_id), &bag.file_path, scope, e),
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
            Ok(documents::write_document_file(
                main,
                mount,
                &ctx,
                scope,
                &fp,
                mp.as_deref(),
                &content,
                expected_mtime,
                refresh_of(&scheduler),
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
    let log = DocLog::chat(chat_id);

    // v4 resolves the OLD path first (verb `rename`) and passes it on, so it is
    // not resolved twice.
    let resolved_old = match resolve_for_route(
        db,
        &ctx,
        doc_scope,
        &doc.file_path,
        doc.mount_point.as_deref(),
    ) {
        Ok(Ok(r)) => r,
        Ok(Err(msg)) => {
            return resolve_failed(
                &log,
                "rename",
                &doc.file_path,
                doc_scope.as_str(),
                doc.mount_point.as_deref(),
                &msg,
            )
        }
        Err(e) => return internal(e),
    };
    // Capture the read-policy BEFORE the rename moves the link off the old path.
    let hidden = document_hidden_from_characters(
        db,
        resolved_old.mount_point_id.as_deref(),
        Some(&resolved_old.relative_path),
    )
    .await;

    // Resolve the target, move, update the active row, then sweep every other
    // row still at the old path — in one write closure.
    let ctx_move = ctx.clone();
    let old_fp = doc.file_path.clone();
    let new_fp = new_file_path.clone();
    let new_title2 = new_display_title.clone();
    let mp2 = doc.mount_point.clone();
    let scope_str = doc.scope.clone();
    let doc_id = doc.id.clone();
    let cid = chat_id.to_string();
    let move_outcome: Result<Result<Result<RenameOutcome, DocError>, String>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            let resolved_new = match documents::resolve_operator_doc_path(
                main,
                mount,
                &ctx_move,
                doc_scope,
                &new_fp,
                mp2.as_deref(),
                None,
            ) {
                Ok(r) => r,
                Err(e) => return Ok(Err(resolve_failure_message(e))),
            };
            let moved = doc_core::rename_document(
                main,
                mount,
                &resolved_old,
                &resolved_new,
                refresh_of(&scheduler),
            );
            if !matches!(moved, Ok(RenameOutcome::Moved)) {
                return Ok(Ok(moved));
            }
            ChatDocumentsRepository::new(main).update(
                &doc_id,
                &crate::db::chat_documents::CdUpdate {
                    file_path: Some(new_fp.clone()),
                    display_title: Some(new_title2.clone()),
                    updated_at: crate::clock::now_iso(),
                    ..Default::default()
                },
            )?;
            // v4 `30a3b4cb7`: every other chat's (and the standalone sentinel's)
            // row still at the old path follows, matched by the store it
            // reaches. The row above is already at the new path.
            doc_core::sweep_chat_documents_after_move(
                main,
                mount,
                SweepStore {
                    scope: &scope_str,
                    mount_point: mp2.as_deref().map(Some),
                    mount_point_id: resolved_old.mount_point_id.as_deref(),
                },
                &old_fp,
                &new_fp,
                Some(&new_title2),
                Some(&cid),
            );
            Ok(Ok(moved))
        })
        .await;
    match move_outcome {
        Ok(Ok(outcome)) => {
            if let Some(r) = rename_failed(&log, doc_scope, &doc.file_path, &new_file_path, outcome)
            {
                return r;
            }
        }
        Ok(Err(msg)) => {
            return resolve_failed(
                &log,
                "rename",
                &new_file_path,
                doc_scope.as_str(),
                doc.mount_point.as_deref(),
                &msg,
            )
        }
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
    let log = DocLog::chat(chat_id);

    let resolved = match resolve_for_route(
        db,
        &ctx,
        doc_scope,
        &doc.file_path,
        doc.mount_point.as_deref(),
    ) {
        Ok(Ok(r)) => r,
        Ok(Err(msg)) => {
            return resolve_failed(
                &log,
                "delete",
                &doc.file_path,
                doc_scope.as_str(),
                doc.mount_point.as_deref(),
                &msg,
            )
        }
        Err(e) => return internal(e),
    };
    // Capture the read-policy BEFORE the delete removes the link row.
    let hidden = document_hidden_from_characters(
        db,
        resolved.mount_point_id.as_deref(),
        Some(&resolved.relative_path),
    )
    .await;

    // Delete the underlying file, deactivate the row, recompute documentMode.
    let doc_id = doc.id.clone();
    let cid = chat_id.to_string();
    let del_outcome: Result<Result<DeleteOutcome, DocError>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            let outcome = match doc_core::delete_document(main, mount, &resolved) {
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
        Ok(outcome) => {
            if let Some(r) = delete_failed(&log, &doc.file_path, doc_scope, outcome) {
                return r;
            }
        }
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

    // === P4.D380 === (v4 `c17033f04`): the project's official mount and
    // Quilltap General are the left-column "Project library" and "General
    // library" buttons, so they are kept out of the accordions — General's id
    // goes into `seen` FIRST, then the project official.
    let mut seen: HashSet<String> = HashSet::new();
    let general_library = documents::resolve_general_library(main, mount)?;
    if let Some(g) = &general_library {
        seen.insert(g.mount_point_id.clone());
    }
    // === end P4.D380 ===
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

    // === P4.D376 ===
    // The chat's reach as a cast (v4 `0c47f4539`): every live character
    // participant's vault, the union of their groups' stores, the project's
    // stores and General — one walk (`resolve_cast_pool`). The group tier is
    // surfaced as its own "Group Files" bucket in both default and
    // look-everywhere modes. An archived participant, or another user's
    // character, contributes neither its vault nor its groups.
    //
    // `userId` is v4's session `user.id`; the route reached this chat through
    // the user's own repositories, so it is the chat's owner — read off the row
    // (P4.D376's lane-local call: no boundary edit).
    let cast_ids: Vec<String> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter(|p| p.get("type").and_then(Value::as_str) == Some("CHARACTER"))
                .filter(|p| p.get("status").and_then(Value::as_str) != Some("removed"))
                .filter_map(|p| p.get("characterId").and_then(Value::as_str))
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let user_id = chat
        .get("userId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let reach = crate::db::reach::resolve_cast_pool(
        main,
        mount,
        &crate::db::reach::CastPoolOptions {
            user_id: &user_id,
            project_id: chat.get("projectId").and_then(Value::as_str),
            character_ids: &cast_ids,
            group_ids: &[],
        },
    );
    let group_mount_ids: HashSet<String> =
        reach.pool.group_mount_point_ids.iter().cloned().collect();
    // === end P4.D376 ===

    let stores: Vec<Value> = if all {
        // Look-everywhere: every enabled store (holding back the project mount).
        documents::list_all_enabled_stores(main, mount, &mut seen, &group_mount_ids)?
            .into_iter()
            .map(|s| serde_json::to_value(s).unwrap_or(Value::Null))
            .collect()
    } else {
        // === P4.D376 === (the cast pool, in tier order — no `HashSet` walk)
        chat_reachable_stores(main, mount, &mut seen, &reach)?
        // === end P4.D376 ===
    };

    // === P4.D380 === v4's DEBUG (never ported before), its counts over the
    // final `stores`, then `{stores, projectLibrary, generalLibrary}`.
    let chat_id_field = chat.get("id").and_then(serde_json::Value::as_str);
    let count = |f: &dyn Fn(&Value) -> bool| stores.iter().filter(|s| f(s)).count() as u64;
    let store_type = |s: &Value| {
        s.get("storeType")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let is_group = |s: &Value| s.get("isGroupStore").and_then(Value::as_bool) == Some(true);
    let is_db = |s: &Value| s.get("mountType").and_then(Value::as_str) == Some("database");
    tracing::debug!(
        target: "quilltap::documents",
        chatId = chat_id_field,
        mode = if all { "all" } else { "chat" },
        total = stores.len() as u64,
        hasProjectLibrary = !project_library.is_null(),
        hasGeneralLibrary = general_library.is_some(),
        characterVaults = count(&|s| store_type(s).as_deref() == Some("character")),
        groupStores = count(&|s| is_group(s)),
        databaseStores = count(&|s| store_type(s).as_deref() == Some("documents") && is_db(s) && !is_group(s)),
        filesystemStores = count(&|s| store_type(s).as_deref() == Some("documents") && !is_db(s) && !is_group(s)),
        "Resolved accessible document stores for picker"
    );
    Ok(json!({
        "stores": stores,
        "projectLibrary": project_library,
        "generalLibrary": general_library.map(|g| serde_json::to_value(g).unwrap_or(Value::Null)),
    }))
    // === end P4.D380 ===
}

// === P4.D376 ===
/// The default (chat-reach) accessible-store set: participant vaults, group
/// stores, then project stores (Quilltap General is held back as the "General
/// library" button, never listed here) — v4's walk over
/// `resolveCastPool`'s answer (`0c47f4539`), nearest tier first. The vaults are
/// the pool's `castVaults` (a vault that is also a scoped-tier store is
/// surfaced in that tier instead), each labelled with the owner's name from v4's
/// HYDRATED `characters.findById` (the store's name when the row is gone); the
/// group stores walk the pool's ARRAY in tier order (v5 had iterated a
/// `HashSet` — a random order whenever a chat reached two group stores), and
/// the project stores are the pool's, the official store first.
fn chat_reachable_stores(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    seen: &mut HashSet<String>,
    reach: &crate::db::reach::CastPool,
) -> Result<Vec<Value>, DbError> {
    use crate::db::characters_read;
    use crate::db::doc_mount_points::DocMountPointsRepository;
    let mp_repo = DocMountPointsRepository::new(mount);
    let mut stores: Vec<Value> = Vec::new();

    // 1. Participant character vaults.
    for vault in &reach.cast_vaults {
        if seen.contains(&vault.mount_point_id) {
            continue;
        }
        let Some(mp) = mp_repo.find_full_json_by_id(&vault.mount_point_id)? else {
            continue;
        };
        // v4's hydrated `findById` — a broken vault overlay THROWS (the
        // route's 500), so the read propagates.
        let character = characters_read::find_by_id(main, mount, &vault.character_id)?;
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
        stores.push(store_entry(
            &mp,
            &label,
            "character",
            Some(&vault.character_id),
            false,
        ));
        seen.insert(vault.mount_point_id.clone());
    }

    // 2. Group stores — official + linked stores of every group a live
    //    character participant belongs to, in tier order.
    for id in &reach.pool.group_mount_point_ids {
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

    // 3. The chat's project's stores — the official store first, then every
    //    linked store.
    for mount_point_id in &reach.pool.project_mount_point_ids {
        if seen.contains(mount_point_id) {
            continue;
        }
        let Some(mp) = mp_repo.find_full_json_by_id(mount_point_id)? else {
            continue;
        };
        let name = mp
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        stores.push(store_entry(&mp, &name, "document-store", None, false));
        seen.insert(mount_point_id.clone());
    }
    // === end P4.D376 ===

    // No fourth step: v4's walk ends at the project's stores. Quilltap General
    // is the left-column "General library" — its id is already in `seen`
    // when enabled (`accessible_stores_body`), so a General step never fired.

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
    // === P4.D380 === (v4 `c17033f04`): Quilltap General is `generalLibrary`,
    // the "General library" button, and is held back from the accordions.
    let result: Result<Value, DbError> = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let general_library = documents::resolve_general_library(main, mount)?;
            let mut exclude: HashSet<String> = general_library
                .iter()
                .map(|g| g.mount_point_id.clone())
                .collect();
            let empty = HashSet::new();
            let stores: Vec<Value> =
                documents::list_all_enabled_stores(main, mount, &mut exclude, &empty)?
                    .into_iter()
                    .map(|s| serde_json::to_value(s).unwrap_or(Value::Null))
                    .collect();
            Ok(json!({
                "stores": stores,
                "projectLibrary": Value::Null,
                "generalLibrary": general_library.map(|g| serde_json::to_value(g).unwrap_or(Value::Null)),
            }))
        })
    });
    match result {
        Ok(body) => ok(body),
        Err(e) => {
            tracing::error!(
                target: "quilltap::documents",
                error = %crate::db::fallback::error_text(&e),
                "Failed to resolve stores for standalone document picker"
            );
            server_error("Failed to resolve accessible document stores")
        }
    }
    // === end P4.D380 ===
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

/// v4 standalone `handleOpenDocument` — records the open under STANDALONE_CHAT_ID
/// (best-effort), no Librarian. A blank create refreshes its store (P4.D380).
pub async fn document_open(
    db: &Db,
    bag: Value,
    scheduler: Option<Arc<dyn MountRefreshScheduler>>,
    files_dir: Option<std::path::PathBuf>,
) -> Response {
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
    let outcome: Result<Result<OpenedDocumentFile, DocError>, DbError> = db
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
            let opened = match documents::open_document_file(
                main,
                mount,
                &ctx,
                &params,
                refresh_of(&scheduler),
            ) {
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
            Ok(Ok(opened))
        })
        .await;

    let opened = match outcome {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            return open_failed(
                &DocLog::standalone(),
                bag.file_path.as_deref(),
                scope,
                bag.mount_point.as_deref(),
                e,
            )
        }
        Err(e) => return internal(e),
    };
    let mut body = opened_document_payload(
        &opened,
        json!({
            "filePath": opened.file_path,
            "scope": scope.as_str(),
            "mountPoint": bag.mount_point,
            "displayTitle": opened.display_title,
        }),
    );
    body.insert("isNew".into(), json!(opened.is_new));
    ok(Value::Object(body))
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
        DocLog::standalone(),
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
        Err(e) => write_failed(&DocLog::standalone(), &bag.file_path, scope, e),
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

    let log = DocLog::standalone();
    let ctx = DocumentAccessContext::standalone(files_dir);
    let resolved_old =
        match resolve_for_route(db, &ctx, scope, &bag.file_path, bag.mount_point.as_deref()) {
            Ok(Ok(r)) => r,
            Ok(Err(msg)) => {
                return resolve_failed(
                    &log,
                    "rename",
                    &bag.file_path,
                    scope.as_str(),
                    bag.mount_point.as_deref(),
                    &msg,
                )
            }
            Err(e) => return internal(e),
        };

    let old_fp = bag.file_path.clone();
    let new_fp = new_file_path.clone();
    let new_title2 = new_display_title.clone();
    let mp = bag.mount_point.clone();
    let scope_str = bag.scope.clone();
    let move_outcome: Result<Result<Result<RenameOutcome, DocError>, String>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            let resolved_new = match documents::resolve_operator_doc_path(
                main,
                mount,
                &ctx,
                scope,
                &new_fp,
                mp.as_deref(),
                None,
            ) {
                Ok(r) => r,
                Err(e) => return Ok(Err(resolve_failure_message(e))),
            };
            let moved = doc_core::rename_document(
                main,
                mount,
                &resolved_old,
                &resolved_new,
                refresh_of(&scheduler),
            );
            if matches!(moved, Ok(RenameOutcome::Moved)) {
                // Keep the recent-documents history pointing at the new name
                // (v4 `30a3b4cb7` — no `logFields` on this route).
                doc_core::sweep_chat_documents_after_move(
                    main,
                    mount,
                    SweepStore {
                        scope: &scope_str,
                        mount_point: mp.as_deref().map(Some),
                        mount_point_id: resolved_old.mount_point_id.as_deref(),
                    },
                    &old_fp,
                    &new_fp,
                    Some(&new_title2),
                    None,
                );
            }
            Ok(Ok(moved))
        })
        .await;
    match move_outcome {
        Ok(Ok(outcome)) => {
            if let Some(r) = rename_failed(&log, scope, &bag.file_path, &new_file_path, outcome) {
                return r;
            }
        }
        Ok(Err(msg)) => {
            return resolve_failed(
                &log,
                "rename",
                &new_file_path,
                scope.as_str(),
                bag.mount_point.as_deref(),
                &msg,
            )
        }
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
    let log = DocLog::standalone();
    let ctx = DocumentAccessContext::standalone(files_dir);
    let resolved =
        match resolve_for_route(db, &ctx, scope, &bag.file_path, bag.mount_point.as_deref()) {
            Ok(Ok(r)) => r,
            Ok(Err(msg)) => {
                return resolve_failed(
                    &log,
                    "delete",
                    &bag.file_path,
                    scope.as_str(),
                    bag.mount_point.as_deref(),
                    &msg,
                )
            }
            Err(e) => return internal(e),
        };
    let outcome: Result<Result<DeleteOutcome, DocError>, DbError> = db
        .write(move |w| {
            let mount = require_mount(w)?;
            let main = w.main().connection();
            Ok(doc_core::delete_document(main, mount, &resolved))
        })
        .await;
    match outcome {
        Ok(outcome) => match delete_failed(&log, &bag.file_path, scope, outcome) {
            Some(r) => r,
            None => ok(json!({ "success": true })),
        },
        Err(e) => internal(e),
    }
}

#[cfg(test)]
mod chat_reachable_stores_source_tests {
    /// P4.D380 H1: v4's `handleAccessibleStores` default walk
    /// (`chats/[id]/actions/documents.ts:241-310` at `624d0d25c`) has steps
    /// 1–3 only — Quilltap General is held back as the left-column "General
    /// library" (`accessible_stores_body` seeds `seen` with its id when it is
    /// enabled), so a fourth "Quilltap General" step in `chat_reachable_stores`
    /// could never fire. Pinned by source text: the walk reads no General
    /// pointer of its own.
    #[test]
    fn the_default_walk_has_no_quilltap_general_step() {
        let src = include_str!("documents.rs");
        let start = src
            .find("\nfn chat_reachable_stores(")
            .expect("chat_reachable_stores is in documents.rs");
        let body = &src[start..];
        let body = &body[..body.find("\n}\n").expect("the fn closes")];
        assert!(
            !body.contains(concat!("get_general_", "mount_point_id")),
            "chat_reachable_stores reads General's pointer again"
        );
        assert!(
            !body.contains(concat!("4. Quilltap ", "General")),
            "chat_reachable_stores grew a fourth step"
        );
    }
}
