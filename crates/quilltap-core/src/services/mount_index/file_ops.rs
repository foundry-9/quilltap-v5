//! Port of v4 `lib/mount-index/file-ops.ts` — mount-point file operations
//! (move / copy / link / write / delete across mounts).
//!
//! Database-backed mounts use `doc_mount_file_links` as the hard-link primitive
//! (one content row, many link rows). Filesystem-backed mounts use POSIX
//! `hard_link` / `rename` when source and destination sit on the same device;
//! everything else falls back to a byte copy. Every operation reports
//! `sourceSha256` and `destSha256` so the caller can verify the bytes survived
//! end-to-end.
//!
//! v4's `emitDocumentWritten`/`emitDocumentDeleted`/`emitDocumentMoved` events
//! feed only the watcher's debounced embedding refresh — part of the
//! chokidar-equivalent watcher deferral (loud in the P4.6y status header), so
//! the emit sites here are annotated but not wired.

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::{
    sha256_of_string, DocMountFileLinksRepository, LinkBlobInput, LinkDocumentInput, LinkUpdate,
};
use crate::db::doc_mount_points::MountServiceInfo;
use crate::db::DbError;

use super::converters::DocumentTextExtractor;
use super::file_op_error::{FileOpError, FileOpErrorCode};
use super::path_utils::{
    detect_native_text, mime_for_extension, normalise_relative_path, path_extname,
    path_posix_resolve,
};
use super::read_file::MountFileError;
use super::reindex_file::node_basename;
use super::scanner::process_mount_file;

/// v4 `resolveFsAbsolute`: resolve a filesystem-mount relative path to an
/// absolute path, refusing any result that escapes the mount's `basePath`.
///
/// The guard mirrors v4 exactly: `abs !== base && !abs.startsWith(base + sep)`.
/// `base_path` is `None` when the mount has no `basePath` configured (v4's
/// `if (!mp.basePath)` falsy check).
pub fn resolve_fs_absolute(
    base_path: Option<&str>,
    mount_id: &str,
    relative_path: &str,
) -> Result<String, FileOpError> {
    let base_path = match base_path {
        Some(b) if !b.is_empty() => b,
        _ => {
            return Err(FileOpError::new(
                format!("Filesystem mount has no basePath configured: {mount_id}"),
                FileOpErrorCode::InvalidPath,
            ))
        }
    };
    let abs = path_posix_resolve(&[base_path, relative_path]);
    let base = path_posix_resolve(&[base_path]);
    let base_with_sep = if base.ends_with('/') {
        base.clone()
    } else {
        format!("{base}/")
    };
    if abs != base && !abs.starts_with(&base_with_sep) {
        return Err(FileOpError::new(
            format!("Path escapes mount boundary: {relative_path}"),
            FileOpErrorCode::InvalidPath,
        ));
    }
    Ok(abs)
}

// ============================================================================
// Types
// ============================================================================

/// v4 `FileOpStrategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOpStrategy {
    DbLink,
    FsLink,
    Rename,
    ByteCopy,
}

impl FileOpStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            FileOpStrategy::DbLink => "db-link",
            FileOpStrategy::FsLink => "fs-link",
            FileOpStrategy::Rename => "rename",
            FileOpStrategy::ByteCopy => "byte-copy",
        }
    }
}

/// v4 `FileOpResult` — contract surface (the SVAR adapter keys on
/// `strategy === 'byte-copy'` + the sha pair); serialized faithfully.
#[derive(Debug, Clone)]
pub struct FileOpResult {
    pub strategy: FileOpStrategy,
    pub source_sha256: String,
    pub dest_sha256: String,
    pub size_bytes: i64,
    pub source_path: String,
    pub dest_path: String,
    pub source_mount_point_id: String,
    pub dest_mount_point_id: String,
}

impl FileOpResult {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "strategy": self.strategy.as_str(),
            "sourceSha256": self.source_sha256,
            "destSha256": self.dest_sha256,
            "sizeBytes": self.size_bytes,
            "sourcePath": self.source_path,
            "destPath": self.dest_path,
            "sourceMountPointId": self.source_mount_point_id,
            "destMountPointId": self.dest_mount_point_id,
        })
    }
}

