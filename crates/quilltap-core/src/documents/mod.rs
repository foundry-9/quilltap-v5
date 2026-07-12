//! Operator Document Actions — the shared, chat-agnostic core (P4.6w).
//!
//! Ports v4 `lib/documents/operator-doc-actions.ts` (+ `lib/chat-documents/
//! constants.ts`): the mechanics behind Document Mode's operator actions — path
//! resolution with the operator override, existence probing, blank-document
//! naming, read/write with mtime conflict detection, rename/delete file
//! mechanics, and store listing. Two routes drive this core (both in
//! [`crate::api::documents`]): the chat-scoped surface (adds `chat_documents`
//! row tracking + Librarian announcements) and the chat-less standalone surface
//! (no rows, no Librarian).
//!
//! ## The host-filesystem seam
//!
//! v4 resolves `general` scope and filesystem/obsidian mounts to a real
//! `absolutePath` and reads/writes them with plain `fs`. The v5 path resolver
//! ([`crate::doc_edit::path_resolver`]) defers every such branch to the
//! Phase-4 host ([`ResolveError::FsSeam`]); a [`ResolvedPath`] is only ever
//! produced for a **database-backed** mount. So the operator-doc-actions core
//! resolves + operates on database stores faithfully, and any `general`/fs
//! target surfaces the seam as a loud [`DocError::Fs`] error. The differential
//! corpus is entirely database-backed (matching the whole doc-edit surface) —
//! the fs branches are a tracked deferral, never exercised by the diff.

use rusqlite::Connection;
use serde::Serialize;

use crate::db::database_store::{self, DbStoreErrorCode, ReadDoc, StoreError};
use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::DbError;
use crate::doc_edit::path_resolver::ResolveError;
use crate::doc_edit::path_resolver::{resolve_doc_edit_path, PathResolutionContext, ResolvedPath};
use crate::doc_edit::DocEditScope;
use crate::tools::doc_edit::shared::is_text_file;

// ===========================================================================
// Constants (v4 `lib/chat-documents/constants.ts`)
// ===========================================================================

/// v4 `MAX_RECENT_DOCUMENTS` — how many recent documents the picker holds/shows.
pub const MAX_RECENT_DOCUMENTS: usize = 10;

/// v4 `STANDALONE_CHAT_ID` — the reserved all-zeros `chatId` for standalone
/// (chat-less) Document Mode opens. A schema-valid UUID no real chat holds, so
/// the `(chatId, filePath, scope, mountPoint)` unique index still dedupes
/// reopens, but these rows never mistake for a document opened "in" a chat.
pub const STANDALONE_CHAT_ID: &str = "00000000-0000-0000-0000-000000000000";

// ===========================================================================
// Access context + the refresh seam + the typed errors
// ===========================================================================

/// Who is reaching for documents (v4 `DocumentAccessContext`): the chat route
/// fills this from the chat's project + character participants; the standalone
/// route passes an empty context and relies on the operator override.
#[derive(Debug, Clone, Default)]
pub struct DocumentAccessContext {
    pub project_id: Option<String>,
    pub character_ids: Vec<String>,
}

impl DocumentAccessContext {
    /// v4 `STANDALONE_ACCESS_CONTEXT` — nothing behind it (the chat-less caller).
    pub fn standalone() -> Self {
        DocumentAccessContext {
            project_id: None,
            character_ids: Vec::new(),
        }
    }
}

/// The document-store refresh seam (v4 `scheduleDocumentStoreRefresh`): a
/// fire-and-forget reindex + embeddings + stats pass after a `document_store`
/// write. Lane B ports the SEAM; the machinery is lane A's, wired in at
/// unification. Defaults to `None` on [`crate::api::engine::EngineAssembly`];
/// the write call sites log a loud skip when unwired (never a silent no-op).
pub trait MountRefreshScheduler: Send + Sync {
    /// Fire-and-forget: reindex + embeddings + stats for the scoped path.
    fn schedule_refresh(&self, mount_point_id: &str, path: Option<&str>);
}

