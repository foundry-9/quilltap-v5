//! Port of v4 `lib/mount-index/store-file.ts` — the canonical mount-point
//! write/ingest pipeline. `store_mount_file` is the single chokepoint every
//! *fresh* file write funnels through (the per-file PUT, the blob upload, and
//! the file-storage bridges): native-text vs binary routing, image transcode
//! to WebP (seam), pdf/docx text extraction (seam) + chunk/embedding enqueue,
//! content-addressed dedup + folder-row ensure, mtime-based optimistic
//! concurrency, and the store event (the watcher deferral).
//!
//! Deliberately distinct from `file_ops::copy_file`/`move_file`, which are
//! *byte-preserving* (they must not transcode, so their sha verification
//! holds). Fresh writes ingest; copy/move relocate.
//!
//! v4 guards against running in a background-job child (raw mount-index writes
//! would bypass the child's buffered repo proxy). In v5 that invariant is
//! STRUCTURAL: only the writer task holds the RW connection, and this function
//! takes `&Connection`s that exist only inside a writer closure — a child
//! context cannot arise. (The why is kept because the single-writer rule is
//! the correctness content of v4's guard.)

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::{
    sha256_of_string, DocMountFileLinksRepository, LinkBlobInput, LinkUpdate,
};
use crate::db::doc_mount_points::MountServiceInfo;

use super::blob_transcode::{normalise_blob_relative_path, transcode_to_webp, WebpTranscoder};
use super::converters::DocumentTextExtractor;
use super::file_op_error::{FileOpError, FileOpErrorCode};
use super::file_ops::{delete_at_dest, dest_exists, resolve_fs_absolute, write_fs_file_bytes};
use super::path_utils::{
    detect_native_text, mime_for_extension, normalise_relative_path, path_extname,
};
use super::read_file::MountFileError;
use super::refresh::refresh_now;
use super::reindex_file::{node_basename, reindex_single_file};

/// v4 `CollisionStrategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionStrategy {
    /// Throw `DEST_EXISTS` unless `force` (then overwrite).
    ErrorIfExists,
    /// Upsert in place; rely on `expectedMtime` for conflict detection.
    Overwrite,
    /// Pick a free ` (2)`, ` (3)`… path (bridges/attachments).
    UniqueSuffix,
}

/// v4 `AssetStorageMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetStorageMode {
    /// Mount-type aware (fs mounts → disk; database mounts → documents/blobs).
    Auto,
    /// Always store in the mount-index DB (the legacy `/blobs` behaviour).
    Database,
}

/// v4 `StoreFileInput` (defaults per the v4 field docs).
pub struct StoreFileInput {
    pub mount_point_id: String,
    pub relative_path: String,
    pub data: Vec<u8>,
    pub original_mime_type: Option<String>,
    pub original_file_name: Option<String>,
    pub description: Option<String>,
    pub force: bool,
    pub expected_mtime: Option<i64>,
    /// Default true.
    pub transcode_images: bool,
    /// Default true.
    pub extract_text: bool,
    /// Default true.
    pub enqueue_embedding: bool,
    /// Default true (bridges set false so attachments stay binary blobs).
    pub treat_native_text_as_document: bool,
    pub collision_strategy: CollisionStrategy,
    pub asset_storage: AssetStorageMode,
}

impl StoreFileInput {
    /// The v4 defaults for everything past the identity fields.
    pub fn new(mount_point_id: &str, relative_path: &str, data: Vec<u8>) -> Self {
        StoreFileInput {
            mount_point_id: mount_point_id.to_string(),
            relative_path: relative_path.to_string(),
            data,
            original_mime_type: None,
            original_file_name: None,
            description: None,
            force: false,
            expected_mtime: None,
            transcode_images: true,
            extract_text: true,
            enqueue_embedding: true,
            treat_native_text_as_document: true,
            collision_strategy: CollisionStrategy::ErrorIfExists,
            asset_storage: AssetStorageMode::Auto,
        }
    }
}

/// v4 `StoreFileResult` (serialized by the write routes).
#[derive(Debug, Clone)]
pub struct StoreFileResult {
    pub mount_point_id: String,
    pub relative_path: String,
    /// `'document' | 'blob' | 'filesystem'`.
    pub kind: &'static str,
    pub file_type: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub stored_mime_type: String,
    pub mtime: i64,
    pub file_id: Option<String>,
    pub link_id: Option<String>,
    pub blob_id: Option<String>,
}