/// v4 `CopyOpts` (`MoveOpts` = the same minus `force`).
#[derive(Debug, Clone)]
pub struct CopyOpts {
    pub source_mount_point_id: String,
    pub source_path: String,
    pub dest_mount_point_id: String,
    pub dest_path: String,
    /// Copy only: overwrite an existing destination AND skip hard-link.
    pub force: bool,
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn load_mount(conn: &Connection, mount_point_id: &str) -> Result<MountServiceInfo, MountFileError> {
    let mp = crate::db::doc_mount_points::DocMountPointsRepository::new(conn)
        .find_service_info_by_id(mount_point_id)?;
    mp.ok_or_else(|| {
        MountFileError::FileOp(FileOpError::new(
            format!("Mount point not found: {mount_point_id}"),
            FileOpErrorCode::MountNotFound,
        ))
    })
}

/// The source-existence probe result (v4 `sourceExistsOrThrow`).
struct SourceInfo {
    absolute_path: Option<String>,
    link_id: Option<String>,
    file_id: Option<String>,
    sha256: String,
    size_bytes: i64,
}

fn source_exists_or_throw(
    conn: &Connection,
    mp: &MountServiceInfo,
    relative_path: &str,
) -> Result<SourceInfo, MountFileError> {
    let links = DocMountFileLinksRepository::new(conn);
    if let Some(link) = links.find_by_mount_point_and_path(&mp.id, relative_path)? {
        return Ok(SourceInfo {
            absolute_path: if mp.is_filesystem_mount() {
                Some(resolve_fs_absolute(
                    mp.base_path.as_deref(),
                    &mp.id,
                    relative_path,
                )?)
            } else {
                None
            },
            link_id: Some(link.id),
            file_id: Some(link.file_id),
            sha256: link.sha256,
            size_bytes: link.file_size_bytes,
        });
    }
    // Filesystem source may not yet be indexed (dropped on disk between scans):
    // tolerate by computing the sha on the fly.
    if mp.is_filesystem_mount() {
        let abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, relative_path)?;
        return match std::fs::metadata(&abs) {
            Ok(meta) if meta.is_file() => {
                let bytes =
                    std::fs::read(&abs).map_err(|e| MountFileError::Other(e.to_string()))?;
                Ok(SourceInfo {
                    absolute_path: Some(abs),
                    link_id: None,
                    file_id: None,
                    sha256: sha256_hex(&bytes),
                    size_bytes: meta.len() as i64,
                })
            }
            Ok(_) => Err(MountFileError::FileOp(FileOpError::new(
                format!("Source is not a file: {relative_path}"),
                FileOpErrorCode::SourceNotFound,
            ))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(MountFileError::FileOp(FileOpError::new(
                    format!("Source not found: {relative_path}"),
                    FileOpErrorCode::SourceNotFound,
                )))
            }
            Err(e) => Err(MountFileError::Other(e.to_string())),
        };
    }
    Err(MountFileError::FileOp(FileOpError::new(
        format!("Source not found: {relative_path}"),
        FileOpErrorCode::SourceNotFound,
    )))
}

/// v4 `destExists`.
pub fn dest_exists(
    conn: &Connection,
    mp: &MountServiceInfo,
    relative_path: &str,
) -> Result<bool, MountFileError> {
    let links = DocMountFileLinksRepository::new(conn);
    if links
        .find_by_mount_point_and_path(&mp.id, relative_path)?
        .is_some()
    {
        return Ok(true);
    }
    if mp.is_filesystem_mount() {
        let abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, relative_path)?;
        return Ok(std::path::Path::new(&abs).exists());
    }
    Ok(false)
}

/// v4 `readSourceBytes` — pull the source content across storage shapes.
fn read_source_bytes(
    conn: &Connection,
    mp: &MountServiceInfo,
    relative_path: &str,
    info: &SourceInfo,
) -> Result<Vec<u8>, MountFileError> {
    if mp.is_filesystem_mount() {
        let abs = match &info.absolute_path {
            Some(a) => a.clone(),
            None => resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, relative_path)?,
        };
        return std::fs::read(&abs).map_err(|e| MountFileError::Other(e.to_string()));
    }
    if let Some(file_id) = &info.file_id {
        if let Some(content) =
            DocMountDocumentsRepository::new(conn).find_content_by_file_id(file_id)?
        {
            return Ok(content.into_bytes());
        }
        if let Some(bytes) = crate::db::doc_mount_blobs::DocMountBlobsRepository::new(conn)
            .read_data_by_file_id(file_id)?
        {
            return Ok(bytes);
        }
    }
    Err(MountFileError::FileOp(FileOpError::new(
        format!("Source content missing: {relative_path}"),
        FileOpErrorCode::SourceNotFound,
    )))
}

/// v4 `insertLinkRow` — a new `doc_mount_file_links` row pointing at an
/// existing fileId (the db hard-link primitive).
#[allow(clippy::too_many_arguments)]
fn insert_link_row(
    conn: &Connection,
    file_id: &str,
    file_type: &str,
    mount_point_id: &str,
    relative_path: &str,
    file_name: &str,
    folder_id: Option<&str>,
    original_file_name: Option<&str>,
    original_mime_type: Option<&str>,
    description: &str,
    conversion_status: Option<&str>,
    plain_text_length: Option<f64>,
) -> Result<String, DbError> {
    let now = crate::clock::now_iso();
    let link_id = uuid::Uuid::new_v4().to_string();
    let conversion_status = conversion_status.map(|c| c.to_string()).unwrap_or_else(|| {
        if file_type == "blob" {
            "skipped".to_string()
        } else if file_type == "pdf" || file_type == "docx" {
            "pending".to_string()
        } else {
            "converted".to_string()
        }
    });
    conn.execute(
        "INSERT INTO doc_mount_file_links (\
           id, fileId, mountPointId, relativePath, fileName, folderId, \
           originalFileName, originalMimeType, \
           description, descriptionUpdatedAt, \
           conversionStatus, conversionError, plainTextLength, \
           extractedText, extractedTextSha256, extractionStatus, extractionError, \
           chunkCount, lastModified, createdAt, updatedAt\
         ) VALUES (\
           ?1, ?2, ?3, ?4, ?5, ?6, \
           ?7, ?8, \
           ?9, NULL, \
           ?10, NULL, ?11, \
           NULL, NULL, 'none', NULL, \
           0, ?12, ?13, ?14\
         )",
        rusqlite::params![
            link_id,
            file_id,
            mount_point_id,
            relative_path,
            file_name,
            folder_id,
            original_file_name,
            original_mime_type,
            description,
            conversion_status,
            plain_text_length,
            now, // lastModified (v4 `input.lastModified ?? now`)
            now,
            now
        ],
    )?;
    Ok(link_id)
}

