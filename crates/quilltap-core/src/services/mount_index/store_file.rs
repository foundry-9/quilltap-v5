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

use crate::inventory::lint_scope::TitleLintScope;
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
    detect_blob_file_type, detect_native_text, is_extractable_blob_path, mime_for_extension,
    normalise_relative_path, path_extname,
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
    /// Extract text from PDF/DOCX and native-text blobs and enqueue chunks
    /// (default true; v4 `40fab1c54` widened it from PDF/DOCX only).
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

/// v4 `extractBlobText`'s argument bag (`store-file.ts:158-164`).
pub struct ExtractBlobTextArgs<'a> {
    pub blob_id: &'a str,
    pub link_id: &'a str,
    pub relative_path: &'a str,
    pub data: &'a [u8],
    pub mount_point_id: &'a str,
}

/// What [`extract_blob_text`] does with a `pdf`/`docx` blob when the
/// [`DocumentTextExtractor`] cannot convert at all
/// (`is_available() == false`) — P4.D354 ruling R-1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefusedConverterPolicy {
    /// v4's bytes: the empty-text arm writes `failed` / `Converter produced no
    /// text`. The Scriptorium REST upload (`store_mount_file`) — where v5
    /// already wrote those bytes and `mount_write_equivalence` pins them.
    WriteFailed,
    /// Write NOTHING: the row stays `extractionStatus 'none'`, so the startup
    /// backfill's worklist revisits it once a real extractor lands. The upload
    /// bridges and the backfill — under v5's refusing seam v4's write would
    /// stamp every PDF/DOCX upload `failed` forever. The NAMED divergence
    /// `REFUSING_SEAM_LEAVES_PDF_UNEXTRACTED`.
    LeaveUnextracted,
}

/// v4 `extractBlobText` (`store-file.ts:148-239`, `40fab1c54`): derive a
/// blob's plain text and record it on its link (`extractedText`,
/// `extractionStatus`, `conversionStatus`). The one place blob text extraction
/// lives: `storeMountFile` calls it on ingest and the startup backfill
/// (`upload_blob_text_backfill`) on uploads that predate it. Does not chunk or
/// embed — callers run `reindex_single_file` after. Never throws in v4; a
/// failed conversion is recorded as `failed` and returns false.
///
/// Returns true when usable text was stored. Native text is the bytes as
/// UTF-8 (v4's `Buffer.toString('utf-8')` replaces each invalid sequence with
/// U+FFFD — `String::from_utf8_lossy`, byte-identical).
///
/// v4's `try` wraps the conversion AND the writes, so its catch arm is
/// reachable in v5 through a failed write: WARN `extractBlobText: text
/// extraction failed` `{mountPointId, relativePath, blobType, error}`
/// (renamed by `40fab1c54` from `storeMountFile: text extraction failed`; the
/// key `blobType` was `fileType`), then the `failed` / `skipped` writes with
/// the error's message, and — if THOSE fail — WARN `extractBlobText: could
/// not record the failure` `{mountPointId, relativePath, error}`. What stays
/// unreachable is a CONVERTER throw: the seam is `extract(&self, &[u8], &str)
/// -> String`, infallible by signature; the future extractor order widens it
/// to `Result` (`converters.rs`'s spec; P4.D354 H5).
pub fn extract_blob_text(
    mount: &Connection,
    args: &ExtractBlobTextArgs<'_>,
    extractor: &dyn DocumentTextExtractor,
    refused_converter: RefusedConverterPolicy,
) -> bool {
    if !is_extractable_blob_path(args.relative_path) {
        return false;
    }
    let blob_type = detect_blob_file_type(args.relative_path);
    let text = if blob_type == "pdf" || blob_type == "docx" {
        if refused_converter == RefusedConverterPolicy::LeaveUnextracted
            && !extractor.is_available()
        {
            // v5's own line (v4 has a working converter): R-1's skip.
            tracing::debug!(
                target: "quilltap::mount_index",
                mountPointId = %args.mount_point_id,
                relativePath = %args.relative_path,
                blobType = %blob_type,
                "extractBlobText: converter unavailable; leaving the blob unextracted"
            );
            return false;
        }
        extractor.extract(args.data, blob_type)
    } else {
        String::from_utf8_lossy(args.data).into_owned()
    };
    match record_extracted_text(mount, args, blob_type, &text) {
        Ok(stored) => stored,
        Err(e) => {
            let msg = e.to_string();
            tracing::warn!(
                target: "quilltap::mount_index",
                mountPointId = %args.mount_point_id,
                relativePath = %args.relative_path,
                blobType = %blob_type,
                error = %msg,
                "extractBlobText: text extraction failed"
            );
            if let Err(record_err) = record_extraction_failure(mount, args, &msg) {
                tracing::warn!(
                    target: "quilltap::mount_index",
                    mountPointId = %args.mount_point_id,
                    relativePath = %args.relative_path,
                    error = %record_err,
                    "extractBlobText: could not record the failure"
                );
            }
            false
        }
    }
}

/// The body of v4's `try` past the conversion: the `converted` writes + the
/// DEBUG on non-blank text, else the empty arm (no log line).
fn record_extracted_text(
    mount: &Connection,
    args: &ExtractBlobTextArgs<'_>,
    blob_type: &str,
    text: &str,
) -> Result<bool, crate::db::DbError> {
    if crate::jsstr::js_trim(text).is_empty() {
        record_extraction_failure(mount, args, "Converter produced no text")?;
        return Ok(false);
    }
    let sha = sha256_of_string(text);
    DocMountBlobsRepository::new(mount).update_extracted_text(
        args.blob_id,
        Some(text),
        Some(&sha),
        "converted",
        None,
        args.link_id,
    )?;
    let text_length = crate::jsstr::utf16_len(text);
    DocMountFileLinksRepository::new(mount).update(
        args.link_id,
        &LinkUpdate {
            conversion_status: Some("converted".to_string()),
            conversion_error: Some(None),
            plain_text_length: Some(Some(text_length as f64)),
            chunk_count: Some(0.0),
            updated_at: crate::clock::now_iso(),
            ..Default::default()
        },
    )?;
    tracing::debug!(
        target: "quilltap::mount_index",
        mountPointId = %args.mount_point_id,
        relativePath = %args.relative_path,
        blobType = %blob_type,
        textLength = text_length,
        "extractBlobText: stored extracted text"
    );
    Ok(true)
}