/// A document operation outcome the route handlers map to the v4 responses.
#[derive(Debug)]
pub enum DocError {
    /// v4 `DocumentMissingError` — a named file could not be read (→ 404).
    Missing(String),
    /// v4 `DocumentConflictError` — the destination is taken (→ 409).
    Conflict(String),
    /// v4 `DatabaseStoreError` with code `UNSUPPORTED` (rename bad extension → 400).
    Unsupported(String),
    /// The mtime-mismatch concurrency conflict (→ 409). Message carries "mtime
    /// mismatch" for v4-parity detection.
    MtimeMismatch(String),
    /// A path-resolution failure (message is the resolver's).
    Resolve(String),
    /// The host-filesystem seam (`general` scope or an fs/obsidian mount) — a
    /// tracked deferral; loudly refused, never silently swallowed.
    Fs,
    /// Any other store / DB error.
    Store(String),
}

impl DocError {
    /// The message text (for the resolve/store arms the handlers log/surface).
    pub fn message(&self) -> String {
        match self {
            DocError::Missing(m)
            | DocError::Conflict(m)
            | DocError::Unsupported(m)
            | DocError::MtimeMismatch(m)
            | DocError::Resolve(m)
            | DocError::Store(m) => m.clone(),
            DocError::Fs => {
                "This location is served from the host filesystem, which is unavailable here."
                    .to_string()
            }
        }
    }
}

impl From<DbError> for DocError {
    fn from(e: DbError) -> Self {
        DocError::Store(e.to_string())
    }
}

/// Flatten a resolver error into a [`DocError`] (the `FsSeam` becomes the loud
/// host-fs deferral; a path error carries its message).
fn resolve_err(e: ResolveError) -> DocError {
    match e {
        ResolveError::FsSeam => DocError::Fs,
        ResolveError::Path { message, .. } => DocError::Resolve(message),
    }
}

/// Flatten a database-store error into a [`DocError`] preserving the code arm.
fn store_err(e: StoreError) -> DocError {
    match e {
        StoreError::Store(s) => match s.code {
            DbStoreErrorCode::Unsupported => DocError::Unsupported(s.message),
            DbStoreErrorCode::Conflict => DocError::Conflict(s.message),
            _ => DocError::Store(s.message),
        },
        StoreError::Db(d) => DocError::Store(d.to_string()),
    }
}

// ===========================================================================
// Resolution + existence + classification
// ===========================================================================

/// v4 `resolveOperatorDocPath` — resolve a Document Mode target with the
/// operator override (reaches any enabled store, including "look everywhere"
/// picks + the standalone surface). Character doc tools never get this override.
pub fn resolve_operator_doc_path(
    main: &Connection,
    mount: &Connection,
    ctx: &DocumentAccessContext,
    scope: DocEditScope,
    file_path: &str,
    mount_point: Option<&str>,
    character_id: Option<&str>,
) -> Result<ResolvedPath, ResolveError> {
    let context = PathResolutionContext {
        project_id: ctx.project_id.clone(),
        character_id: character_id.map(str::to_string),
        character_ids: ctx.character_ids.clone(),
        mount_point: mount_point.map(str::to_string),
        operator_override: true,
    };
    resolve_doc_edit_path(main, mount, scope, Some(file_path), &context)
}

/// v4 `resolvedPathExists` — does a resolved database target currently have a
/// file? `readDatabaseDocument` NOT_FOUND → `false`; **any other error bubbles**,
/// so a permission/DB failure is never mistaken for "absent" (which would
/// overwrite). A non-database resolved path is the host-fs seam (unreachable on
/// the corpus — the resolver errors first).
pub fn resolved_path_exists(mount: &Connection, resolved: &ResolvedPath) -> Result<bool, DocError> {
    if resolved.mount_type.as_deref() != Some("database") {
        return Err(DocError::Fs);
    }
    let mp = resolved.mount_point_id.as_deref().ok_or_else(|| {
        DocError::Store("Database-backed ResolvedPath is missing mountPointId".into())
    })?;
    match database_store::read_database_document(mount, mp, &resolved.relative_path) {
        Ok(_) => Ok(true),
        Err(StoreError::Store(s)) if s.code == DbStoreErrorCode::NotFound => Ok(false),
        Err(e) => Err(store_err(e)),
    }
}