/// v4 `computeDestSha256` — the post-op verification read.
fn compute_dest_sha256(
    conn: &Connection,
    mp: &MountServiceInfo,
    relative_path: &str,
) -> Result<String, MountFileError> {
    if mp.is_filesystem_mount() {
        let abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, relative_path)?;
        let bytes = std::fs::read(&abs).map_err(|e| MountFileError::Other(e.to_string()))?;
        return Ok(sha256_hex(&bytes));
    }
    let link = DocMountFileLinksRepository::new(conn)
        .find_by_mount_point_and_path(&mp.id, relative_path)?;
    match link {
        Some(l) => Ok(l.sha256),
        None => Err(MountFileError::FileOp(FileOpError::new(
            format!("Destination not visible after write: {relative_path}"),
            FileOpErrorCode::VerifyFailed,
        ))),
    }
}

// ============================================================================
// Internal writers / deleters (shared with store-file)
// ============================================================================

/// v4 `writeFsFileBytes` — write raw bytes to a filesystem mount and
/// (best-effort) index the result. Does NOT transcode.
pub fn write_fs_file_bytes(
    conn: &Connection,
    mp: &MountServiceInfo,
    relative_path: &str,
    bytes: &[u8],
    extractor: &dyn DocumentTextExtractor,
) -> Result<(String, i64, i64), MountFileError> {
    let abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, relative_path)?;
    if let Some(parent) = std::path::Path::new(&abs).parent() {
        std::fs::create_dir_all(parent).map_err(|e| MountFileError::Other(e.to_string()))?;
    }
    std::fs::write(&abs, bytes).map_err(|e| MountFileError::Other(e.to_string()))?;
    if let Err(e) = process_mount_file(conn, mp, &abs, relative_path, extractor) {
        eprintln!(
            "writeFsFileBytes: processMountFile failed for {relative_path} (mount {}): {e}",
            mp.id
        );
    }
    let meta = std::fs::metadata(&abs).map_err(|e| MountFileError::Other(e.to_string()))?;
    let reread = std::fs::read(&abs).map_err(|e| MountFileError::Other(e.to_string()))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Ok((sha256_hex(&reread), reread.len() as i64, mtime))
}

/// v4 `writeDestBytes` — the byte-preserving cross-storage writer.
fn write_dest_bytes(
    conn: &Connection,
    dest_mount: &MountServiceInfo,
    dest_rel: &str,
    bytes: &[u8],
    extractor: &dyn DocumentTextExtractor,
) -> Result<String, MountFileError> {
    let sha = sha256_hex(bytes);
    let dest_file_name = node_basename(dest_rel).to_string();

    if dest_mount.is_filesystem_mount() {
        return Ok(write_fs_file_bytes(conn, dest_mount, dest_rel, bytes, extractor)?.0);
    }

    let links = DocMountFileLinksRepository::new(conn);
    if let Some(native) = detect_native_text(dest_rel) {
        // v4 `bytes.toString('utf-8')` — lossy.
        let text = String::from_utf8_lossy(bytes).into_owned();
        let content_sha = sha256_of_string(&text);
        links.link_document_content(&LinkDocumentInput {
            mount_point_id: dest_mount.id.clone(),
            relative_path: dest_rel.to_string(),
            file_name: dest_file_name,
            file_type: native.as_str().to_string(),
            content: text.clone(),
            content_sha256: content_sha.clone(),
            plain_text_length: crate::jsstr::utf16_len(&text) as i64,
            file_size_bytes: text.len() as i64,
            allow_embed: None,
            allow_character_read: None,
            allow_character_write: None,
        })?;
        // v4 emitDocumentWritten — the watcher deferral (see module header).
        return Ok(content_sha);
    }

    // Binary path: linkBlobContent stores bytes verbatim.
    let ext = path_extname(dest_rel).to_lowercase();
    let file_type = if ext == ".pdf" {
        "pdf"
    } else if ext == ".docx" {
        "docx"
    } else {
        "blob"
    };
    let original_mime = mime_for_extension(dest_rel).to_string();
    links.link_blob_content(&LinkBlobInput {
        mount_point_id: dest_mount.id.clone(),
        relative_path: dest_rel.to_string(),
        file_name: dest_file_name.clone(),
        file_type: Some(file_type.to_string()),
        original_file_name: dest_file_name,
        original_mime_type: original_mime.clone(),
        stored_mime_type: original_mime,
        sha256: sha.clone(),
        data: bytes.to_vec(),
        description: None,
        conversion_status: None,
        extracted_text: None,
        extracted_text_sha256: None,
        extraction_status: None,
    })?;
    // v4 emitDocumentWritten — the watcher deferral.
    Ok(sha)
}