fn detect_blob_file_type(relative_path: &str) -> &'static str {
    match path_extname(relative_path).to_lowercase().as_str() {
        ".pdf" => "pdf",
        ".docx" => "docx",
        _ => "blob",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// v4 `resolveUniqueRelativePath` (`bridge-path-helpers.ts:46`) — bump ` (2)`,
/// ` (3)`… against the blob namespace until free (the 999-attempt hash
/// fallback rides `now_unix_ms`, unreachable in practice).
fn resolve_unique_relative_path(
    conn: &Connection,
    mount_point_id: &str,
    desired: &str,
) -> Result<String, MountFileError> {
    let blobs = DocMountBlobsRepository::new(conn);
    if blobs
        .find_by_mount_point_and_path(mount_point_id, desired)?
        .is_none()
    {
        return Ok(desired.to_string());
    }
    let dir = super::file_ops::node_dirname(desired);
    let ext = path_extname(desired);
    let base = node_basename(desired);
    let stem = base.strip_suffix(&ext).unwrap_or(base);
    let prefix = if dir == "." || dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    for attempt in 2..=999 {
        let candidate = format!("{prefix}{stem} ({attempt}){ext}");
        if blobs
            .find_by_mount_point_and_path(mount_point_id, &candidate)?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(format!("{desired}:{}", crate::clock::now_unix_ms()));
    let hash = hex::encode(hasher.finalize());
    Ok(format!("{prefix}{stem}-{}{ext}", &hash[..8]))
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

/// v4 `storeMountFile` — the single ingest pipeline. The post-write refresh
/// chain (v4's fire-and-forget) executes synchronously here: under the
/// single-writer model the work serializes on the writer thread regardless
/// (see `refresh.rs`).
pub fn store_mount_file(
    main: &Connection,
    mount: &Connection,
    input: &StoreFileInput,
    extractor: &dyn DocumentTextExtractor,
    webp: &dyn WebpTranscoder,
) -> Result<StoreFileResult, MountFileError> {
    let mp = load_mount(mount, &input.mount_point_id)?;
    let rel = normalise_relative_path(&input.relative_path)?;
    let links = DocMountFileLinksRepository::new(mount);

    // ------------------------------------------------------------------
    // Filesystem mounts (auto storage): bytes to disk, indexed by the scanner.
    // ------------------------------------------------------------------
    if input.asset_storage == AssetStorageMode::Auto && mp.is_filesystem_mount() {
        // Optimistic concurrency: reject when the on-disk file changed under us
        // (ENOENT = no conflict — the file doesn't exist yet).
        if let Some(expected) = input.expected_mtime {
            let abs = resolve_fs_absolute(mp.base_path.as_deref(), &mp.id, &rel)?;
            match std::fs::metadata(&abs) {
                Ok(meta) => {
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    if mtime != expected {
                        return Err(MountFileError::FileOp(FileOpError::new(
                            "File was modified by another process (mtime mismatch). Reload and try again.".to_string(),
                            FileOpErrorCode::Conflict,
                        )));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(MountFileError::Other(e.to_string())),
            }
        }
        if input.collision_strategy == CollisionStrategy::ErrorIfExists
            && dest_exists(mount, &mp, &rel)?
        {
            if !input.force {
                return Err(MountFileError::FileOp(FileOpError::new(
                    format!("Destination already exists: {rel}. Use force to overwrite."),
                    FileOpErrorCode::DestExists,
                )));
            }
            delete_at_dest(mount, &mp, &rel)?;
        }
        let (sha, size, mtime) = write_fs_file_bytes(mount, &mp, &rel, &input.data, extractor)?;
        let link = links.find_by_mount_point_and_path(&mp.id, &rel)?;
        // If processMountFile failed to index (no link row), still report a
        // sensible type: native text by extension before the binary fallback.
        let file_type = link
            .as_ref()
            .map(|l| l.file_type.clone())
            .or_else(|| detect_native_text(&rel).map(|t| t.as_str().to_string()))
            .unwrap_or_else(|| detect_blob_file_type(&rel).to_string());
        return Ok(StoreFileResult {
            mount_point_id: mp.id.clone(),
            relative_path: rel.clone(),
            kind: "filesystem",
            file_type,
            sha256: sha,
            size_bytes: size,
            stored_mime_type: input
                .original_mime_type
                .clone()
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| mime_for_extension(&rel).to_string()),
            mtime,
            file_id: link.as_ref().map(|l| l.file_id.clone()),
            link_id: link.map(|l| l.id),
            blob_id: None,
        });
    }

    // ------------------------------------------------------------------
    // Database storage — native-text documents.
    // ------------------------------------------------------------------
    let native_text = detect_native_text(&rel);
    if let (true, Some(native_text), "database") = (
        input.treat_native_text_as_document,
        native_text,
        mp.mount_type.as_str(),
    ) {
        if input.collision_strategy == CollisionStrategy::ErrorIfExists
            && crate::db::database_store::database_document_exists(mount, &mp.id, &rel)?
            && !input.force
        {
            return Err(MountFileError::FileOp(FileOpError::new(
                format!("Destination already exists: {rel}. Use force to overwrite."),
                FileOpErrorCode::DestExists,
            )));
        }
        let text = String::from_utf8_lossy(&input.data).into_owned();

        // v4 `writeDatabaseDocument`: the expectedMtime guard (a
        // DatabaseStoreError CONFLICT → 409 via fileOpStatus)...
        if let Some(expected) = input.expected_mtime {
            let docs = DocMountDocumentsRepository::new(mount);
            if let Some((_, last_modified)) =
                docs.find_content_and_mtime_by_mount_point_and_path(&mp.id, &rel)?
            {
                let current = crate::clock::iso_to_ms(&last_modified).unwrap_or(0);
                if current != expected {
                    return Err(MountFileError::Store(
                        crate::db::database_store::DatabaseStoreError {
                            message: "Document was modified by another process (mtime mismatch). Please reload and try again.".to_string(),
                            code: crate::db::database_store::DbStoreErrorCode::Conflict,
                        },
                    ));
                }
            }
        }
        // ...the linkDocumentContent write...
        let mtime =
            match crate::db::database_store::write_database_document(mount, &mp.id, &rel, &text) {
                Ok(m) => m,
                Err(crate::db::database_store::StoreError::Store(se)) => {
                    return Err(MountFileError::Store(se))
                }
                Err(crate::db::database_store::StoreError::Db(de)) => {
                    return Err(MountFileError::Db(de))
                }
            };
        // ...and the parent-side inline chunk pass (v4's why-comment: without
        // it the embedding scheduler finds no chunks and the document stays
        // unsearchable until a manual rescan).
        reindex_single_file(mount, &mp.id, &rel, "", extractor);

        if input.enqueue_embedding {
            // v4's fire-and-forget reindex + embed + stats — synchronous here
            // (see the module header / refresh.rs).
            refresh_now(main, mount, &mp.id, &rel, "", extractor);
        } else if let Err(e) =
            crate::db::doc_mount_points::DocMountPointsRepository::new(mount).refresh_stats(&mp.id)
        {
            eprintln!("storeMountFile: best-effort stats refresh failed: {e}");
        }

        let link = links.find_by_mount_point_and_path(&mp.id, &rel)?;
        let size_bytes = text.len() as i64;
        return Ok(StoreFileResult {
            mount_point_id: mp.id.clone(),
            relative_path: rel.clone(),
            kind: "document",
            file_type: native_text.as_str().to_string(),
            sha256: link
                .as_ref()
                .map(|l| l.sha256.clone())
                .unwrap_or_else(|| sha256_of_string(&text)),
            size_bytes,
            stored_mime_type: mime_for_extension(&rel).to_string(),
            mtime,
            file_id: link.as_ref().map(|l| l.file_id.clone()),
            link_id: link.map(|l| l.id),
            blob_id: None,
        });
    }

    // ------------------------------------------------------------------
    // Database storage — binary/blob mirror (transcode + extract seams).
    // ------------------------------------------------------------------
    let original_mime_type = input
        .original_mime_type
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| mime_for_extension(&rel).to_string());
    let transcoded = if input.transcode_images {
        transcode_to_webp(&input.data, &original_mime_type, webp)
    } else {
        super::blob_transcode::TranscodeResult {
            data: input.data.clone(),
            stored_mime_type: original_mime_type.clone(),
            size_bytes: input.data.len() as i64,
            sha256: sha256_hex(&input.data),
        }
    };

    let mut final_path = normalise_blob_relative_path(&rel, &transcoded.stored_mime_type);

    match input.collision_strategy {
        CollisionStrategy::UniqueSuffix => {
            final_path = resolve_unique_relative_path(mount, &mp.id, &final_path)?;
        }
        CollisionStrategy::ErrorIfExists => {
            if dest_exists(mount, &mp, &final_path)? {
                if !input.force {
                    return Err(MountFileError::FileOp(FileOpError::new(
                        format!(
                            "Destination already exists: {final_path}. Use force to overwrite."
                        ),
                        FileOpErrorCode::DestExists,
                    )));
                }
                delete_at_dest(mount, &mp, &final_path)?;
            }
        }
        CollisionStrategy::Overwrite => {}
    }

    let mirror_file_type = detect_blob_file_type(&final_path);

    // 'overwrite' (and force) re-point an existing link at new content — drop
    // its stale chunks first so a re-extraction starts clean.
    if let Some(existing) = links.find_by_mount_point_and_path(&mp.id, &final_path)? {
        links.delete_chunks_by_link_id(&existing.id)?;
    }

    let result = links.link_blob_content(&LinkBlobInput {
        mount_point_id: mp.id.clone(),
        relative_path: final_path.clone(),
        file_name: node_basename(&final_path).to_string(),
        file_type: Some(mirror_file_type.to_string()),
        original_file_name: input
            .original_file_name
            .clone()
            .unwrap_or_else(|| node_basename(&final_path).to_string()),
        original_mime_type: original_mime_type.clone(),
        stored_mime_type: transcoded.stored_mime_type.clone(),
        sha256: transcoded.sha256.clone(),
        data: transcoded.data.clone(),
        description: Some(input.description.clone().unwrap_or_default()),
        conversion_status: None,
        extracted_text: None,
        extracted_text_sha256: None,
        extraction_status: None,
    })?;

    // PDF/DOCX: extract plain text from the ORIGINAL bytes (transcode only
    // touches bitmaps, so for these types transcoded bytes == input bytes).
    let mut has_extracted_text = false;
    if input.extract_text && (mirror_file_type == "pdf" || mirror_file_type == "docx") {
        let blobs = DocMountBlobsRepository::new(mount);
        let text = extractor.extract(&input.data, mirror_file_type);
        if !crate::jsstr::js_trim(&text).is_empty() {
            let sha = sha256_of_string(&text);
            blobs.update_extracted_text(
                &result.blob_id,
                Some(&text),
                Some(&sha),
                "converted",
                None,
                Some(&result.link_id),
            )?;
            has_extracted_text = true;
            links.update(
                &result.link_id,
                &LinkUpdate {
                    conversion_status: Some("converted".to_string()),
                    conversion_error: Some(None),
                    plain_text_length: Some(Some(crate::jsstr::utf16_len(&text) as f64)),
                    chunk_count: Some(0.0),
                    updated_at: crate::clock::now_iso(),
                    ..Default::default()
                },
            )?;
        } else {
            // The extractor produced no text (v4's empty arm; the refusing
            // seam routes here too, loudly).
            blobs.update_extracted_text(
                &result.blob_id,
                None,
                None,
                "failed",
                Some("Converter produced no text"),
                Some(&result.link_id),
            )?;
            links.update(
                &result.link_id,
                &LinkUpdate {
                    conversion_status: Some("skipped".to_string()),
                    conversion_error: Some(Some("Converter produced no text".to_string())),
                    plain_text_length: Some(None),
                    chunk_count: Some(0.0),
                    updated_at: crate::clock::now_iso(),
                    ..Default::default()
                },
            )?;
        }
    }

    // v4 emitDocumentWritten — the watcher deferral (see file_ops header).

    if input.enqueue_embedding && has_extracted_text {
        refresh_now(main, mount, &mp.id, &final_path, "", extractor);
    } else if let Err(e) =
        crate::db::doc_mount_points::DocMountPointsRepository::new(mount).refresh_stats(&mp.id)
    {
        eprintln!("storeMountFile: best-effort stats refresh failed: {e}");
    }

    Ok(StoreFileResult {
        mount_point_id: mp.id.clone(),
        relative_path: final_path,
        kind: "blob",
        file_type: mirror_file_type.to_string(),
        sha256: transcoded.sha256,
        size_bytes: transcoded.size_bytes,
        stored_mime_type: transcoded.stored_mime_type,
        mtime: crate::clock::now_unix_ms(),
        file_id: Some(result.file_id),
        link_id: Some(result.link_id),
        blob_id: Some(result.blob_id),
    })
}