/// The qtap-link gate classification (v4 `classifyResolvedTarget`): a resolved
/// target is a `document`, an `image`, or `other`.
pub fn classify_resolved_target(
    mount: &Connection,
    resolved: &ResolvedPath,
) -> Result<&'static str, DbError> {
    if is_text_file(&resolved.relative_path) {
        return Ok("document");
    }
    if mime_for_extension(&resolved.relative_path).starts_with("image/") {
        return Ok("image");
    }
    if resolved.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved.mount_point_id.as_deref() {
            let links = DocMountFileLinksRepository::new(mount);
            if let Some(link) = links.find_by_mount_point_and_path(mp, &resolved.relative_path)? {
                if link.file_type == "blob" {
                    let blobs = DocMountBlobsRepository::new(mount);
                    if let Some(blob) = blobs.find_by_file_id(&link.file_id)? {
                        if blob.stored_mime_type.to_lowercase().starts_with("image/") {
                            return Ok("image");
                        }
                    }
                }
            }
        }
    }
    Ok("other")
}

/// v4 `mimeForExtension` (`lib/mount-index/path-utils.ts`) — the extension→MIME
/// map the qtap classification + byte route key on. Distinct from
/// [`crate::mime::detect_mime_type`] (the scanner map): only png/jpg/jpeg/gif/
/// svg/webp resolve to `image/*` here. Ported verbatim for byte-exactness.
pub fn mime_for_extension(relative_path: &str) -> &'static str {
    let ext = node_extname(relative_path).to_ascii_lowercase();
    match ext.as_str() {
        ".md" | ".markdown" => "text/markdown; charset=utf-8",
        ".txt" => "text/plain; charset=utf-8",
        ".json" => "application/json; charset=utf-8",
        ".jsonl" | ".ndjson" => "application/jsonl; charset=utf-8",
        ".webp" => "image/webp",
        ".png" => "image/png",
        ".jpg" | ".jpeg" => "image/jpeg",
        ".gif" => "image/gif",
        ".svg" => "image/svg+xml",
        ".pdf" => "application/pdf",
        ".docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
    }
}

// ===========================================================================
// Blank-document naming (pure) + the picker
// ===========================================================================

/// v4 `pickUntitledDocumentPath` — pick an unused "Untitled Document.md" inside
/// `target_folder`, appending a counter on collision ("Untitled Document 2.md",
/// …); after 1000 attempts, a UUID fallback. Returns the relative file path and
/// the resolved path it lives at.
pub fn pick_untitled_document_path(
    main: &Connection,
    mount: &Connection,
    ctx: &DocumentAccessContext,
    scope: DocEditScope,
    mount_point: Option<&str>,
    target_folder: Option<&str>,
) -> Result<(String, ResolvedPath), DocError> {
    let folder = trim_slashes(target_folder.unwrap_or(""));
    let join = |name: &str| {
        if folder.is_empty() {
            name.to_string()
        } else {
            format!("{folder}/{name}")
        }
    };
    const MAX_ATTEMPTS: usize = 1000;
    for i in 1..=MAX_ATTEMPTS {
        let candidate = untitled_candidate(i);
        let file_path = join(&candidate);
        let resolved =
            resolve_operator_doc_path(main, mount, ctx, scope, &file_path, mount_point, None)
                .map_err(resolve_err)?;
        if !resolved_path_exists(mount, &resolved)? {
            return Ok((file_path, resolved));
        }
    }
    // Defensive UUID fallback.
    let file_path = join(&format!("Untitled Document {}.md", uuid::Uuid::new_v4()));
    let resolved =
        resolve_operator_doc_path(main, mount, ctx, scope, &file_path, mount_point, None)
            .map_err(resolve_err)?;
    Ok((file_path, resolved))
}

/// The i-th untitled candidate basename (v4: `i===1 ? 'Untitled Document.md' :
/// 'Untitled Document ${i}.md'`). Pure — the collision-counter shape.
fn untitled_candidate(i: usize) -> String {
    if i == 1 {
        "Untitled Document.md".to_string()
    } else {
        format!("Untitled Document {i}.md")
    }
}