/// v4 `deleteAtSource`.
fn delete_at_source(
    conn: &Connection,
    mp: &MountServiceInfo,
    _relative_path: &str,
    info: &SourceInfo,
) -> Result<(), MountFileError> {
    let links = DocMountFileLinksRepository::new(conn);
    if mp.is_filesystem_mount() {
        if let Some(abs) = &info.absolute_path {
            match std::fs::remove_file(abs) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(MountFileError::Other(e.to_string())),
            }
        }
        if let Some(link_id) = &info.link_id {
            links.delete_with_gc(link_id)?;
        }
        return Ok(());
    }
    if let Some(link_id) = &info.link_id {
        links.delete_with_gc(link_id)?;
        // v4 emitDocumentDeleted — the watcher deferral.
    }
    Ok(())
}

/// v4 `deleteAtDest` (shared with store-file's force-overwrite arm).
pub fn delete_at_dest(
    conn: &Connection,
    mp: &MountServiceInfo,
    relative_path: &str,
) -> Result<(), MountFileError> {
    let links = DocMountFileLinksRepository::new(conn);
    if let Some(link) = links.find_by_mount_point_and_path(&mp.id, relative_path)? {
        links.delete_with_gc(&link.id)?;
    }
    if mp.is_filesystem_mount() {
        let abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, relative_path)?;
        match std::fs::remove_file(&abs) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(MountFileError::Other(e.to_string())),
        }
    }
    // else: v4 emitDocumentDeleted — the watcher deferral.
    Ok(())
}

/// v4 `hardLinkDbToDb` — a second link row sharing the source's fileId,
/// preserving the source link's metadata. Returns the dest link id it minted.
///
/// `bind_group` is what separates `link` from `copy` (v4 `40319484`). Both end
/// up sharing a `fileId` — the store is content-addressed, so identical bytes
/// always land on one row — but only a linked pair is enrolled in a hard-link
/// group, and only a group survives a write: writing through a grouped link
/// repoints every member, while writing through a merely-deduped copy forks it
/// onto its own content row. Without that distinction a `copy` would inherit
/// edits from its original, and two unrelated files with identical bytes would
/// rewrite each other.
fn hard_link_db_to_db(
    conn: &Connection,
    source_file_id: &str,
    source_link_id: &str,
    dest_mount_point_id: &str,
    dest_relative_path: &str,
    bind_group: bool,
) -> Result<String, MountFileError> {
    let links = DocMountFileLinksRepository::new(conn);
    let source_link = links.find_by_id_with_content(source_link_id)?;
    let Some(source_link) = source_link else {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Source link disappeared: {source_link_id}"),
            FileOpErrorCode::SourceNotFound,
        )));
    };
    let dest_dir = node_dirname(dest_relative_path);
    let dest_folder_id = if dest_dir != "." {
        links.ensure_folder_path(dest_mount_point_id, dest_dir)?
    } else {
        None
    };
    let dest_link_id = insert_link_row(
        conn,
        source_file_id,
        &source_link.file_type,
        dest_mount_point_id,
        dest_relative_path,
        node_basename(dest_relative_path),
        dest_folder_id.as_deref(),
        source_link.original_file_name.as_deref(),
        source_link.original_mime_type.as_deref(),
        source_link.description.as_deref().unwrap_or(""),
        Some(&source_link.conversion_status),
        source_link.plain_text_length,
    )?;
    if bind_group {
        // Warn, never fail — v4 wraps `bindLinkGroup` in `safeQuery` (log +
        // null), so once the dest link row exists a bind failure can only
        // leave the pair ungrouped, never fail the link the caller was told
        // about.
        if let Err(e) = links.bind_link_group(source_link_id, &dest_link_id) {
            eprintln!(
                "bindLinkGroup after db link failed for {dest_relative_path} \
                 (mount {dest_mount_point_id}): {e}"
            );
        }
    }
    // v4 emitDocumentWritten — the watcher deferral.
    Ok(dest_link_id)
}

/// v4 `updateLinkLocation` — the db→db move (same fileId, new location).
fn update_link_location(
    conn: &Connection,
    link_id: &str,
    dest_mount_point_id: &str,
    dest_relative_path: &str,
) -> Result<(), MountFileError> {
    let links = DocMountFileLinksRepository::new(conn);
    let dest_dir = node_dirname(dest_relative_path);
    let dest_folder_id = if dest_dir != "." {
        links.ensure_folder_path(dest_mount_point_id, dest_dir)?
    } else {
        None
    };
    links.update(
        link_id,
        &LinkUpdate {
            mount_point_id: Some(dest_mount_point_id.to_string()),
            relative_path: Some(dest_relative_path.to_string()),
            file_name: Some(node_basename(dest_relative_path).to_string()),
            folder_id: Some(dest_folder_id),
            updated_at: crate::clock::now_iso(),
            ..Default::default()
        },
    )?;
    Ok(())
}

