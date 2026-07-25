//! The shared "save an image into a photo album" service — port of v4
//! `lib/photos/save-image-to-album.ts`.
//!
//! One service backs both the `keep_image` LLM tool and the Salon's Save-Image
//! button. Given any `doc_mount_point` id, an image-v2 file id, and an attribution
//! (who is saving it), it:
//!   - resolves the `FileEntry` (with the mount-blob fallback for a link-id input),
//!   - reads the image bytes via the injected [`FileBytesStore`] seam,
//!   - dedupes by sha256 within the target mount's `photos/` folder,
//!   - builds the kept-image Markdown sidecar,
//!   - hard-links the binary into `<mount>/photos/<ts>-<slug>.<ext>` via
//!     `link_blob_content`,
//!   - runs the chunk pass (v4 `chunkAndInsertExtractedText` — chunks the
//!     Markdown sidecar into `doc_mount_chunks` and rolls up the link's
//!     `chunkCount`; live since P4.6BK) and the (recorded-seam) mount
//!     invalidation + embedding enqueue.
//!
//! Character-vault saves (`keep_image`) and ad-hoc UI saves all flow through this
//! one codepath so the on-disk artifact is identical regardless of origin.
//!
//! ## The bytes seam
//!
//! v4 reads the image bytes from `fileStorageManager.downloadFile(entry)` (host
//! filesystem / S3). The Rust core keeps that host I/O behind the [`FileBytesStore`]
//! trait, keyed by the [`FileEntry`] the metadata read resolved. The differential
//! injects a canned in-memory store keyed by fileId, and `ingest_image_buffer`'s
//! mount-blob fallback is likewise routed through the seam.

use rusqlite::Connection;

use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_file_links::{DocMountFileLinksRepository, LinkBlobInput};
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::files::{FileEntry, FilesRepository};
use crate::db::DbError;
use crate::photos::keep_image_markdown::{
    basename_of_relative_path, build_kept_image_markdown, build_slug_and_filename,
    sha256_of_string, BuildKeptImageMarkdownInput, BuildSlugAndFilenameInput,
    KeptImageAttributionRole, SceneState,
};
use crate::photos::photos_paths::{
    build_photos_relative_path, is_photos_relative_path, PHOTOS_FOLDER,
};

/// v4's `SINGLE_USER_ID` — the constant user the mount-blob-fallback ingest
/// attributes to (`lib/auth/single-user.ts`).
pub const SINGLE_USER_ID: &str = "00000000-0000-0000-0000-000000000000";

/// Who is saving the image (v4 `SaveImageAttribution`).
#[derive(Debug, Clone)]
pub struct SaveImageAttribution {
    /// Display name (character name, user persona name, or "Quilltap").
    pub name: String,
    /// Optional id matching `name` — characterId / participantId / userId / None.
    pub id: Option<String>,
    pub role: KeptImageAttributionRole,
}

/// Input to [`save_image_to_album`] (v4 `SaveImageToAlbumInput`).
pub struct SaveImageToAlbumInput<'a> {
    /// Any `doc_mount_point` id — character vault, project store, General, etc.
    pub mount_point_id: &'a str,
    /// The image-v2 `FileEntry` id (or a `doc_mount_file_links.id` — the fallback).
    pub file_id: &'a str,
    pub caption: Option<&'a str>,
    pub tags: &'a [String],
    /// When set, the chat's `sceneState` is snapshotted into the markdown sidecar.
    pub chat_id: Option<&'a str>,
    pub attribution: SaveImageAttribution,
}

/// Output of [`save_image_to_album`] (v4 `SaveImageToAlbumOutput`).
#[derive(Debug, Clone)]
pub struct SaveImageToAlbumOutput {
    pub mount_point_name: String,
    pub relative_path: String,
    pub link_id: String,
    pub kept_at: String,
    pub file_id: String,
    pub sha256: String,
}