/// v4 `(targetFolder ?? '').replace(/^\/+|\/+$/g, '')` — strip leading/trailing
/// slashes.
fn trim_slashes(s: &str) -> String {
    s.trim_matches('/').to_string()
}

// ===========================================================================
// Open / write / rename / delete
// ===========================================================================

/// The result of opening a document (v4 `OpenedDocumentFile`).
#[derive(Debug, Clone)]
pub struct OpenedDocumentFile {
    pub file_path: String,
    pub display_title: String,
    pub content: String,
    /// `None` only for a host-fs read (never on the corpus); database reads/writes
    /// always mint an mtime.
    pub mtime: Option<i64>,
    /// True when a new blank document was created (no `file_path` was given).
    pub is_new: bool,
}

/// Parameters for [`open_document_file`] (v4 `OpenDocumentFileParams`).
pub struct OpenDocumentFileParams<'a> {
    pub file_path: Option<&'a str>,
    pub title: Option<&'a str>,
    pub scope: DocEditScope,
    pub mount_point: Option<&'a str>,
    /// Folder for a NEW blank document; ignored when `file_path` is set.
    pub target_folder: Option<&'a str>,
}

/// v4 `openDocumentFile` — read an existing file, or pick an unused
/// "Untitled Document.md" and create it blank. The create branch WRITES, so this
/// runs inside a write closure. Throws [`DocError::Missing`] when a named file
/// can't be read (v4's catch-all → `DocumentMissingError`).
pub fn open_document_file(
    main: &Connection,
    mount: &Connection,
    ctx: &DocumentAccessContext,
    params: &OpenDocumentFileParams,
) -> Result<OpenedDocumentFile, DocError> {
    if let Some(file_path) = params.file_path {
        // v4 wraps the whole resolve+read in one try/catch → DocumentMissingError.
        let read: Result<ReadDoc, DocError> = (|| {
            let resolved = resolve_operator_doc_path(
                main,
                mount,
                ctx,
                params.scope,
                file_path,
                params.mount_point,
                None,
            )
            .map_err(resolve_err)?;
            read_database(mount, &resolved)
        })();
        let doc = read.map_err(|_| DocError::Missing(format!("File not found: {file_path}")))?;
        return Ok(OpenedDocumentFile {
            file_path: file_path.to_string(),
            display_title: display_title_for(params.title, file_path),
            content: doc.content,
            mtime: Some(doc.mtime_ms),
            is_new: false,
        });
    }

    let (file_path, resolved) = pick_untitled_document_path(
        main,
        mount,
        ctx,
        params.scope,
        params.mount_point,
        params.target_folder,
    )?;
    let mtime = write_database(mount, &resolved, "")?;
    let display_title = display_title_for(params.title, &file_path);
    Ok(OpenedDocumentFile {
        file_path,
        display_title,
        content: String::new(),
        mtime: Some(mtime),
        is_new: true,
    })
}

/// v4 `writeDocumentFile` — mtime-checked write, scheduling the store refresh for
/// `document_store` scope. Propagates the mtime-mismatch as
/// [`DocError::MtimeMismatch`] for the 409 mapping.
#[allow(clippy::too_many_arguments)]
pub fn write_document_file(
    main: &Connection,
    mount: &Connection,
    ctx: &DocumentAccessContext,
    scope: DocEditScope,
    file_path: &str,
    mount_point: Option<&str>,
    content: &str,
    expected_mtime: Option<i64>,
    scheduler: Option<&dyn MountRefreshScheduler>,
) -> Result<i64, DocError> {
    let resolved = resolve_operator_doc_path(main, mount, ctx, scope, file_path, mount_point, None)
        .map_err(resolve_err)?;
    let mtime = write_database_checked(mount, &resolved, content, expected_mtime)?;

    if scope == DocEditScope::DocumentStore {
        if let Some(mp) = resolved.mount_point_id.as_deref() {
            schedule_refresh(scheduler, mp, Some(&resolved.relative_path), file_path);
        }
    }
    Ok(mtime)
}