/// Node `path.posix.dirname` for the already-normalised relative paths here.
pub(crate) fn node_dirname(p: &str) -> &str {
    match p.rfind('/') {
        Some(0) => "/",
        Some(i) => &p[..i],
        None => ".",
    }
}

// ============================================================================
// Public API: copy / move / link / write / delete
// ============================================================================

/// v4 `copyFile`.
pub fn copy_file(
    conn: &Connection,
    opts: &CopyOpts,
    extractor: &dyn DocumentTextExtractor,
) -> Result<FileOpResult, MountFileError> {
    let source_mount = load_mount(conn, &opts.source_mount_point_id)?;
    let dest_mount = load_mount(conn, &opts.dest_mount_point_id)?;
    let source_rel = normalise_relative_path(&opts.source_path)?;
    let dest_rel = normalise_relative_path(&opts.dest_path)?;

    let source_info = source_exists_or_throw(conn, &source_mount, &source_rel)?;

    // Same mount and same path is a no-op; reject so the caller doesn't
    // accidentally garbage-collect both ends in the move flow. Case-insensitive
    // (paths are one namespace regardless of casing) and BEFORE the force-
    // overwrite branch — force-copying onto a case-variant of the source would
    // otherwise delete the source itself.
    if source_mount.id == dest_mount.id && source_rel.to_lowercase() == dest_rel.to_lowercase() {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Source and destination are the same path: {dest_rel}"),
            FileOpErrorCode::InvalidPath,
        )));
    }

    if dest_exists(conn, &dest_mount, &dest_rel)? {
        if !opts.force {
            return Err(MountFileError::FileOp(FileOpError::new(
                format!("Destination already exists: {dest_rel}. Use --force to overwrite."),
                FileOpErrorCode::DestExists,
            )));
        }
        delete_at_dest(conn, &dest_mount, &dest_rel)?;
    }

    let source_is_fs = source_mount.is_filesystem_mount();
    let dest_is_fs = dest_mount.is_filesystem_mount();

    let strategy: FileOpStrategy;
    let dest_sha: String;
    let mut size_bytes = source_info.size_bytes;

    if !source_is_fs && !dest_is_fs {
        // DB -> DB: hard-link (force = true byte rewrite; content-addressed, so
        // the observable end-state matches either way).
        let Some(file_id) = source_info.file_id.clone() else {
            return Err(MountFileError::FileOp(FileOpError::new(
                format!("Source file has no content row: {source_rel}"),
                FileOpErrorCode::SourceNotFound,
            )));
        };
        if opts.force {
            let bytes = read_source_bytes(conn, &source_mount, &source_rel, &source_info)?;
            dest_sha = write_dest_bytes(conn, &dest_mount, &dest_rel, &bytes, extractor)?;
            strategy = FileOpStrategy::ByteCopy;
        } else {
            // No group: a copy is an independent file that merely starts out
            // sharing a deduped content row. The first write to either side
            // forks it (v4 `40319484`).
            hard_link_db_to_db(
                conn,
                &file_id,
                source_info
                    .link_id
                    .as_deref()
                    .expect("link id with file id"),
                &dest_mount.id,
                &dest_rel,
                false,
            )?;
            dest_sha = source_info.sha256.clone();
            strategy = FileOpStrategy::DbLink;
        }
    } else if source_is_fs && dest_is_fs {
        // FS -> FS: hard link when same device and !force; else byte copy.
        let source_abs = source_info.absolute_path.clone().expect("fs source abs");
        let dest_abs =
            resolve_fs_absolute(dest_mount.base_path.as_deref(), &dest_mount.id, &dest_rel)?;
        if let Some(parent) = std::path::Path::new(&dest_abs).parent() {
            std::fs::create_dir_all(parent).map_err(|e| MountFileError::Other(e.to_string()))?;
        }
        let mut linked = false;
        if !opts.force {
            match std::fs::hard_link(&source_abs, &dest_abs) {
                Ok(()) => linked = true,
                Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {}
                Err(e) => return Err(MountFileError::Other(e.to_string())),
            }
        }
        if !linked {
            std::fs::copy(&source_abs, &dest_abs)
                .map_err(|e| MountFileError::Other(e.to_string()))?;
        }
        // Index the new path so search/list pick it up (best-effort).
        if let Err(e) = process_mount_file(conn, &dest_mount, &dest_abs, &dest_rel, extractor) {
            eprintln!(
                "processMountFile after fs copy failed for {dest_rel} (mount {}): {e}",
                dest_mount.id
            );
        }
        let dest_bytes =
            std::fs::read(&dest_abs).map_err(|e| MountFileError::Other(e.to_string()))?;
        dest_sha = sha256_hex(&dest_bytes);
        size_bytes = dest_bytes.len() as i64;
        strategy = if linked {
            FileOpStrategy::FsLink
        } else {
            FileOpStrategy::ByteCopy
        };
    } else {
        // Cross-storage: byte copy through the appropriate writer.
        let bytes = read_source_bytes(conn, &source_mount, &source_rel, &source_info)?;
        dest_sha = write_dest_bytes(conn, &dest_mount, &dest_rel, &bytes, extractor)?;
        size_bytes = bytes.len() as i64;
        strategy = FileOpStrategy::ByteCopy;
    }

    if dest_sha != source_info.sha256 {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!(
                "Checksum mismatch after copy: source {} != dest {dest_sha}",
                source_info.sha256
            ),
            FileOpErrorCode::VerifyFailed,
        )));
    }

    Ok(FileOpResult {
        strategy,
        source_sha256: source_info.sha256,
        dest_sha256: dest_sha,
        size_bytes,
        source_path: source_rel,
        dest_path: dest_rel,
        source_mount_point_id: source_mount.id,
        dest_mount_point_id: dest_mount.id,
    })
}