/// v4 `SaveImageErrorCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveImageErrorCode {
    ImageNotFound,
    NotAnImage,
    EmptyBytes,
    MountNotFound,
    AlreadySaved,
}

/// v4 `SaveImageToAlbumError` — the expected failure modes. The caller decides
/// whether to surface these as user-facing errors.
#[derive(Debug, Clone)]
pub struct SaveImageToAlbumError {
    pub code: SaveImageErrorCode,
    pub message: String,
    /// Existing relativePath when `code == AlreadySaved`.
    pub existing_relative_path: Option<String>,
    /// Existing createdAt when `code == AlreadySaved`.
    pub existing_created_at: Option<String>,
}

impl SaveImageToAlbumError {
    fn new(code: SaveImageErrorCode, message: impl Into<String>) -> Self {
        SaveImageToAlbumError {
            code,
            message: message.into(),
            existing_relative_path: None,
            existing_created_at: None,
        }
    }
}

/// The image-bytes boundary (v4's `fileStorageManager.downloadFile` +
/// `ingestImageBuffer`). Host-side I/O in production; the differential injects a
/// canned in-memory store.
pub trait FileBytesStore: Send + Sync {
    /// Read the stored bytes for a resolved [`FileEntry`]. `Ok(None)` when the
    /// bytes are missing (v4's `readImageBuffer` throw is folded here into a
    /// `None`/`Err` → `EMPTY_BYTES`); `Err` for a genuine read failure.
    fn read_image_buffer(&self, entry: &FileEntry) -> Result<Option<Vec<u8>>, String>;

    /// v4 `ingestImageBuffer` — synthesize + persist a NEW `FileEntry` for a set of
    /// raw bytes (the mount-blob fallback: the sister `FileEntry` was reaped, so we
    /// re-ingest the mount-blob bytes, losing generation metadata). Returns the new
    /// entry (its `id`/`sha256`/`mimeType`/`originalFilename` at least). Host-side
    /// in production.
    fn ingest_image_buffer(&self, req: &IngestImageRequest) -> Result<FileEntry, String>;
}

/// The `ingestImageBuffer` request (v4's params subset the fallback uses).
#[derive(Debug, Clone)]
pub struct IngestImageRequest {
    pub buffer: Vec<u8>,
    pub original_filename: String,
    pub mime_type: String,
    pub user_id: String,
}

/// v4 `inferImageMimeFromFilename` — the mount-blob fallback's extension → MIME map.
fn infer_image_mime_from_filename(filename: &str) -> Option<&'static str> {
    let ext = filename
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("webp") => Some("image/webp"),
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("avif") => Some("image/avif"),
        Some("svg") => Some("image/svg+xml"),
        _ => None,
    }
}

/// v4 `findExistingPhotosLinkBySha` — the per-mount, `photos/`-only dedup scan.
/// Returns `(link_id, relative_path, created_at)` of the first `photos/` link whose
/// file sha matches. A sha existing ELSEWHERE in the vault does NOT collide.
fn find_existing_photos_link_by_sha(
    mount: &Connection,
    mount_point_id: &str,
    sha256: &str,
) -> Result<Option<(String, String, String)>, DbError> {
    let links = DocMountFileLinksRepository::new(mount).find_by_mount_point_id(mount_point_id)?;
    for l in links {
        if l.sha256 == sha256 && is_photos_relative_path(Some(&l.relative_path)) {
            return Ok(Some((l.id, l.relative_path, l.created_at)));
        }
    }
    Ok(None)
}

/// The default production [`FileBytesStore`] for an instance with no host byte
/// source wired (faithful to "no images-v2 storage backend available"): every read
/// reports missing bytes and the mount-blob-fallback ingest is unavailable. A
/// `keep_image` on such an instance surfaces the `EMPTY_BYTES` failure, never a
/// silent wrong answer. Production (the Tauri/CLI host) injects a real store over
/// `fileStorageManager`; the differential injects a canned in-memory store.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotConfiguredBytes;