/// The rename-target computation result (v4 `RenameTarget`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameTarget {
    Ok {
        new_file_path: String,
        new_display_title: String,
    },
    Err {
        reason: String,
    },
}

/// v4 `computeRenameTarget` — turn a user-typed title into the rename target
/// path: the input is the new basename (directory preserved); a missing
/// extension inherits the old one ("backstory" → "backstory.md"). Path
/// separators / `.` / `..` are rejected — a rename within the directory, not a
/// move. **Pure.**
pub fn compute_rename_target(current_file_path: &str, new_title: &str) -> RenameTarget {
    let raw = crate::jsstr::js_trim(new_title);
    if raw.is_empty() {
        return RenameTarget::Err {
            reason: "Name cannot be empty".to_string(),
        };
    }
    if raw.contains('/') || raw.contains('\\') {
        return RenameTarget::Err {
            reason: "Name cannot contain path separators".to_string(),
        };
    }
    if raw == "." || raw == ".." || raw.split(['\\', '/']).any(|s| s == "..") {
        return RenameTarget::Err {
            reason: "Invalid name".to_string(),
        };
    }

    let old_ext = node_extname(current_file_path);
    let old_dir = node_dirname(current_file_path);
    let new_basename = if node_extname(raw).is_empty() {
        format!("{raw}{old_ext}")
    } else {
        raw.to_string()
    };
    let new_file_path = if old_dir == "." || old_dir.is_empty() {
        new_basename
    } else {
        format!("{}/{}", old_dir.replace('\\', "/"), new_basename)
    };
    let new_display_title = node_basename(&new_file_path).to_string();
    RenameTarget::Ok {
        new_file_path,
        new_display_title,
    }
}

/// v4 `renameDocumentFile` — move a document's underlying file to its rename
/// target. Database stores route through `moveDatabaseDocument` (CONFLICT →
/// [`DocError::Conflict`], UNSUPPORTED bubbles); the fs branch is the host-fs
/// seam. Schedules the store refresh for `document_store` scope.
#[allow(clippy::too_many_arguments)]
pub fn rename_document_file(
    main: &Connection,
    mount: &Connection,
    ctx: &DocumentAccessContext,
    scope: DocEditScope,
    mount_point: Option<&str>,
    old_file_path: &str,
    new_file_path: &str,
    scheduler: Option<&dyn MountRefreshScheduler>,
) -> Result<(), DocError> {
    let resolved_old =
        resolve_operator_doc_path(main, mount, ctx, scope, old_file_path, mount_point, None)
            .map_err(resolve_err)?;
    let resolved_new =
        resolve_operator_doc_path(main, mount, ctx, scope, new_file_path, mount_point, None)
            .map_err(resolve_err)?;

    if resolved_old.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved_old.mount_point_id.as_deref() {
            database_store::move_database_document(
                mount,
                mp,
                &resolved_old.relative_path,
                &resolved_new.relative_path,
            )
            .map_err(|e| match e {
                StoreError::Store(s) if s.code == DbStoreErrorCode::Conflict => {
                    DocError::Conflict("A file already exists at that name.".to_string())
                }
                other => store_err(other),
            })?;

            if scope == DocEditScope::DocumentStore {
                schedule_refresh(
                    scheduler,
                    mp,
                    Some(&resolved_new.relative_path),
                    new_file_path,
                );
            }
            return Ok(());
        }
    }

    // Non-database mount → host-fs seam (never on the corpus).
    Err(DocError::Fs)
}

/// The outcome of a delete (v4 `'deleted'|'not-found'|'not-a-file'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOutcome {
    Deleted,
    NotFound,
    NotAFile,
}

/// v4 `deleteDocumentFile` — delete a document's underlying file. Database stores
/// route through `deleteDatabaseDocument` (false → NotFound); the fs branch is
/// the host-fs seam.
pub fn delete_document_file(
    main: &Connection,
    mount: &Connection,
    ctx: &DocumentAccessContext,
    scope: DocEditScope,
    mount_point: Option<&str>,
    file_path: &str,
) -> Result<DeleteOutcome, DocError> {
    let resolved = resolve_operator_doc_path(main, mount, ctx, scope, file_path, mount_point, None)
        .map_err(resolve_err)?;

    if resolved.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved.mount_point_id.as_deref() {
            let deleted =
                database_store::delete_database_document(mount, mp, &resolved.relative_path)?;
            return Ok(if deleted {
                DeleteOutcome::Deleted
            } else {
                DeleteOutcome::NotFound
            });
        }
    }
    // Non-database mount → host-fs seam.
    Err(DocError::Fs)
}