/// v4 `moveFile`.
pub fn move_file(
    conn: &Connection,
    source_mount_point_id: &str,
    source_path: &str,
    dest_mount_point_id: &str,
    dest_path: &str,
    extractor: &dyn DocumentTextExtractor,
) -> Result<FileOpResult, MountFileError> {
    let source_mount = load_mount(conn, source_mount_point_id)?;
    let dest_mount = load_mount(conn, dest_mount_point_id)?;
    let source_rel = normalise_relative_path(source_path)?;
    let dest_rel = normalise_relative_path(dest_path)?;

    if source_mount.id == dest_mount.id && source_rel == dest_rel {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Source and destination are the same path: {dest_rel}"),
            FileOpErrorCode::InvalidPath,
        )));
    }

    let source_info = source_exists_or_throw(conn, &source_mount, &source_rel)?;

    // A case-only rename (notes.md → Notes.md on the same mount) is legal: the
    // case-insensitive existence check would find the source itself, so skip it.
    // The exact-equal guard above still rejects true no-ops.
    let case_only_rename =
        source_mount.id == dest_mount.id && source_rel.to_lowercase() == dest_rel.to_lowercase();
    if !case_only_rename && dest_exists(conn, &dest_mount, &dest_rel)? {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Destination already exists: {dest_rel}. Move will not overwrite."),
            FileOpErrorCode::DestExists,
        )));
    }

    let source_is_fs = source_mount.is_filesystem_mount();
    let dest_is_fs = dest_mount.is_filesystem_mount();

    let strategy: FileOpStrategy;
    let dest_sha: String;
    let mut size_bytes = source_info.size_bytes;

    if !source_is_fs && !dest_is_fs {
        // DB -> DB: update the link row in place — same fileId, new location.
        let (Some(link_id), Some(_)) = (&source_info.link_id, &source_info.file_id) else {
            return Err(MountFileError::FileOp(FileOpError::new(
                format!("Source file has no link row: {source_rel}"),
                FileOpErrorCode::SourceNotFound,
            )));
        };
        update_link_location(conn, link_id, &dest_mount.id, &dest_rel)?;
        dest_sha = source_info.sha256.clone();
        strategy = FileOpStrategy::DbLink;
        // v4 emitDocumentMoved — the watcher deferral.
    } else if source_is_fs && dest_is_fs {
        let source_abs = source_info.absolute_path.clone().expect("fs source abs");
        let dest_abs =
            resolve_fs_absolute(dest_mount.base_path.as_deref(), &dest_mount.id, &dest_rel)?;
        if let Some(parent) = std::path::Path::new(&dest_abs).parent() {
            std::fs::create_dir_all(parent).map_err(|e| MountFileError::Other(e.to_string()))?;
        }
        let mut renamed = false;
        match std::fs::rename(&source_abs, &dest_abs) {
            Ok(()) => renamed = true,
            Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                std::fs::copy(&source_abs, &dest_abs)
                    .map_err(|e| MountFileError::Other(e.to_string()))?;
                std::fs::remove_file(&source_abs)
                    .map_err(|e| MountFileError::Other(e.to_string()))?;
            }
            Err(e) => return Err(MountFileError::Other(e.to_string())),
        }
        // Remove source from the index, re-index the dest path.
        if let Some(link_id) = &source_info.link_id {
            DocMountFileLinksRepository::new(conn).delete_with_gc(link_id)?;
        }
        if let Err(e) = process_mount_file(conn, &dest_mount, &dest_abs, &dest_rel, extractor) {
            eprintln!(
                "processMountFile after fs move failed for {dest_rel} (mount {}): {e}",
                dest_mount.id
            );
        }
        let dest_bytes =
            std::fs::read(&dest_abs).map_err(|e| MountFileError::Other(e.to_string()))?;
        dest_sha = sha256_hex(&dest_bytes);
        size_bytes = dest_bytes.len() as i64;
        strategy = if renamed {
            FileOpStrategy::Rename
        } else {
            FileOpStrategy::ByteCopy
        };
    } else {
        // Cross-storage move = copy then delete source.
        let bytes = read_source_bytes(conn, &source_mount, &source_rel, &source_info)?;
        dest_sha = write_dest_bytes(conn, &dest_mount, &dest_rel, &bytes, extractor)?;
        size_bytes = bytes.len() as i64;
        delete_at_source(conn, &source_mount, &source_rel, &source_info)?;
        strategy = FileOpStrategy::ByteCopy;
    }

    if dest_sha != source_info.sha256 {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!(
                "Checksum mismatch after move: source {} != dest {dest_sha}",
                source_info.sha256
            ),
            FileOpErrorCode::VerifyFailed,
        )));
    }

    Ok(FileOpResult {
        strategy,
        source_sha256: source_info.sha256,
        dest_sha256: dest_sha,
        size_bytes,
        source_path: source_rel,
        dest_path: dest_rel,
        source_mount_point_id: source_mount.id,
        dest_mount_point_id: dest_mount.id,
    })
}