impl FileBytesStore for NotConfiguredBytes {
    fn read_image_buffer(&self, _entry: &FileEntry) -> Result<Option<Vec<u8>>, String> {
        // No host byte store → the bytes are unavailable (→ EMPTY_BYTES).
        Ok(None)
    }
    fn ingest_image_buffer(&self, _req: &IngestImageRequest) -> Result<FileEntry, String> {
        Err("image ingest is not available without a host file store".to_string())
    }
}

/// The recorded side effects (mount invalidation + embedding enqueue) — the
/// differential mirrors v4's jest-mocked no-ops. In production these are wired to
/// the mount-chunk cache + the embedding scheduler (host-side). Default: no-op.
pub trait SaveImageSideEffects {
    /// v4 `invalidateMountPoint(mountPointId)`.
    fn invalidate_mount_point(&self, _mount_point_id: &str) {}
    /// v4 `enqueueEmbeddingJobsForMountPoint(mountPointId)` (a recorded seam this
    /// round — the EMBEDDING_GENERATE enqueue is owned by another unit).
    fn enqueue_embedding_jobs(&self, _mount_point_id: &str) {}
}

/// The default no-op side effects (faithful to the differential's mocks).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSideEffects;
impl SaveImageSideEffects for NoSideEffects {}

/// Save an image into the `photos/` folder of any mount point (v4
/// `saveImageToAlbum`). `kept_at` is the injected ISO clock (v4 mints
/// `new Date().toISOString()`); `bytes` is the injected image-bytes seam.
///
/// `main` is a MAIN-db connection (for the `files` + `chats` reads); `mount` is a
/// mount-index connection (the store lives there). Both are held on the writer
/// thread by the caller.
pub fn save_image_to_album(
    main: &Connection,
    mount: &Connection,
    input: &SaveImageToAlbumInput<'_>,
    bytes: &dyn FileBytesStore,
    side_effects: &dyn SaveImageSideEffects,
    kept_at: &str,
) -> Result<SaveImageToAlbumOutput, SaveImageToAlbumError> {
    let files = FilesRepository::new(main);
    let links = DocMountFileLinksRepository::new(mount);
    let blobs = DocMountBlobsRepository::new(mount);
    let points = DocMountPointsRepository::new(mount);

    // Resolve the FileEntry (with the mount-blob fallback). `fileId` may be either
    // an images-v2 FileEntry id OR a `doc_mount_file_links.id` (the Librarian attach
    // announcement surfaces the link id). When given a link id we resolve to the
    // underlying FileEntry by sha256; if the sister FileEntry was reaped we fall
    // back to ingesting the mount-blob bytes — lossy (loses generation metadata),
    // but the photo survives.
    let mut file_entry: Option<FileEntry> = files.find_by_id(input.file_id).map_err(db_err)?;
    if file_entry.is_none() {
        if let Some(source_link) = links
            .find_by_id_with_content(input.file_id)
            .map_err(db_err)?
        {
            let sisters = files.find_by_sha256(&source_link.sha256).map_err(db_err)?;
            let sister = sisters
                .iter()
                .find(|f| f.category == "IMAGE")
                .or_else(|| sisters.first())
                .cloned();
            if let Some(sister) = sister {
                file_entry = Some(sister);
            } else {
                // Ingest the mount-blob bytes into a fresh FileEntry (host-side).
                let bytes_opt = blobs
                    .read_data_by_file_id(&source_link.file_id)
                    .map_err(db_err)?;
                if let Some(data) = bytes_opt {
                    if !data.is_empty() {
                        let mime_type = source_link
                            .original_mime_type
                            .clone()
                            .or_else(|| {
                                infer_image_mime_from_filename(&source_link.file_name)
                                    .map(str::to_string)
                            })
                            .unwrap_or_else(|| "application/octet-stream".to_string());
                        let original_filename = source_link
                            .original_file_name
                            .clone()
                            .unwrap_or_else(|| source_link.file_name.clone());
                        let ingested = bytes
                            .ingest_image_buffer(&IngestImageRequest {
                                buffer: data,
                                original_filename,
                                mime_type,
                                user_id: SINGLE_USER_ID.to_string(),
                            })
                            .map_err(|e| {
                                SaveImageToAlbumError::new(SaveImageErrorCode::EmptyBytes, e)
                            })?;
                        file_entry = Some(ingested);
                    }
                }
            }
        }
    }
    let Some(file_entry) = file_entry else {
        return Err(SaveImageToAlbumError::new(
            SaveImageErrorCode::ImageNotFound,
            format!("Image not found: {}", input.file_id),
        ));
    };
    if file_entry.category != "IMAGE" {
        return Err(SaveImageToAlbumError::new(
            SaveImageErrorCode::NotAnImage,
            format!(
                "File {} is not an image (category={})",
                input.file_id, file_entry.category
            ),
        ));
    }

    let mount_point = points
        .find_store_naming_by_id(input.mount_point_id)
        .map_err(db_err)?;
    let Some(mount_point) = mount_point else {
        return Err(SaveImageToAlbumError::new(
            SaveImageErrorCode::MountNotFound,
            format!("Mount point not found: {}", input.mount_point_id),
        ));
    };

    // Read the bytes first, then dedup on the hash of those ACTUAL bytes (the
    // FileEntry's sha256 is an upload-time input hash that can diverge from the
    // stored/transcoded bytes).
    let buffer = match bytes.read_image_buffer(&file_entry) {
        Ok(Some(b)) => b,
        Ok(None) => {
            return Err(SaveImageToAlbumError::new(
                SaveImageErrorCode::EmptyBytes,
                format!("Image {} has empty bytes", input.file_id),
            ));
        }
        Err(msg) => {
            return Err(SaveImageToAlbumError::new(
                SaveImageErrorCode::EmptyBytes,
                format!("Failed to read image bytes: {msg}"),
            ));
        }
    };
    if buffer.is_empty() {
        return Err(SaveImageToAlbumError::new(
            SaveImageErrorCode::EmptyBytes,
            format!("Image {} has empty bytes", input.file_id),
        ));
    }
    let sha256 = crate::photos::keep_image_markdown::sha256_of_buffer(&buffer);

    if let Some((_, existing_relative_path, existing_created_at)) =
        find_existing_photos_link_by_sha(mount, input.mount_point_id, &sha256).map_err(db_err)?
    {
        return Err(SaveImageToAlbumError {
            code: SaveImageErrorCode::AlreadySaved,
            message: format!(
                "Image already saved to {} on {} as {}",
                mount_point.name, existing_created_at, existing_relative_path
            ),
            existing_relative_path: Some(existing_relative_path),
            existing_created_at: Some(existing_created_at),
        });
    }

    // Scene-state snapshot (best-effort parse; a malformed non-null column → the
    // markdown placeholder). v4 `SceneStateSchema.safeParse`.
    let mut parsed_scene_state: Option<SceneState> = None;
    let mut scene_state_malformed = false;
    if let Some(chat_id) = input.chat_id {
        if let Some(chat) = crate::db::chats_read::find_by_id(main, chat_id).map_err(db_err)? {
            if let Some(raw) = chat.get("sceneState") {
                if !raw.is_null() {
                    match serde_json::from_value::<SceneState>(raw.clone()) {
                        Ok(scene) => parsed_scene_state = Some(scene),
                        Err(_) => scene_state_malformed = true,
                    }
                }
            }
        }
    }

    let markdown = build_kept_image_markdown(&BuildKeptImageMarkdownInput {
        generation_prompt: file_entry.generation_prompt.as_deref(),
        generation_revised_prompt: file_entry.generation_revised_prompt.as_deref(),
        generation_model: file_entry.generation_model.as_deref(),
        scene_state: parsed_scene_state.as_ref(),
        scene_state_malformed,
        linked_by_name: &input.attribution.name,
        linked_by_id: input.attribution.id.as_deref(),
        linked_by_role: Some(input.attribution.role),
        tags: input.tags,
        caption: input.caption,
        kept_at,
    });
    let extracted_text_sha256 = sha256_of_string(&markdown);

    let slug = build_slug_and_filename(&BuildSlugAndFilenameInput {
        caption: input.caption,
        generation_prompt: file_entry.generation_prompt.as_deref(),
        mime_type: &file_entry.mime_type,
        kept_at,
    });
    let desired_path = build_photos_relative_path(&slug.filename);
    let relative_path =
        resolve_unique_relative_path(mount, input.mount_point_id, &desired_path).map_err(db_err)?;
    // v4 ensures the `photos/` folder path exists; link_blob_content also derives
    // it internally, so this is belt-and-braces.
    links
        .ensure_folder_path(input.mount_point_id, PHOTOS_FOLDER)
        .map_err(db_err)?;

    let result = links
        .link_blob_content(&LinkBlobInput {
            mount_point_id: input.mount_point_id.to_string(),
            relative_path: relative_path.clone(),
            file_name: basename_of_relative_path(&relative_path),
            file_type: None, // 'blob'
            original_file_name: file_entry.original_filename.clone(),
            original_mime_type: file_entry.mime_type.clone(),
            stored_mime_type: file_entry.mime_type.clone(),
            sha256: sha256.clone(), // advisory — recomputed from data
            data: buffer,
            description: Some(input.caption.unwrap_or("").to_string()),
            conversion_status: None,
            extracted_text: Some(markdown.clone()),
            extracted_text_sha256: Some(extracted_text_sha256),
            extraction_status: Some("converted".to_string()),
        })
        .map_err(db_err)?;

    // v4 `chunkAndInsertExtractedText`: delete prior chunks, chunk the markdown
    // extractedText, roll up the link's chunkCount + plainTextLength (P4.6BK —
    // the kept image is a .webp/.png link the extension-dispatched reindex
    // never picks up, so this direct chunk pass is what makes it searchable).
    chunk_and_insert_extracted_text(
        mount,
        &result.link_id,
        input.mount_point_id,
        &markdown,
        kept_at,
    )
    .map_err(db_err)?;

    // Recorded side effects (mount invalidation + embedding enqueue).
    side_effects.invalidate_mount_point(input.mount_point_id);
    side_effects.enqueue_embedding_jobs(input.mount_point_id);

    Ok(SaveImageToAlbumOutput {
        mount_point_name: mount_point.name,
        relative_path,
        link_id: result.link_id,
        kept_at: kept_at.to_string(),
        file_id: file_entry.id,
        sha256,
    })
}