// ===========================================================================
// Store listing (the picker's "look everywhere")
// ===========================================================================

/// An accessible store (v4 `AccessibleStoreOption`). `kind`: `character` (a
/// vault) or `document-store`. Nullable fields are omitted (route reads omit
/// null nullables — the P4.6p rule).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessibleStoreOption {
    pub mount_point_id: String,
    pub name: String,
    pub label: String,
    /// `'character' | 'document-store'`.
    pub kind: String,
    pub mount_type: String,
    pub store_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_group_store: Option<bool>,
}

/// v4 `listAllEnabledStores` — every enabled store (the "look everywhere"
/// listing). Character vaults are labelled by their owning character's name.
/// `exclude` holds back stores surfaced elsewhere (the project-official mount);
/// `group_mount_ids` tags reachable group stores. The mount enumeration is
/// rowid order (v4 `findEnabled()`), and `exclude` is mutated in-loop to dedupe.
pub fn list_all_enabled_stores(
    main: &Connection,
    mount: &Connection,
    exclude: &mut std::collections::HashSet<String>,
    group_mount_ids: &std::collections::HashSet<String>,
) -> Result<Vec<AccessibleStoreOption>, DbError> {
    let mp_repo = DocMountPointsRepository::new(mount);
    let mounts = mp_repo.find_enabled_for_docedit()?;

    // Reverse-map character vaults to their owning character for labelling
    // (v4 `characters.findAll()` → `characterDocumentMountPointId`). The slim
    // `characterDocumentMountPointId` is an overlay-invariant MAIN column, so the
    // raw read suffices (the vault-unavailable divergence is out of the corpus).
    let characters = crate::db::characters_read::find_all_raw(main)?;
    let mut vault_owner: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for c in &characters {
        if let Some(vault_id) = c
            .get("characterDocumentMountPointId")
            .and_then(|v| v.as_str())
        {
            let id = c
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = c
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            vault_owner
                .entry(vault_id.to_string())
                .or_insert((id, name));
        }
    }

    let mut stores: Vec<AccessibleStoreOption> = Vec::new();
    for mp in mounts {
        if exclude.contains(&mp.id) {
            continue;
        }
        exclude.insert(mp.id.clone());
        let store_type = mp_repo
            .find_store_type_by_id(&mp.id)?
            .unwrap_or_else(|| "documents".to_string());
        let is_character = store_type == "character";
        let owner = if is_character {
            vault_owner.get(&mp.id).cloned()
        } else {
            None
        };
        let label = owner
            .as_ref()
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| mp.name.clone());
        stores.push(AccessibleStoreOption {
            mount_point_id: mp.id.clone(),
            name: mp.name.clone(),
            label,
            kind: if is_character {
                "character".to_string()
            } else {
                "document-store".to_string()
            },
            mount_type: mp.mount_type.clone(),
            store_type,
            character_id: owner.map(|(id, _)| id),
            is_group_store: if group_mount_ids.contains(&mp.id) {
                Some(true)
            } else {
                None
            },
        });
    }
    Ok(stores)
}

// ===========================================================================
// Internal helpers
// ===========================================================================

/// Read a resolved target for a route handler (the `read-document` surface):
/// `(content, mtime_ms)`, mapping the database NOT_FOUND to
/// [`DocError::Missing`] (v4's `ENOENT` → 404) and any other store error to
/// [`DocError::Store`] (→ 500). A non-database resolved path is the host-fs seam.
pub fn read_route_document(
    mount: &Connection,
    resolved: &ResolvedPath,
) -> Result<(String, i64), DocError> {
    if resolved.mount_type.as_deref() != Some("database") {
        return Err(DocError::Fs);
    }
    let mp = resolved.mount_point_id.as_deref().ok_or_else(|| {
        DocError::Store("Database-backed ResolvedPath is missing mountPointId".into())
    })?;
    match database_store::read_database_document(mount, mp, &resolved.relative_path) {
        Ok(doc) => Ok((doc.content, doc.mtime_ms)),
        Err(StoreError::Store(s)) if s.code == DbStoreErrorCode::NotFound => {
            Err(DocError::Missing(s.message))
        }
        Err(e) => Err(store_err(e)),
    }
}