/// v4 `linkFile` — a true hard link (NO copy). Cross-storage and cross-device
/// raise `UNSUPPORTED` rather than silently degrading — that distinction is the
/// whole point of `link` versus `copy`. Never overwrites.
pub fn link_file(
    conn: &Connection,
    source_mount_point_id: &str,
    source_path: &str,
    dest_mount_point_id: &str,
    dest_path: &str,
    extractor: &dyn DocumentTextExtractor,
) -> Result<FileOpResult, MountFileError> {
    let source_mount = load_mount(conn, source_mount_point_id)?;
    let dest_mount = load_mount(conn, dest_mount_point_id)?;
    let source_rel = normalise_relative_path(source_path)?;
    let dest_rel = normalise_relative_path(dest_path)?;

    if source_mount.id == dest_mount.id && source_rel == dest_rel {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Source and destination are the same path: {dest_rel}"),
            FileOpErrorCode::InvalidPath,
        )));
    }

    let source_info = source_exists_or_throw(conn, &source_mount, &source_rel)?;

    if dest_exists(conn, &dest_mount, &dest_rel)? {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Destination already exists: {dest_rel}. Link will not overwrite."),
            FileOpErrorCode::DestExists,
        )));
    }

    let source_is_fs = source_mount.is_filesystem_mount();
    let dest_is_fs = dest_mount.is_filesystem_mount();

    let strategy: FileOpStrategy;

    if !source_is_fs && !dest_is_fs {
        let (Some(file_id), Some(link_id)) = (&source_info.file_id, &source_info.link_id) else {
            return Err(MountFileError::FileOp(FileOpError::new(
                format!("Source file has no content row: {source_rel}"),
                FileOpErrorCode::SourceNotFound,
            )));
        };
        hard_link_db_to_db(conn, file_id, link_id, &dest_mount.id, &dest_rel, true)?;
        strategy = FileOpStrategy::DbLink;
    } else if source_is_fs && dest_is_fs {
        let source_abs = source_info.absolute_path.clone().expect("fs source abs");
        let dest_abs =
            resolve_fs_absolute(dest_mount.base_path.as_deref(), &dest_mount.id, &dest_rel)?;
        if let Some(parent) = std::path::Path::new(&dest_abs).parent() {
            std::fs::create_dir_all(parent).map_err(|e| MountFileError::Other(e.to_string()))?;
        }
        match std::fs::hard_link(&source_abs, &dest_abs) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                return Err(MountFileError::FileOp(FileOpError::new(
                    format!(
                        "Cannot hard-link across devices: {source_rel} → {dest_rel}. Use copy instead."
                    ),
                    FileOpErrorCode::Unsupported,
                )));
            }
            Err(e) => return Err(MountFileError::Other(e.to_string())),
        }
        if let Err(e) = process_mount_file(conn, &dest_mount, &dest_abs, &dest_rel, extractor) {
            eprintln!(
                "processMountFile after fs link failed for {dest_rel} (mount {}): {e}",
                dest_mount.id
            );
        }
        // The OS already shares the bytes through the inode, so no content
        // fan-out is needed here — but the index doesn't know the two paths are
        // the same file, and a write through one would leave the other's row
        // (sha, size, chunks) stale. Record the group so the sibling gets
        // re-indexed (v4 `40319484`). Warn, never fail: the link itself
        // succeeded.
        let links = DocMountFileLinksRepository::new(conn);
        // Both legs warn-and-continue on a DB error too: v4 wraps the lookup
        // AND the bind in `safeQuery` (log + null), so after the OS hard link
        // succeeds v4 structurally cannot fail this arm — worst case is an
        // ungrouped link and a log line.
        let dest_link = links
            .find_by_mount_point_and_path(&dest_mount.id, &dest_rel)
            .unwrap_or_else(|e| {
                eprintln!(
                    "looking up the fs-linked dest row failed for {dest_rel} (mount {}): {e}",
                    dest_mount.id
                );
                None
            });
        match (source_info.link_id.as_deref(), dest_link) {
            (Some(source_link_id), Some(dest_link)) => {
                if let Err(e) = links.bind_link_group(source_link_id, &dest_link.id) {
                    eprintln!(
                        "bindLinkGroup after fs link failed for {dest_rel} (mount {}): {e}",
                        dest_mount.id
                    );
                }
            }
            _ => eprintln!(
                "fs link created but could not be bound into a link group: \
                 {}:{source_rel} → {}:{dest_rel}",
                source_mount.id, dest_mount.id
            ),
        }
        strategy = FileOpStrategy::FsLink;
    } else {
        return Err(MountFileError::FileOp(FileOpError::new(
            "Cannot hard-link across storage types (filesystem ↔ database). Use copy instead."
                .to_string(),
            FileOpErrorCode::Unsupported,
        )));
    }

    let dest_sha = compute_dest_sha256(conn, &dest_mount, &dest_rel)?;
    if dest_sha != source_info.sha256 {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!(
                "Checksum mismatch after link: source {} != dest {dest_sha}",
                source_info.sha256
            ),
            FileOpErrorCode::VerifyFailed,
        )));
    }

    Ok(FileOpResult {
        strategy,
        source_sha256: source_info.sha256,
        dest_sha256: dest_sha,
        size_bytes: source_info.size_bytes,
        source_path: source_rel,
        dest_path: dest_rel,
        source_mount_point_id: source_mount.id,
        dest_mount_point_id: dest_mount.id,
    })
}

