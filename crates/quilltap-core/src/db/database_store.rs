//! Database-backed document-store **operations** — v4's
//! `lib/mount-index/database-store.ts`.
//!
//! For a mount point with `mountType === 'database'`, document bytes live in
//! `doc_mount_documents` inside the mount-index DB — there is no filesystem path.
//! This module implements the same read / write / delete / move / list / folder
//! operations the filesystem path-resolver offers, but routed through the ported
//! repositories. It is the layer the `doc_*` tool handlers (W4.1d batch 3b) call.
//!
//! Every function here is a **free function** over a borrowed
//! [`rusqlite::Connection`] (the mount-index DB connection) — the same style as
//! [`super::doc_mount_file_links`] — and **composes** the already-ported storage
//! primitives (`link_document_content`, `delete_database_document`,
//! `ensure_folder_path`, `normalise_relative_path`, `detect_database_file_type`,
//! `sha256_of_string`, `basename`/`dirname`), never reimplementing them.
//!
//! ## Seams deliberately NOT ported (no-ops in v4's port surface)
//!
//! - The **`emitDocument*` events** (`emitDocumentWritten` / `Deleted` / `Moved`)
//!   trigger the embedding re-index watcher; they are no-op seams here.
//! - The post-write **`reindexSingleFile`** chunk pass (v4 skips it under
//!   `QUILLTAP_JOB_CHILD=1`) is a standing no-op seam — the differential pins it
//!   off, so [`write_database_document`] does NOT re-chunk.
//! - The **`expectedMtime`** mtime-conflict guard is out of scope (not ported).
//! - `backfillFolderRowsForMountPoint` / `rescanDatabaseMountPoint` are out of
//!   scope (they drive the chunk/embed pipeline).
//!
//! ## Error messages reach the LLM
//!
//! The `doc_*` tool handlers inspect the thrown message text (they check
//! `.includes('not empty')`, `.toLowerCase().includes('not found')`, `already
//! exists`), so every message here is byte-faithful to v4's `DatabaseStoreError`
//! strings. Errors carry a typed [`DbStoreErrorCode`] so handlers can dispatch
//! precisely; the `Display`/`message` is the exact v4 string.

use rusqlite::Connection;

use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::{
    detect_database_file_type, normalise_relative_path, sha256_of_string,
    DocMountFileLinksRepository, LinkDocumentInput, LinkUpdate,
};
use super::doc_mount_files::{DmfUpdate, DocMountFilesRepository};
use super::doc_mount_folders::DocMountFoldersRepository;
use super::DbError;
use crate::clock::{iso_to_ms, now_iso, now_unix_ms};

/// v4 `DatabaseStoreError.code` — the discriminant the tool handlers dispatch on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbStoreErrorCode {
    NotFound,
    Unsupported,
    Conflict,
    Invalid,
    NotEmpty,
}

/// v4 `DatabaseStoreError` — a message + a code. `Display` is the message
/// verbatim (the byte-faithful v4 string the handlers `.includes(...)` on).
#[derive(Clone, Debug)]
pub struct DatabaseStoreError {
    pub message: String,
    pub code: DbStoreErrorCode,
}

impl DatabaseStoreError {
    fn new(message: impl Into<String>, code: DbStoreErrorCode) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }
}

impl std::fmt::Display for DatabaseStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DatabaseStoreError {}

/// A database-store op fails with either a typed store error (the v4-message
/// carrying cases the handlers inspect) or an underlying DB error.
#[derive(Debug)]
pub enum StoreError {
    Store(DatabaseStoreError),
    Db(DbError),
}

impl From<DbError> for StoreError {
    fn from(e: DbError) -> Self {
        StoreError::Db(e)
    }
}