/// v4 `chunkAndInsertExtractedText` (`lib/photos/chunk-extracted-text.ts`) —
/// the direct chunk pass for links whose text lives in `extractedText` (kept
/// images: a `.webp`/`.png` link carrying a Markdown context document, which
/// the extension-dispatched `reindexSingleFile` won't touch). Clears prior
/// chunks for the link, chunks the extractedText, inserts the chunk rows, and
/// rolls up `chunkCount` + `plainTextLength` (UTF-16 length). A
/// whitespace-only extractedText zeroes both. (P4.6BK: previously ported
/// without the chunker under the pinned-`<cc>` seam — the chunker now runs.)
pub(crate) fn chunk_and_insert_extracted_text(
    mount: &Connection,
    link_id: &str,
    mount_point_id: &str,
    extracted_text: &str,
    updated_at: &str,
) -> Result<(), DbError> {
    let links = DocMountFileLinksRepository::new(mount);
    let trimmed = crate::jsstr::js_trim(extracted_text);
    if trimmed.is_empty() {
        links.delete_chunks_by_link_id(link_id)?;
        links.set_chunk_rollups(link_id, 0, 0, updated_at)?;
        return Ok(());
    }
    let chunks = crate::services::mount_index::chunker::chunk_document(
        extracted_text,
        crate::services::mount_index::chunker::ChunkOptions::default(),
    );
    links.delete_chunks_by_link_id(link_id)?;
    if !chunks.is_empty() {
        crate::services::mount_index::reindex_file::insert_chunks(
            mount,
            link_id,
            mount_point_id,
            &chunks,
        )?;
    }
    let plain_text_length =
        crate::photos::keep_image_markdown::extracted_text_utf16_len(extracted_text);
    links.set_chunk_rollups(link_id, chunks.len() as i64, plain_text_length, updated_at)?;
    Ok(())
}