/// v4's `failed` / `skipped` pair with `error` as both messages (the empty arm
/// passes `Converter produced no text`; the catch arm the thrown message).
fn record_extraction_failure(
    mount: &Connection,
    args: &ExtractBlobTextArgs<'_>,
    error: &str,
) -> Result<(), crate::db::DbError> {
    DocMountBlobsRepository::new(mount).update_extracted_text(
        args.blob_id,
        None,
        None,
        "failed",
        Some(error),
        args.link_id,
    )?;
    DocMountFileLinksRepository::new(mount).update(
        args.link_id,
        &LinkUpdate {
            conversion_status: Some("skipped".to_string()),
            conversion_error: Some(Some(error.to_string())),
            plain_text_length: Some(None),
            chunk_count: Some(0.0),
            updated_at: crate::clock::now_iso(),
            ..Default::default()
        },
    )?;
    Ok(())
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
    // P4.104: the blob write below normalizes through the same encoder the
    // pre-transcode uses — v4 runs BOTH (`store-file.ts:250` transcodes when
    // `transcodeImages`, then `linkBlobContent` normalizes whatever arrives).
    // After a successful pre-transcode the bytes are lossy WebP, which the
    // normalization declines; with `transcode_images: false` it is the
    // normalization alone that moves a PNG.
    let links = DocMountFileLinksRepository::with_blob_codec(mount, webp);

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
        // v4 `store-file.ts:181`: the fallback path read — `null` shapes the
        // result from the extension (P4.142 G2).
        let link = links.find_by_mount_point_and_path_or_none(&mp.id, &rel);
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
        let mtime = match crate::db::database_store::write_database_document(
            mount,
            &mp.id,
            &rel,
            &text,
            TitleLintScope::Unlinted { main },
        ) {
            Ok(m) => m,
            Err(crate::db::database_store::StoreError::Store(se)) => {
                return Err(MountFileError::Store(se))
            }
            Err(crate::db::database_store::StoreError::Db(de)) => {
                return Err(MountFileError::Db(de))
            }
            // Neither a FileOpError nor a DatabaseStoreError in v4: the
            // route's catch-all 500 `Failed to write file` (P4.D340 R-8).
            Err(crate::db::database_store::StoreError::TitleLint(r)) => {
                return Err(MountFileError::Other(r.message))
            }
            // P4.D377: v4's store throws the shared FileOpError (INVALID_PATH
            // → 400 through `fileOpStatus`).
            Err(crate::db::database_store::StoreError::FileOp(fe)) => {
                return Err(MountFileError::FileOp(fe))
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

        // v4 `store-file.ts:230`: the fallback path read — `null` → the sha from
        // the text (P4.142 G2).
        let link = links.find_by_mount_point_and_path_or_none(&mp.id, &rel);
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
            transcoded: false,
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
    // v4 `store-file.ts:276-279`: the fallback path read (`null` → no chunk
    // delete → the upsert) — P4.142 G2.
    if let Some(existing) = links.find_by_mount_point_and_path_or_none(&mp.id, &final_path) {
        links.delete_chunks_by_link_id(&existing.id)?;
    }

    let result = links.link_blob_content(&LinkBlobInput {
        mount_point_id: mp.id.clone(),
        relative_path: final_path.clone(),
        file_name: node_basename(&final_path).to_string(),
        file_type: Some(mirror_file_type.to_string()),
        original_file_name: Some(
            input
                .original_file_name
                .clone()
                .unwrap_or_else(|| node_basename(&final_path).to_string()),
        ),
        original_mime_type: Some(original_mime_type.clone()),
        stored_mime_type: transcoded.stored_mime_type.clone(),
        sha256: transcoded.sha256.clone(),
        data: transcoded.data.clone(),
        // P4.D209: v4 `normalizeImages` (`186eb09cb`) — the write-side
        // chokepoint, default true.
        normalize_images: true,
        description: Some(input.description.clone().unwrap_or_default()),
        conversion_status: None,
        extracted_text: None,
        extracted_text_sha256: None,
        extraction_status: None,
        last_modified: None,
        created_at: None,
    })?;

    // Text the doc tools can read, grep and search (v4 `40fab1c54`, bug 260):
    // PDF/DOCX are converted from the ORIGINAL bytes (transcode only touches
    // bitmaps, so for these types transcoded bytes == input bytes); native text
    // is the bytes themselves. The REST path keeps v4's `failed` bytes when the
    // converter answers nothing (R-1: `WriteFailed`).
    let has_extracted_text = input.extract_text
        && extract_blob_text(
            mount,
            &ExtractBlobTextArgs {
                blob_id: &result.blob_id,
                link_id: &result.link_id,
                relative_path: &final_path,
                data: &input.data,
                mount_point_id: &mp.id,
            },
            extractor,
            RefusedConverterPolicy::WriteFailed,
        );

    // v4 `store-file.ts:430` emitDocumentWritten — its publish half is the
    // store's hint (P4.D381); the watcher half is the standing deferral.
    super::realtime::emit_document_written(&mp.id, &final_path);

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