impl From<DatabaseStoreError> for StoreError {
    fn from(e: DatabaseStoreError) -> Self {
        StoreError::Store(e)
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Store(e) => write!(f, "{e}"),
            StoreError::Db(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

// ============================================================================
// READ
// ============================================================================

/// A read document — v4 `readDatabaseDocument`'s return
/// (`{ content, mtime, size }`).
#[derive(Clone, Debug)]
pub struct ReadDoc {
    pub content: String,
    /// `new Date(doc.lastModified).getTime()` where `doc.lastModified` IS the
    /// joined `l.lastModified`.
    pub mtime_ms: i64,
    /// `doc.content.length` — JS UTF-16 code units.
    pub size: i64,
}

/// v4 `readDatabaseDocument` (`database-store.ts:78`): read a document's content +
/// mtime + size at `(mountPointId, rel)`. NOT_FOUND → a `NotFound` store error
/// with v4's exact message (kept faithful; the text-read handler special-cases it).
pub fn read_database_document(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<ReadDoc, StoreError> {
    let rel = normalise_relative_path(relative_path)?;
    let docs = DocMountDocumentsRepository::new(conn);
    let found = docs.find_content_and_mtime_by_mount_point_and_path(mount_point_id, &rel)?;
    let Some((content, last_modified)) = found else {
        return Err(DatabaseStoreError::new(
            format!("Document not found in database-backed store: {rel}"),
            DbStoreErrorCode::NotFound,
        )
        .into());
    };
    // JS `content.length` = UTF-16 code units.
    let size = content.encode_utf16().count() as i64;
    // v4 `new Date(doc.lastModified).getTime()`; a NaN date would surface as 0.
    let mtime_ms = iso_to_ms(&last_modified).unwrap_or(0);
    Ok(ReadDoc {
        content,
        mtime_ms,
        size,
    })
}

// ============================================================================
// WRITE (create or update)
// ============================================================================

/// v4 `writeDatabaseDocument` (`database-store.ts:102`): normalise the path,
/// reject non-text extensions, then land the bytes via `linkDocumentContent`.
/// Returns the freshly-minted `mtime` (`new Date(now).getTime()`).
///
/// The `expectedMtime` mtime-conflict guard and the post-write `reindexSingleFile`
/// chunk pass are OUT OF SCOPE (reindex is a standing no-op seam; the differential
/// pins it off), so neither is ported here.
///
/// NB: this reproduces **v4's `writeDatabaseDocument` unsupported-extension
/// message** — which differs from the repo method
/// `DocMountFileLinksRepository::write_database_document`'s message — so it is
/// composed from `link_document_content` directly rather than reusing that method.
pub fn write_database_document(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    content: &str,
) -> Result<i64, StoreError> {
    let rel = normalise_relative_path(relative_path)?;
    let file_type = detect_database_file_type(&rel).ok_or_else(|| {
        DatabaseStoreError::new(
            format!(
                "Database-backed stores only accept text documents (.md, .markdown, .txt, .json, .jsonl, .ndjson). Got: {}",
                path_extname(&rel)
            ),
            DbStoreErrorCode::Unsupported,
        )
    })?;

    let content_sha256 = sha256_of_string(content);
    let file_name = basename(&rel).to_string();

    let links = DocMountFileLinksRepository::new(conn);
    // Build LinkDocumentInput exactly as the repo's write_database_document does:
    // plain_text_length = UTF-16 count, file_size_bytes = UTF-8 byte len, allow_*
    // = None (policy derived from content).
    links.link_document_content(&LinkDocumentInput {
        mount_point_id: mount_point_id.to_string(),
        relative_path: rel.clone(),
        file_name,
        file_type: file_type.to_string(),
        content: content.to_string(),
        content_sha256,
        plain_text_length: content.encode_utf16().count() as i64,
        file_size_bytes: content.len() as i64,
        allow_embed: None,
        allow_character_read: None,
        allow_character_write: None,
    })?;

    // v4 returns `new Date(now).getTime()` for a freshly-minted `now`.
    Ok(now_unix_ms())
}

// ============================================================================
// DELETE
// ============================================================================

/// v4 `deleteDatabaseDocument` (`database-store.ts:188`): unlink the document at
/// `(mountPointId, rel)` with GC. Returns `false` when no link exists (v4's
/// NOT_FOUND-tolerant early return). The `emitDocumentDeleted` event is a no-op
/// seam (skipped).
pub fn delete_database_document(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<bool, DbError> {
    DocMountFileLinksRepository::new(conn).delete_database_document(mount_point_id, relative_path)
}

// ============================================================================
// MOVE / RENAME
// ============================================================================

/// v4 `moveDatabaseDocument` (`database-store.ts:209`): rename a document. Verifies
/// the source exists, the target extension is supported, and no conflict at the
/// target; ensures the destination folder; then updates the link's
/// `relativePath`/`fileName`/`folderId`, and the file row's `fileType` when it
/// changed. `emitDocumentMoved` is a no-op seam.
pub fn move_database_document(
    conn: &Connection,
    mount_point_id: &str,
    from_relative_path: &str,
    to_relative_path: &str,
) -> Result<(), StoreError> {
    let from_rel = normalise_relative_path(from_relative_path)?;
    let to_rel = normalise_relative_path(to_relative_path)?;

    let docs = DocMountDocumentsRepository::new(conn);
    // Source must exist (content lookup suffices for existence).
    if docs
        .find_by_mount_point_and_path(mount_point_id, &from_rel)?
        .is_none()
    {
        return Err(DatabaseStoreError::new(
            format!("Source document not found: {from_rel}"),
            DbStoreErrorCode::NotFound,
        )
        .into());
    }

    let file_type = detect_database_file_type(&to_rel).ok_or_else(|| {
        DatabaseStoreError::new(
            format!(
                "Target path has unsupported extension: {}",
                path_extname(&to_rel)
            ),
            DbStoreErrorCode::Unsupported,
        )
    })?;

    // Case-only rename of the same document (notes.md → Notes.md) is allowed —
    // the case-insensitive lookup would otherwise find the source itself and
    // report a bogus conflict.
    let case_only_rename = from_rel != to_rel && from_rel.to_lowercase() == to_rel.to_lowercase();
    if !case_only_rename
        && docs
            .find_by_mount_point_and_path(mount_point_id, &to_rel)?
            .is_some()
    {
        return Err(DatabaseStoreError::new(
            format!("Target already exists: {to_rel}"),
            DbStoreErrorCode::Conflict,
        )
        .into());
    }

    // Ensure destination folder exists and get its ID.
    let links = DocMountFileLinksRepository::new(conn);
    let dest_folder_path = dirname(&to_rel);
    let dest_folder_id = if dest_folder_path != "." {
        links.ensure_folder_path(mount_point_id, &dest_folder_path)?
    } else {
        None
    };

    // Move = update the link row at the source path. fileType lives on the content
    // row so it doesn't move with the rename, but the fileType detected from the
    // new path may differ; update the file row's fileType when it changed.
    if let Some(link) = links.find_by_mount_point_and_path(mount_point_id, &from_rel)? {
        links.update(
            &link.id,
            &LinkUpdate {
                relative_path: Some(to_rel.clone()),
                file_name: Some(basename(&to_rel).to_string()),
                folder_id: Some(dest_folder_id),
                updated_at: now_iso(),
                ..Default::default()
            },
        )?;
        if link.file_type != file_type {
            DocMountFilesRepository::new(conn).update(
                &link.file_id,
                &DmfUpdate {
                    file_type: Some(file_type.to_string()),
                    updated_at: now_iso(),
                    ..Default::default()
                },
            )?;
        }
    }

    Ok(())
}

// ============================================================================
// LIST (for doc_list_files support)
// ============================================================================

/// A listing entry — v4 `DatabaseFileListEntry`. Folders are synthesised to look
/// enough like file links for the `doc_list_files` tool / Scriptorium UI to render
/// a unified tree (`kind` distinguishes them).
#[derive(Clone, Debug)]
pub struct DbFileListEntry {
    pub id: String,
    pub mount_point_id: String,
    pub relative_path: String,
    pub file_name: String,
    pub file_type: String,
    pub sha256: String,
    pub file_size_bytes: i64,
    pub last_modified: String,
    pub source: String,
    pub folder_id: Option<String>,
    /// `"file"` | `"folder"`.
    pub kind: String,
}

/// v4 `listDatabaseFiles` (`database-store.ts:286`): enumerate a mount point's
/// files + synthesised folder entries, optionally filtered to a folder. Links are
/// emitted first, then folders (matching v4's push order). The folder filter is
/// normalised (leading/trailing slashes stripped); empty/root returns everything,
/// else prefix-filters on `"{folder}/"`.
pub fn list_database_files(
    conn: &Connection,
    mount_point_id: &str,
    folder: Option<&str>,
) -> Result<Vec<DbFileListEntry>, DbError> {
    let links = DocMountFileLinksRepository::new(conn).find_by_mount_point_id(mount_point_id)?;
    let folders = DocMountFoldersRepository::new(conn).find_by_mount_point_id(mount_point_id)?;

    // Normalise folder input. Stored paths carry no leading/trailing slashes, so
    // strip any here before comparing. '', '/', '//' etc. → "no filter" (root).
    let normalised_folder = folder.unwrap_or("").trim_matches('/').to_string();

    let mut entries: Vec<DbFileListEntry> = Vec::new();

    let link_to_entry = |link: &super::doc_mount_file_links::LinkRow| DbFileListEntry {
        id: link.id.clone(),
        mount_point_id: link.mount_point_id.clone(),
        relative_path: link.relative_path.clone(),
        file_name: link.file_name.clone(),
        file_type: link.file_type.clone(),
        sha256: link.sha256.clone(),
        file_size_bytes: link.file_size_bytes,
        last_modified: link.last_modified.clone(),
        source: link.source.clone(),
        folder_id: link.folder_id.clone(),
        kind: "file".to_string(),
    };

    let folder_to_entry = |f: &super::doc_mount_folders::FolderRow| DbFileListEntry {
        id: f.id.clone(),
        mount_point_id: f.mount_point_id.clone(),
        relative_path: f.path.clone(),
        file_name: basename(&f.path).to_string(),
        file_type: "markdown".to_string(),
        sha256: String::new(),
        file_size_bytes: 0,
        last_modified: f.created_at.clone(),
        source: "database".to_string(),
        folder_id: f.parent_id.clone(),
        kind: "folder".to_string(),
    };

    if normalised_folder.is_empty() {
        for link in &links {
            entries.push(link_to_entry(link));
        }
        for f in &folders {
            entries.push(folder_to_entry(f));
        }
        return Ok(entries);
    }

    // Filter to a specific folder.
    let folder_prefix = format!("{normalised_folder}/");
    for link in &links {
        if link.relative_path.starts_with(&folder_prefix) {
            entries.push(link_to_entry(link));
        }
    }
    for f in &folders {
        if f.path.starts_with(&folder_prefix) {
            entries.push(folder_to_entry(f));
        }
    }
    Ok(entries)
}

// ============================================================================
// FOLDER OPERATIONS
// ============================================================================

/// v4 `createDatabaseFolder` (`database-store.ts:367`): `ensureFolderPath` over the
/// normalised path, returning `(folderId, path)` (empty id / path for root).
pub fn create_database_folder(
    conn: &Connection,
    mount_point_id: &str,
    folder_path: &str,
) -> Result<(String, String), DbError> {
    let rel = normalise_relative_path(folder_path)?;
    let folder_id =
        DocMountFileLinksRepository::new(conn).ensure_folder_path(mount_point_id, &rel)?;
    // v4: `folderId || ''`, `rel || ''`.
    Ok((folder_id.unwrap_or_default(), rel))
}

/// v4 `deleteDatabaseFolder` (`database-store.ts:380`): delete an EMPTY folder.
/// NOT_FOUND (message carries the ORIGINAL `folder_path`, not normalised) and
/// NOT_EMPTY are byte-faithful (the handler `.includes('not empty')` /
/// `.includes('not found')`).
pub fn delete_database_folder(
    conn: &Connection,
    mount_point_id: &str,
    folder_path: &str,
) -> Result<(), StoreError> {
    let rel = normalise_relative_path(folder_path)?;
    let folders = DocMountFoldersRepository::new(conn);

    let Some(folder) = folders.find_by_mount_point_and_path(mount_point_id, &rel)? else {
        // v4 interpolates the ORIGINAL folderPath, not the normalised rel.
        return Err(DatabaseStoreError::new(
            format!("Folder not found: {folder_path}"),
            DbStoreErrorCode::NotFound,
        )
        .into());
    };

    if folder_has_contents(conn, mount_point_id, &folder.id)? {
        return Err(DatabaseStoreError::new(
            format!("Folder is not empty: {folder_path}. Only empty folders can be deleted."),
            DbStoreErrorCode::NotEmpty,
        )
        .into());
    }

    folders.delete(&folder.id)?;
    Ok(())
}

/// v4 `moveDatabaseFolder` (`database-store.ts:412`): rename/move a folder and
/// rewrite every descendant folder's path + every link under it. Verifies source
/// exists + dest doesn't; ensures the dest parent. `emitDocumentMoved` per moved
/// doc is a no-op seam.
pub fn move_database_folder(
    conn: &Connection,
    mount_point_id: &str,
    from_path: &str,
    to_path: &str,
) -> Result<(), StoreError> {
    let from_rel = normalise_relative_path(from_path)?;
    let to_rel = normalise_relative_path(to_path)?;

    let folders = DocMountFoldersRepository::new(conn);
    let links_repo = DocMountFileLinksRepository::new(conn);

    let Some(source_folder) = folders.find_by_mount_point_and_path(mount_point_id, &from_rel)?
    else {
        return Err(DatabaseStoreError::new(
            format!("Source folder not found: {from_path}"),
            DbStoreErrorCode::NotFound,
        )
        .into());
    };

    // The lookup is case-insensitive, so a case-only rename of the folder itself
    // (lore → Lore) finds the source row — that's allowed; any OTHER folder at
    // the destination (in any casing) is a conflict.
    if let Some(dest_folder) = folders.find_by_mount_point_and_path(mount_point_id, &to_rel)? {
        if dest_folder.id != source_folder.id {
            return Err(DatabaseStoreError::new(
                format!("Destination folder already exists: {to_path}"),
                DbStoreErrorCode::Conflict,
            )
            .into());
        }
    }

    // Ensure parent of destination exists + resolve its id AND its stored path.
    let dest_dir = dirname(&to_rel);
    let mut dest_parent_id: Option<String> = None;
    let mut dest_parent_path = String::new();
    if dest_dir != "." {
        links_repo.ensure_folder_path(mount_point_id, &dest_dir)?;
        if let Some(parent) = folders.find_by_mount_point_and_path(mount_point_id, &dest_dir)? {
            dest_parent_id = Some(parent.id);
            dest_parent_path = parent.path;
        }
    }

    // Canonicalise: the destination directory may have been addressed in a
    // different casing than its stored path; the source's descendants share the
    // source's STORED path prefix, not necessarily the caller-typed one.
    let new_name = basename(&to_rel).to_string();
    let canonical_to_rel = if dest_parent_path.is_empty() {
        new_name.clone()
    } else {
        format!("{dest_parent_path}/{new_name}")
    };
    let old_prefix = format!("{}/", source_folder.path);
    let new_prefix = format!("{canonical_to_rel}/");

    // Update the source folder row itself.
    folders.update(
        &source_folder.id,
        &super::doc_mount_folders::DmfUpdate {
            parent_id: dest_parent_id.clone(),
            name: Some(new_name),
            path: Some(canonical_to_rel.clone()),
            updated_at: now_iso(),
            ..Default::default()
        },
    )?;

    // Update all descendant folder paths.
    let all_folders = folders.find_by_mount_point_id(mount_point_id)?;
    for folder in &all_folders {
        if folder.id == source_folder.id {
            continue;
        }
        if !old_prefix.is_empty() && folder.path.starts_with(&old_prefix) {
            let new_path = format!("{}{}", new_prefix, &folder.path[old_prefix.len()..]);
            folders.update(
                &folder.id,
                &super::doc_mount_folders::DmfUpdate {
                    path: Some(new_path),
                    updated_at: now_iso(),
                    ..Default::default()
                },
            )?;
        }
    }

    // Walk every link and rewrite its relativePath + folderId when it falls inside
    // the renamed folder. Post-refactor, path/folder membership lives entirely on
    // doc_mount_file_links.
    let links = links_repo.find_by_mount_point_id(mount_point_id)?;
    for link in &links {
        if !old_prefix.is_empty() && link.relative_path.starts_with(&old_prefix) {
            let new_path = format!("{}{}", new_prefix, &link.relative_path[old_prefix.len()..]);
            let new_folder_path = dirname(&new_path);
            let mut new_folder_id: Option<String> = None;
            if new_folder_path != "." {
                if let Some(nf) =
                    folders.find_by_mount_point_and_path(mount_point_id, &new_folder_path)?
                {
                    new_folder_id = Some(nf.id);
                }
            }
            links_repo.update(
                &link.id,
                &LinkUpdate {
                    relative_path: Some(new_path.clone()),
                    file_name: Some(basename(&new_path).to_string()),
                    folder_id: Some(new_folder_id),
                    updated_at: now_iso(),
                    ..Default::default()
                },
            )?;
        }
    }

    Ok(())
}

// ============================================================================
// EXISTENCE / FOLDER SEMANTICS
// ============================================================================

/// v4 `databaseDocumentExists` (`database-store.ts:568`).
pub fn database_document_exists(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<bool, DbError> {
    let rel = normalise_relative_path(relative_path)?;
    Ok(DocMountDocumentsRepository::new(conn)
        .find_by_mount_point_and_path(mount_point_id, &rel)?
        .is_some())
}

/// v4 `databaseFolderExists` (`database-store.ts:578`).
pub fn database_folder_exists(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<bool, DbError> {
    let rel = normalise_relative_path(relative_path)?;
    Ok(DocMountFoldersRepository::new(conn)
        .find_by_mount_point_and_path(mount_point_id, &rel)?
        .is_some())
}

/// v4 `folderHasContents` (`folder-paths.ts:194`): true iff the folder has any
/// child folder OR any link (file/document) directly under it. Post-refactor,
/// folder membership lives on the link row (`folderId`), not on files/documents.
pub fn folder_has_contents(
    conn: &Connection,
    mount_point_id: &str,
    folder_id: &str,
) -> Result<bool, DbError> {
    // Child folders (parentId == folderId).
    let folders = DocMountFoldersRepository::new(conn).find_by_mount_point_id(mount_point_id)?;
    if folders
        .iter()
        .any(|f| f.parent_id.as_deref() == Some(folder_id))
    {
        return Ok(true);
    }

    // Any link directly under this folder (folderId == folderId).
    let links = DocMountFileLinksRepository::new(conn).find_by_mount_point_id(mount_point_id)?;
    Ok(links
        .iter()
        .any(|l| l.folder_id.as_deref() == Some(folder_id)))
}

// ============================================================================
// Path helpers (POSIX, over clean normalised relative paths)
// ============================================================================

/// POSIX `path.dirname` for a clean relative path: everything before the last
/// `/`, or `.` when there is none.
fn dirname(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((dir, _)) if !dir.is_empty() => dir.to_string(),
        Some((_, _)) => "/".to_string(),
        None => ".".to_string(),
    }
}

/// POSIX `path.basename` for a clean relative path: everything after the last `/`.
fn basename(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, name)) => name,
        None => path,
    }
}

/// JS `path.extname(rel).toLowerCase()` — the trailing `.ext` (WITH the dot),
/// lowercased; empty string when the basename has no dot (or a leading-dot-only
/// basename, which Node treats as having no extension). Used only for the
/// unsupported-extension error text, so it must match v4 byte-for-byte.
fn path_extname(rel: &str) -> String {
    let base = basename(rel);
    // Node: a leading dot (dotfile like `.env`) is NOT an extension.
    match base.rfind('.') {
        Some(idx) if idx > 0 => base[idx..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Minimal mount-index DDL covering the columns these ops touch (a faithful
    /// subset of v4's generateDDL output; the full byte-exact proof is a later
    /// differential). `foreign_keys` stays off — the primitives don't rely on FK
    /// cascade here (deleteWithGC deletes explicitly).
    fn open_store_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_files (
               id TEXT PRIMARY KEY, sha256 TEXT, fileSizeBytes INTEGER,
               fileType TEXT, source TEXT, createdAt TEXT, updatedAt TEXT);
             CREATE TABLE doc_mount_documents (
               id TEXT PRIMARY KEY, fileId TEXT, content TEXT, contentSha256 TEXT,
               plainTextLength INTEGER, createdAt TEXT, updatedAt TEXT);
             CREATE TABLE doc_mount_folders (
               id TEXT PRIMARY KEY, mountPointId TEXT, parentId TEXT, name TEXT,
               path TEXT, createdAt TEXT, updatedAt TEXT);
             CREATE TABLE doc_mount_file_links (
               id TEXT PRIMARY KEY, fileId TEXT, mountPointId TEXT, relativePath TEXT,
               fileName TEXT, folderId TEXT, originalFileName TEXT, originalMimeType TEXT,
               description TEXT DEFAULT '', descriptionUpdatedAt TEXT,
               conversionStatus TEXT, conversionError TEXT, plainTextLength INTEGER,
               extractedText TEXT, extractedTextSha256 TEXT, extractionStatus TEXT,
               extractionError TEXT, chunkCount INTEGER DEFAULT 0,
               allowEmbed INTEGER, allowCharacterRead INTEGER, allowCharacterWrite INTEGER,
               lastModified TEXT, createdAt TEXT, updatedAt TEXT);",
        )
        .unwrap();
        conn
    }

    const MP: &str = "mount-1";

    fn seed(conn: &Connection, rel: &str, content: &str) {
        write_database_document(conn, MP, rel, content).unwrap();
    }

    #[test]
    fn write_then_read_roundtrips() {
        let conn = open_store_db();
        seed(&conn, "notes.md", "hello");
        let doc = read_database_document(&conn, MP, "notes.md").unwrap();
        assert_eq!(doc.content, "hello");
        assert_eq!(doc.size, 5);
        assert!(doc.mtime_ms > 0);
    }

    #[test]
    fn read_missing_is_not_found_with_v4_message() {
        let conn = open_store_db();
        let err = read_database_document(&conn, MP, "missing.md").unwrap_err();
        match err {
            StoreError::Store(e) => {
                assert_eq!(e.code, DbStoreErrorCode::NotFound);
                assert_eq!(
                    e.message,
                    "Document not found in database-backed store: missing.md"
                );
            }
            other => panic!("expected store error, got {other}"),
        }
    }

    #[test]
    fn write_unsupported_extension_message_matches_v4() {
        let conn = open_store_db();
        let err = write_database_document(&conn, MP, "photo.yaml", "x").unwrap_err();
        match err {
            StoreError::Store(e) => {
                assert_eq!(e.code, DbStoreErrorCode::Unsupported);
                assert_eq!(
                    e.message,
                    "Database-backed stores only accept text documents (.md, .markdown, .txt, .json, .jsonl, .ndjson). Got: .yaml"
                );
            }
            other => panic!("expected store error, got {other}"),
        }
    }

    #[test]
    fn write_no_extension_reports_empty_ext() {
        let conn = open_store_db();
        let err = write_database_document(&conn, MP, "README", "x").unwrap_err();
        match err {
            StoreError::Store(e) => assert!(
                e.message.ends_with("Got: "),
                "expected empty extension, got: {}",
                e.message
            ),
            other => panic!("expected store error, got {other}"),
        }
    }

    #[test]
    fn exists_and_delete() {
        let conn = open_store_db();
        seed(&conn, "a.md", "content");
        assert!(database_document_exists(&conn, MP, "a.md").unwrap());
        assert!(delete_database_document(&conn, MP, "a.md").unwrap());
        assert!(!database_document_exists(&conn, MP, "a.md").unwrap());
        // Deleting again is the tolerant false.
        assert!(!delete_database_document(&conn, MP, "a.md").unwrap());
    }

    #[test]
    fn move_document_updates_path() {
        let conn = open_store_db();
        seed(&conn, "old.md", "body");
        move_database_document(&conn, MP, "old.md", "sub/new.md").unwrap();
        assert!(!database_document_exists(&conn, MP, "old.md").unwrap());
        let doc = read_database_document(&conn, MP, "sub/new.md").unwrap();
        assert_eq!(doc.content, "body");
        // The dest folder was created.
        assert!(database_folder_exists(&conn, MP, "sub").unwrap());
    }

    #[test]
    fn move_to_existing_target_conflicts() {
        let conn = open_store_db();
        seed(&conn, "a.md", "one");
        seed(&conn, "b.md", "two");
        let err = move_database_document(&conn, MP, "a.md", "b.md").unwrap_err();
        match err {
            StoreError::Store(e) => {
                assert_eq!(e.code, DbStoreErrorCode::Conflict);
                assert_eq!(e.message, "Target already exists: b.md");
            }
            other => panic!("expected store error, got {other}"),
        }
    }

    #[test]
    fn move_missing_source_not_found() {
        let conn = open_store_db();
        let err = move_database_document(&conn, MP, "nope.md", "there.md").unwrap_err();
        match err {
            StoreError::Store(e) => {
                assert_eq!(e.code, DbStoreErrorCode::NotFound);
                assert_eq!(e.message, "Source document not found: nope.md");
            }
            other => panic!("expected store error, got {other}"),
        }
    }

    #[test]
    fn list_root_and_filtered() {
        let conn = open_store_db();
        seed(&conn, "top.md", "t");
        seed(&conn, "dir/inner.md", "i");
        // Root listing: two files + one folder (dir), files first.
        let root = list_database_files(&conn, MP, None).unwrap();
        let files: Vec<_> = root.iter().filter(|e| e.kind == "file").collect();
        let dirs: Vec<_> = root.iter().filter(|e| e.kind == "folder").collect();
        assert_eq!(files.len(), 2);
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].relative_path, "dir");
        assert_eq!(dirs[0].file_type, "markdown");
        assert_eq!(dirs[0].sha256, "");
        // Files come before folders (v4 push order).
        assert_eq!(root[0].kind, "file");
        assert_eq!(root.last().unwrap().kind, "folder");
        // Filtered listing.
        let filtered = list_database_files(&conn, MP, Some("dir")).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].relative_path, "dir/inner.md");
    }