/// v4 `resolveUniqueRelativePath` — a free path under a mount. If `desired` is
/// taken (a `doc_mount_blobs` row at that `(mount, path)`), bumps `(2)`, `(3)`, …
/// up to 999; then a sha1-tagged suffix. The photo filename embeds a millisecond
/// ISO timestamp + slug, so a collision is effectively impossible in practice — the
/// bump/sha1 branches are ported for faithfulness but never hit by the corpus (the
/// sha1 branch is non-deterministic, a documented seam).
pub(crate) fn resolve_unique_relative_path(
    mount: &Connection,
    mount_point_id: &str,
    desired: &str,
) -> Result<String, DbError> {
    let blobs = DocMountBlobsRepository::new(mount);
    if blobs
        .find_by_mount_point_and_path(mount_point_id, desired)?
        .is_none()
    {
        return Ok(desired.to_string());
    }

    let dir = posix_dirname(desired);
    let ext = posix_extname(desired);
    let stem = posix_basename_without_ext(desired, &ext);
    let prefix = if dir == "." || dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };

    for attempt in 2..=999u32 {
        let candidate = format!("{prefix}{stem} ({attempt}){ext}");
        if blobs
            .find_by_mount_point_and_path(mount_point_id, &candidate)?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    // Non-deterministic fallback (v4 uses sha1(`${desired}:${Date.now()}`)[:8];
    // here we hash the same seed with the workspace's sha2 — a documented seam,
    // NEVER hit by the corpus because a photo filename embeds a millisecond ISO
    // timestamp + slug, so 998 collisions cannot occur).
    let hash = {
        use sha2::{Digest, Sha256};
        let seed = format!("{desired}:{}", crate::clock::now_unix_ms());
        let full = hex::encode(Sha256::digest(seed.as_bytes()));
        full[..8].to_string()
    };
    Ok(format!("{prefix}{stem}-{hash}{ext}"))
}

