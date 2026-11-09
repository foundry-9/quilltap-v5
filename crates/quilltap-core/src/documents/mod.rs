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

pub mod core;

use rusqlite::Connection;
use serde::Serialize;

#[cfg(test)]
use crate::db::database_store::StoreError;
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
    /// The host files directory (v4 `getFilesDir()` = `<base>/files`). `Some`
    /// makes the operator doc surfaces reach the `general`/legacy-project/fs-mount
    /// host-disk branches; `None` preserves the historic FsSeam refusal (P4.6bg
    /// S2). Threaded straight into [`resolve_operator_doc_path`].
    pub files_dir: Option<std::path::PathBuf>,
}

impl DocumentAccessContext {
    /// v4 `STANDALONE_ACCESS_CONTEXT` — nothing behind it (the chat-less caller).
    /// `files_dir` is the host files dir the operator surface reaches disk through
    /// (`None` keeps the FsSeam refusal).
    pub fn standalone(files_dir: Option<std::path::PathBuf>) -> Self {
        DocumentAccessContext {
            project_id: None,
            character_ids: Vec::new(),
            files_dir,
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
    /// v4 `DocumentReadOnlyError` — an editor write to a file Document Mode
    /// only views (→ 400; P4.D380, contract C14).
    ReadOnly(String),
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
            | DocError::Store(m)
            | DocError::ReadOnly(m) => m.clone(),
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
        // The operator override carries its own accessible set (every enabled
        // store) and bypasses the collector, so the covenant never applies here.
        hide_character_vaults: false,
        mount_point: mount_point.map(str::to_string),
        operator_override: true,
        // P4.D216 OUT-OF-MANDATE — the documents module's owner preserves (a
        // required field on a shared struct, v4 `d1c06cd9d`): Document Mode is
        // the operator surface, never a pre-built pool.
        mount_pool: None,
    };
    resolve_doc_edit_path(
        main,
        mount,
        scope,
        Some(file_path),
        &context,
        ctx.files_dir.as_deref(),
    )
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
            // v4 `operator-doc-actions.ts:160-170`: the fallback link read — a
            // failed read is `null` → `'other'` (P4.142 G2).
            let links = DocMountFileLinksRepository::new(mount);
            if let Some(link) =
                links.find_by_mount_point_and_path_or_none(mp, &resolved.relative_path)
            {
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
        // v4 `documentExists` (R-4: only absence reads as free; a permission
        // or database failure propagates rather than being written over).
        if !self::core::document_exists(mount, &resolved)? {
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
    /// The read's (or the blank create's) mtime; `None` for a file shown
    /// read-only (v4 leaves the key out).
    pub mtime: Option<i64>,
    /// True when a new blank document was created (no `file_path` was given).
    pub is_new: bool,
    /// Set for a file shown read-only (not text); `content` is then empty
    /// (contract C14).
    pub viewer: Option<self::core::DocumentViewTarget>,
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

/// v4 `openDocumentFile` (`operator-doc-actions.ts:196-244`) — the file half
/// of opening a document: read an existing file (or, for a file Document Mode
/// only views, its view target), or pick an unused "Untitled Document.md" name
/// and create it blank through the core's [`self::core::ensure_document`] (a
/// database store gets a document row and every store its refresh). Throws
/// [`DocError::Missing`] when a named file can't be read — v4's catch-all, so a
/// resolution failure is a 404 here too. The create branch WRITES, so this runs
/// inside a write closure.
pub fn open_document_file(
    main: &Connection,
    mount: &Connection,
    ctx: &DocumentAccessContext,
    params: &OpenDocumentFileParams,
    refresh: self::core::Refresh<'_>,
) -> Result<OpenedDocumentFile, DocError> {
    if let Some(file_path) = params.file_path.filter(|p| !p.is_empty()) {
        let display_title = display_title_for(params.title, file_path);
        let attempt: Result<OpenedDocumentFile, DocError> = (|| {
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
            if let Some(viewer) = self::core::view_target_for(mount, &resolved)? {
                tracing::debug!(
                    target: "quilltap::documents",
                    filePath = %file_path,
                    mountPointId = %viewer.mount_point_id,
                    "openDocumentFile: opening read-only"
                );
                return Ok(OpenedDocumentFile {
                    file_path: file_path.to_string(),
                    display_title: display_title.clone(),
                    content: String::new(),
                    mtime: None,
                    is_new: false,
                    viewer: Some(viewer),
                });
            }
            let read = self::core::read_document(mount, &resolved)?;
            Ok(OpenedDocumentFile {
                file_path: file_path.to_string(),
                display_title: display_title.clone(),
                content: read.content,
                mtime: Some(read.mtime_ms),
                is_new: false,
                viewer: None,
            })
        })();
        return attempt.map_err(|e| {
            tracing::debug!(
                target: "quilltap::documents",
                filePath = %file_path,
                scope = params.scope.as_str(),
                mountPoint = params.mount_point,
                error = %e.message(),
                "openDocumentFile: file not readable"
            );
            DocError::Missing(format!("File not found: {file_path}"))
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
    let mtime = match self::core::ensure_document(main, mount, &resolved, refresh)? {
        self::core::Ensured::Created(mtime) => mtime,
        // "The name was free a moment ago; something else took it."
        self::core::Ensured::Existing => {
            return Err(DocError::Conflict(
                "A file already exists at that name.".to_string(),
            ))
        }
    };
    let display_title = display_title_for(params.title, &file_path);
    Ok(OpenedDocumentFile {
        file_path,
        display_title,
        content: String::new(),
        mtime: Some(mtime),
        is_new: true,
        viewer: None,
    })
}

/// v4 `writeDocumentFile` (`operator-doc-actions.ts:252-273`) — resolve with
/// the operator override, refuse a file Document Mode only views
/// ([`DocError::ReadOnly`]), then the core's [`self::core::write_document`]
/// (mtime-checked, refreshed through the ONE chokepoint whatever the scope —
/// #133). A stale mtime is [`DocError::MtimeMismatch`] for the 409; a
/// resolution failure is [`DocError::Resolve`] (v4's catch-all 500 arm).
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
    refresh: self::core::Refresh<'_>,
) -> Result<i64, DocError> {
    let resolved = resolve_operator_doc_path(main, mount, ctx, scope, file_path, mount_point, None)
        .map_err(resolve_err)?;
    if self::core::view_target_for(mount, &resolved)?.is_some() {
        return Err(DocError::ReadOnly(format!(
            "{file_path} is shown read-only and cannot be written from the editor"
        )));
    }
    self::core::write_document(main, mount, &resolved, content, expected_mtime, refresh)
}

/// A store the picker reaches from a left-column button (v4 `LibraryTarget`:
/// the Project library, the General library).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTarget {
    pub mount_point_id: String,
    /// Canonical mount name — what `document_store` opens resolve against.
    pub name: String,
    pub mount_type: String,
}

/// v4 `resolveGeneralLibrary` (`operator-doc-actions.ts:319-328`, `c17033f04`)
/// — Quilltap General, the instance-wide store behind the picker's "General
/// library" button. `None` when it is not provisioned or is disabled (the
/// latter with v4's DEBUG).
pub fn resolve_general_library(
    main: &Connection,
    mount: &Connection,
) -> Result<Option<LibraryTarget>, DbError> {
    let Some(general_id) = crate::db::instance_settings::get_general_mount_point_id(main)? else {
        return Ok(None);
    };
    let mp = DocMountPointsRepository::new(mount).find_full_json_by_id(&general_id)?;
    let enabled = mp
        .as_ref()
        .and_then(|m| m.get("enabled"))
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let Some(mp) = mp.filter(|_| enabled) else {
        tracing::debug!(
            target: "quilltap::documents",
            generalId = %general_id,
            "resolveGeneralLibrary: Quilltap General is missing or disabled"
        );
        return Ok(None);
    };
    let field = |k: &str| {
        mp.get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Ok(Some(LibraryTarget {
        mount_point_id: field("id"),
        name: field("name"),
        mount_type: field("mountType"),
    }))
}

pub use self::core::{compute_rename_target, DeleteOutcome, RenameTarget};

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
    // P4.163: v4 `findEnabled()` (`operator-doc-actions.ts:543`) is a fallback —
    // the filter line and `[]` (v5 had failed the whole listing).
    let mounts = crate::db::fallback::find_by_filter_or_empty("doc_mount_points", || {
        mp_repo.find_enabled_for_docedit()
    });

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

/// v4 `params.title || path.basename(filePath)`.
fn display_title_for(title: Option<&str>, file_path: &str) -> String {
    match title {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => node_basename(file_path).to_string(),
    }
}

// --- Node path.* helpers (posix) ------------------------------------------

/// Node `path.extname` (posix): the substring from the last `.` in the final
/// segment, or `""` for no dot / a leading-dot dotfile.
pub(crate) fn node_extname(p: &str) -> String {
    let base = p.rsplit('/').next().unwrap_or(p);
    match base.rfind('.') {
        None | Some(0) => String::new(),
        Some(i) => base[i..].to_string(),
    }
}

/// Node `path.basename` (posix).
pub(crate) fn node_basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Node `path.dirname` (posix), for the file paths this module handles (no
/// trailing slashes): the prefix up to the last `/`, `"."` when there is none,
/// `"/"` when the only `/` is at the front.
pub(crate) fn node_dirname(p: &str) -> String {
    match p.rfind('/') {
        None => ".".to_string(),
        Some(0) => "/".to_string(),
        Some(i) => p[..i].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P4.D340 R-8 (c): Document Mode answers v4's `Failed to write document:
    /// <message>` 500 — a refusal is a plain store error here, carrying its
    /// message. (P4.D380: the write reaches it through the core's one writer,
    /// `write_file_with_mtime_check` → `store_error_message` → `DocError::Store`.)
    #[test]
    fn a_title_lint_refusal_is_a_store_error_with_its_message() {
        let store_err =
            |e: StoreError| DocError::Store(crate::tools::doc_edit::shared::store_error_message(e));
        let r = crate::inventory::errors::InventoryRefusal::title_shadow(
            "Lantern",
            &[
                crate::inventory::errors::InventoryTitleCollision::KindNameShadow {
                    kind_name: "lantern".into(),
                },
            ],
        );
        let e = store_err(StoreError::TitleLint(r.clone()));
        assert!(matches!(e, DocError::Store(_)));
        assert_eq!(e.message(), r.message);
    }

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

#[cfg(test)]
mod classify_fallback_tests {
    use super::*;
    use crate::doc_edit::path_resolver::ResolvedPath;
    use crate::doc_edit::DocEditScope;

    /// P4.142 G2: the qtap-link gate's link read is v4's fallback
    /// (`operator-doc-actions.ts:160-170`) — a broken links table classifies a
    /// database-mount `.bin` as `other` with v4's joined line, never an `Err`.
    #[test]
    fn a_failed_link_read_classifies_as_other() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let resolved = ResolvedPath {
            absolute_path: String::new(),
            scope: DocEditScope::DocumentStore,
            mount_point_id: Some("mp-1".into()),
            mount_point_name: None,
            mount_type: Some("database".into()),
            base_path: String::new(),
            relative_path: "files/x.bin".into(),
        };
        let (got, lines) =
            crate::test_support::captured_with(|| classify_resolved_target(&conn, &resolved));
        assert_eq!(got.unwrap(), "other");
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error querying joined file links collection=doc_mount_file_links whereClause=WHERE l.mountPointId = ? AND LOWER(l.relativePath) = LOWER(?) error=no such table: doc_mount_file_links".to_string()]
        );
    }
}