    #[test]
    fn folder_create_delete_and_not_empty_guard() {
        let conn = open_store_db();
        let (fid, path) = create_database_folder(&conn, MP, "Folder").unwrap();
        assert!(!fid.is_empty());
        assert_eq!(path, "Folder");
        assert!(database_folder_exists(&conn, MP, "Folder").unwrap());
        assert!(!folder_has_contents(&conn, MP, &fid).unwrap());
        // Empty folder deletes.
        delete_database_folder(&conn, MP, "Folder").unwrap();
        assert!(!database_folder_exists(&conn, MP, "Folder").unwrap());

        // Non-empty folder refuses with the v4 "not empty" message.
        seed(&conn, "Docs/x.md", "x");
        let err = delete_database_folder(&conn, MP, "Docs").unwrap_err();
        match err {
            StoreError::Store(e) => {
                assert_eq!(e.code, DbStoreErrorCode::NotEmpty);
                assert!(e.message.contains("not empty"), "{}", e.message);
                assert_eq!(
                    e.message,
                    "Folder is not empty: Docs. Only empty folders can be deleted."
                );
            }
            other => panic!("expected store error, got {other}"),
        }
    }

    #[test]
    fn delete_missing_folder_not_found() {
        let conn = open_store_db();
        let err = delete_database_folder(&conn, MP, "Ghost").unwrap_err();
        match err {
            StoreError::Store(e) => {
                assert_eq!(e.code, DbStoreErrorCode::NotFound);
                assert_eq!(e.message, "Folder not found: Ghost");
            }
            other => panic!("expected store error, got {other}"),
        }
    }

    #[test]
    fn move_folder_rewrites_descendants() {
        let conn = open_store_db();
        seed(&conn, "A/one.md", "1");
        seed(&conn, "A/sub/two.md", "2");
        move_database_folder(&conn, MP, "A", "B").unwrap();
        // Old paths gone, new paths present.
        assert!(!database_document_exists(&conn, MP, "A/one.md").unwrap());
        assert!(database_document_exists(&conn, MP, "B/one.md").unwrap());
        assert!(database_document_exists(&conn, MP, "B/sub/two.md").unwrap());
        assert!(database_folder_exists(&conn, MP, "B").unwrap());
        assert!(database_folder_exists(&conn, MP, "B/sub").unwrap());
    }
}