/// POSIX dirname (Node semantics) — the substring up to the last `/`; `"."` when
/// there is no `/`.
fn posix_dirname(p: &str) -> String {
    match p.rfind('/') {
        None => ".".to_string(),
        Some(0) => "/".to_string(),
        Some(idx) => p[..idx].to_string(),
    }
}

/// `path.extname` — the substring from the last `.` in the basename (including the
/// dot), or `""` when the basename has no interior dot.
fn posix_extname(p: &str) -> String {
    let base = match p.rfind('/') {
        None => p,
        Some(idx) => &p[idx + 1..],
    };
    match base.rfind('.') {
        // A leading dot is not an extension (Node: `.foo` → `""`).
        Some(0) | None => String::new(),
        Some(idx) => base[idx..].to_string(),
    }
}

/// `path.posix.basename(p, ext)` — the last segment with the trailing `ext` removed.
fn posix_basename_without_ext(p: &str, ext: &str) -> String {
    let base = match p.rfind('/') {
        None => p,
        Some(idx) => &p[idx + 1..],
    };
    base.strip_suffix(ext).unwrap_or(base).to_string()
}

fn db_err(e: DbError) -> SaveImageToAlbumError {
    // A genuine DB error is not one of v4's expected SaveImageToAlbumError modes;
    // surface it as an ImageNotFound-shaped error carrying the message so the
    // caller's outer catch reports it. (The tool handler rethrows non-Save errors.)
    SaveImageToAlbumError::new(SaveImageErrorCode::ImageNotFound, e.to_string())
}