/// v4 `writeFile` (the `?action=write-file` byte-preserving writer — distinct
/// from the transcoding `storeMountFile` ingest).
pub fn write_file(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    data: &[u8],
    force: bool,
    extractor: &dyn DocumentTextExtractor,
) -> Result<serde_json::Value, MountFileError> {
    let mp = load_mount(conn, mount_point_id)?;
    let rel = normalise_relative_path(relative_path)?;

    if dest_exists(conn, &mp, &rel)? {
        if !force {
            return Err(MountFileError::FileOp(FileOpError::new(
                format!("Destination already exists: {rel}. Use --force to overwrite."),
                FileOpErrorCode::DestExists,
            )));
        }
        delete_at_dest(conn, &mp, &rel)?;
    }

    let source_sha = sha256_hex(data);
    let dest_sha = write_dest_bytes(conn, &mp, &rel, data, extractor)?;
    if dest_sha != source_sha {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Checksum mismatch after write: source {source_sha} != dest {dest_sha}"),
            FileOpErrorCode::VerifyFailed,
        )));
    }
    Ok(serde_json::json!({
        "sha256": dest_sha,
        "sizeBytes": data.len(),
        "destPath": rel,
        "mountPointId": mp.id,
    }))
}

/// v4 `deleteFile` — `{deleted, mountPointId, path}`; verifies the path is
/// really gone (`VERIFY_FAILED` otherwise).
pub fn delete_file(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<serde_json::Value, MountFileError> {
    let mp = load_mount(conn, mount_point_id)?;
    let rel = normalise_relative_path(relative_path)?;
    let existed = dest_exists(conn, &mp, &rel)?;
    if !existed {
        return Ok(serde_json::json!({
            "deleted": false,
            "mountPointId": mp.id,
            "path": rel,
        }));
    }
    delete_at_dest(conn, &mp, &rel)?;
    if dest_exists(conn, &mp, &rel)? {
        return Err(MountFileError::FileOp(FileOpError::new(
            format!("Path still present after delete: {rel}"),
            FileOpErrorCode::VerifyFailed,
        )));
    }
    Ok(serde_json::json!({
        "deleted": true,
        "mountPointId": mp.id,
        "path": rel,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_fs_absolute_pins_the_boundary_guard() {
        let base = Some("/mnt/vault");
        // In-bounds paths resolve.
        assert_eq!(
            resolve_fs_absolute(base, "m1", "notes/a.md").unwrap(),
            "/mnt/vault/notes/a.md"
        );
        // The base itself resolves.
        assert_eq!(resolve_fs_absolute(base, "m1", "").unwrap(), "/mnt/vault");
        // `..` that resolves within bounds is allowed (path.resolve collapses it).
        assert_eq!(
            resolve_fs_absolute(base, "m1", "notes/../a.md").unwrap(),
            "/mnt/vault/a.md"
        );
        // Escapes are refused with INVALID_PATH.
        for escape in ["../secret", "../../etc/passwd", "notes/../../escape"] {
            let e = resolve_fs_absolute(base, "m1", escape).unwrap_err();
            assert_eq!(e.code, FileOpErrorCode::InvalidPath, "escape {escape}");
        }
        // A sibling directory sharing a prefix does NOT count as in-bounds.
        let e = resolve_fs_absolute(Some("/mnt/vault"), "m1", "../vault-evil/x").unwrap_err();
        assert_eq!(e.code, FileOpErrorCode::InvalidPath);
        // No basePath configured.
        let e = resolve_fs_absolute(None, "m1", "a.md").unwrap_err();
        assert_eq!(e.code, FileOpErrorCode::InvalidPath);
        let e = resolve_fs_absolute(Some(""), "m1", "a.md").unwrap_err();
        assert_eq!(e.code, FileOpErrorCode::InvalidPath);
    }

    #[test]
    fn node_dirname_matches_posix() {
        assert_eq!(node_dirname("a/b/c.md"), "a/b");
        assert_eq!(node_dirname("c.md"), ".");
        assert_eq!(node_dirname("/x"), "/");
    }
}