/// Read a database-backed resolved path's content + mtime. Non-database is the
/// host-fs seam.
fn read_database(mount: &Connection, resolved: &ResolvedPath) -> Result<ReadDoc, DocError> {
    if resolved.mount_type.as_deref() != Some("database") {
        return Err(DocError::Fs);
    }
    let mp = resolved.mount_point_id.as_deref().ok_or_else(|| {
        DocError::Store("Database-backed ResolvedPath is missing mountPointId".into())
    })?;
    database_store::read_database_document(mount, mp, &resolved.relative_path).map_err(store_err)
}

/// Write a database-backed resolved path's content, returning the fresh mtime.
fn write_database(
    mount: &Connection,
    resolved: &ResolvedPath,
    content: &str,
) -> Result<i64, DocError> {
    if resolved.mount_type.as_deref() != Some("database") {
        return Err(DocError::Fs);
    }
    let mp = resolved.mount_point_id.as_deref().ok_or_else(|| {
        DocError::Store("Database-backed ResolvedPath is missing mountPointId".into())
    })?;
    database_store::write_database_document(mount, mp, &resolved.relative_path, content)
        .map_err(store_err)
}

/// Write with the v4 `writeDatabaseDocument` `expectedMtime` guard: when an
/// existing document's `lastModified` disagrees with `expected`, refuse with a
/// CONFLICT-shaped [`DocError::MtimeMismatch`] (v5's `database_store` write does
/// not itself port the guard — it is reproduced here so the 409 arm is faithful,
/// without touching lane A's `database_store.rs`).
fn write_database_checked(
    mount: &Connection,
    resolved: &ResolvedPath,
    content: &str,
    expected_mtime: Option<i64>,
) -> Result<i64, DocError> {
    if resolved.mount_type.as_deref() != Some("database") {
        return Err(DocError::Fs);
    }
    let mp = resolved.mount_point_id.as_deref().ok_or_else(|| {
        DocError::Store("Database-backed ResolvedPath is missing mountPointId".into())
    })?;
    if let Some(expected) = expected_mtime {
        match database_store::read_database_document(mount, mp, &resolved.relative_path) {
            Ok(existing) => {
                if existing.mtime_ms != expected {
                    return Err(DocError::MtimeMismatch(
                        "Document was modified by another process (mtime mismatch). Please reload and try again."
                            .to_string(),
                    ));
                }
            }
            // NOT_FOUND → no existing document → this is a create; proceed.
            Err(StoreError::Store(s)) if s.code == DbStoreErrorCode::NotFound => {}
            Err(e) => return Err(store_err(e)),
        }
    }
    database_store::write_database_document(mount, mp, &resolved.relative_path, content)
        .map_err(store_err)
}

/// v4 `params.title || path.basename(filePath)`.
fn display_title_for(title: Option<&str>, file_path: &str) -> String {
    match title {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => node_basename(file_path).to_string(),
    }
}

/// Fire the refresh seam, or log a loud skip when unwired (never silent).
fn schedule_refresh(
    scheduler: Option<&dyn MountRefreshScheduler>,
    mount_point_id: &str,
    path: Option<&str>,
    file_path: &str,
) {
    match scheduler {
        Some(s) => s.schedule_refresh(mount_point_id, path),
        None => {
            // The seam is unwired (defaults to None until unification wires lane
            // A's reindex/embed services). Loudly note the skip (never silent).
            eprintln!(
                "MountRefreshScheduler unwired — skipping document-store refresh for {file_path} (mount {mount_point_id})"
            );
        }
    }
}

// --- Node path.* helpers (posix) ------------------------------------------

/// Node `path.extname` (posix): the substring from the last `.` in the final
/// segment, or `""` for no dot / a leading-dot dotfile.
fn node_extname(p: &str) -> String {
    let base = p.rsplit('/').next().unwrap_or(p);
    match base.rfind('.') {
        None | Some(0) => String::new(),
        Some(i) => base[i..].to_string(),
    }
}

/// Node `path.basename` (posix).
fn node_basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Node `path.dirname` (posix), for the file paths this module handles (no
/// trailing slashes): the prefix up to the last `/`, `"."` when there is none,
/// `"/"` when the only `/` is at the front.
fn node_dirname(p: &str) -> String {
    match p.rfind('/') {
        None => ".".to_string(),
        Some(0) => "/".to_string(),
        Some(i) => p[..i].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(current: &str, title: &str) -> (String, String) {
        match compute_rename_target(current, title) {
            RenameTarget::Ok {
                new_file_path,
                new_display_title,
            } => (new_file_path, new_display_title),
            RenameTarget::Err { reason } => panic!("expected Ok, got Err({reason})"),
        }
    }
    fn err(current: &str, title: &str) -> String {
        match compute_rename_target(current, title) {
            RenameTarget::Err { reason } => reason,
            RenameTarget::Ok { new_file_path, .. } => panic!("expected Err, got {new_file_path}"),
        }
    }

    #[test]
    fn rename_inherits_old_extension_and_dir() {
        assert_eq!(
            ok("Notes/today.md", "tomorrow"),
            ("Notes/tomorrow.md".into(), "tomorrow.md".into())
        );
        assert_eq!(
            ok("today.md", "backstory"),
            ("backstory.md".into(), "backstory.md".into())
        );
        // An explicit extension is kept verbatim.
        assert_eq!(ok("a/b.md", "c.txt"), ("a/c.txt".into(), "c.txt".into()));
        // No old extension + no new extension → no extension.
        assert_eq!(
            ok("dir/README", "LICENSE"),
            ("dir/LICENSE".into(), "LICENSE".into())
        );
    }

    #[test]
    fn rename_rejects_bad_names() {
        assert_eq!(err("a.md", "   "), "Name cannot be empty");
        assert_eq!(err("a.md", "b/c"), "Name cannot contain path separators");
        assert_eq!(err("a.md", "b\\c"), "Name cannot contain path separators");
        assert_eq!(err("a.md", "."), "Invalid name");
        assert_eq!(err("a.md", ".."), "Invalid name");
    }

    #[test]
    fn rename_trims_title() {
        assert_eq!(
            ok("a/b.md", "  spaced  "),
            ("a/spaced.md".into(), "spaced.md".into())
        );
    }

    #[test]
    fn untitled_candidate_counter() {
        assert_eq!(untitled_candidate(1), "Untitled Document.md");
        assert_eq!(untitled_candidate(2), "Untitled Document 2.md");
        assert_eq!(untitled_candidate(37), "Untitled Document 37.md");
    }

    #[test]
    fn trim_slashes_strips_ends() {
        assert_eq!(trim_slashes("/notes/sub/"), "notes/sub");
        assert_eq!(trim_slashes("notes"), "notes");
        assert_eq!(trim_slashes(""), "");
        assert_eq!(trim_slashes("///"), "");
    }

    #[test]
    fn mime_for_extension_image_gate() {
        assert!(mime_for_extension("a.png").starts_with("image/"));
        assert!(mime_for_extension("a.JPG").starts_with("image/"));
        assert!(!mime_for_extension("a.bmp").starts_with("image/")); // not in the path-utils map
        assert_eq!(mime_for_extension("a.md"), "text/markdown; charset=utf-8");
        assert_eq!(mime_for_extension("noext"), "application/octet-stream");
    }

    #[test]
    fn node_path_helpers() {
        assert_eq!(node_extname("a/b.md"), ".md");
        assert_eq!(node_extname(".gitignore"), "");
        assert_eq!(node_dirname("a/b/c.md"), "a/b");
        assert_eq!(node_dirname("c.md"), ".");
        assert_eq!(node_dirname("/a"), "/");
        assert_eq!(node_basename("a/b/c.md"), "c.md");
    }
}
