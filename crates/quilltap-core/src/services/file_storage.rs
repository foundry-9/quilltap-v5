//! The file-storage manager + byte-layer engine (P4.1b) — v4
//! `lib/file-storage/manager.ts`, `lib/file-storage/project-store-bridge.ts`,
//! `lib/file-storage/user-uploads-bridge.ts`, `lib/files/webp-conversion.ts`,
//! `lib/mount-index/blob-transcode.ts`, `lib/mount-index/store-file.ts` (the
//! database blob branch), `lib/images-v2.ts` (`createFile` / `ingestImageBuffer`
//! / `readImageBuffer`), and `lib/cascade-delete.ts` (`deleteFileCompletely`).
//!
//! The key survey fact shaping this module (see the P4.1b work order): most of
//! v4's byte layer is **mount-blob-backed** — the bridges return
//! `mount-blob:{mountPointId}:{blobId}` storage keys whose bytes live in the
//! already-ported `doc_mount_blobs` table, so the bulk of the "byte layer" is DB
//! reads/writes through ported repos. Only the legacy `<base>/files/**` disk
//! layout is real filesystem, behind the injected [`StorageBackend`] seam
//! (production: `quilltap-host`'s `LocalStorageBackend`).
//!
//! The pixel work (Sharp in v4) sits behind the injected [`PixelCodec`] seam
//! (production: `quilltap-host`'s `HostImageCodec` over the `image`/`webp`
//! crates — D19: operations are ported, byte parity of encoded pixels is NOT
//! required). The WebP **policy** (which mimes convert, the mime/filename
//! rewrites, the failure-passthrough shape) lives HERE in core, since v4
//! implements it in JS around the sharp call — with sharp mocked in the
//! differential, v4's policy still runs, so the Rust policy must too.
//!
//! Ported small helpers that also exist privately in
//! [`crate::services::image_job_storage`] (`sanitize_leaf_name`,
//! `normalise_blob_relative_path`, `resolve_unique_relative_path`,
//! `detect_blob_file_type`): duplicated deliberately — that module is frozen
//! (owned by W4.9c) and its copies are private; a post-port cleanup can unify
//! (see `[[v5-refactor-vestigial-v4-cruft]]`).
//!
//! v4's `refreshStats` (the mount-point fileCount/chunkCount/totalSizeBytes
//! aggregates, fired best-effort by `storeMountFile`) is NOT ported — the
//! standing groups/projects/image-generation precedent; the differential pins
//! those columns.

use std::sync::Arc;

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_file_links::{
    normalise_relative_path, DocMountFileLinksRepository, LinkBlobInput,
};
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::files::{CreateOptions, FileCreate, FileEntry, FileUpdate, FilesRepository};
use crate::db::runtime::Db;
use crate::db::DbError;

// ============================================================================
// Pure key/path logic (manager.ts + project-store-bridge.ts + thumbnail-utils)
// ============================================================================

/// v4 `STORAGE_KEY_PREFIX` (`project-store-bridge.ts`).
pub const MOUNT_BLOB_STORAGE_KEY_PREFIX: &str = "mount-blob:";

/// v4 `DEFAULT_THUMBNAIL_SIZE` (`thumbnail-utils.ts`).
pub const DEFAULT_THUMBNAIL_SIZE: i64 = 150;
/// v4 `MAX_THUMBNAIL_SIZE` — requested sizes are clamped to this.
pub const MAX_THUMBNAIL_SIZE: i64 = 300;
/// v4 `THUMBNAIL_QUALITY` (WebP quality for thumbnails).
pub const THUMBNAIL_QUALITY: i64 = 80;
/// v4 `COMMON_THUMBNAIL_SIZES` — cleaned up on file deletion.
pub const COMMON_THUMBNAIL_SIZES: &[i64] = &[120, 150, 300];

/// v4 `WEBP_QUALITY` (`webp-conversion.ts`) — the `convertToWebP` quality.
pub const CONVERT_WEBP_QUALITY: i64 = 90;
/// v4 blob-transcode default quality (`blob-transcode.ts` `quality ?? 85`).
pub const TRANSCODE_WEBP_QUALITY: i64 = 85;
/// v4 blob-transcode `effort: 4`.
pub const TRANSCODE_WEBP_EFFORT: i64 = 4;

/// v4 `MAX_FILE_SIZE` (`images-v2.ts`) — 10 MB image-upload cap.
pub const MAX_IMAGE_FILE_SIZE: i64 = 10 * 1024 * 1024;

/// v4 `ALLOWED_IMAGE_TYPES` (`images-v2.ts`).
pub const ALLOWED_IMAGE_TYPES: &[&str] = &[
    "image/jpeg",
    "image/jpg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/avif",
    "image/svg+xml",
];

/// v4 `UNSAFE_FILENAME_CHARS = /[\/\\:*?"<>|\x00-\x1f\x7f]/` (`manager.ts:62`).
fn is_unsafe_filename_char(c: char) -> bool {
    matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        || (c as u32) <= 0x1F
        || (c as u32) == 0x7F
}

/// v4 `safeFilename` (`manager.ts:73`): replace unsafe chars with `_`, collapse
/// `_{2,}` → `_`, trim leading/trailing `_`/`.`, fall back to `'unnamed'`.
/// NOTE: unlike [`sanitize_leaf_name`] this does NOT strip directory components
/// first — `/` itself is in the unsafe set, so path separators become `_`.
pub fn safe_filename(filename: &str) -> String {
    let mut safe: String = filename
        .chars()
        .map(|c| if is_unsafe_filename_char(c) { '_' } else { c })
        .collect();
    while safe.contains("__") {
        safe = safe.replace("__", "_");
    }
    let trimmed = safe.trim_matches(|c| c == '_' || c == '.');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// v4 `sanitizeLeafName` (`bridge-path-helpers.ts:34`): strip directory
/// components (split on `\`/`/`, take last), replace `UNSAFE_LEAF_CHARS` with
/// `_`, collapse `_{2,}`, trim leading/trailing `_`/`.`, `'unnamed'` when empty.
pub fn sanitize_leaf_name(filename: &str) -> String {
    let basename = filename.rsplit(['\\', '/']).next().unwrap_or(filename);
    safe_filename(basename)
}

/// Trim leading/trailing `/` runs (v4's `folderPath.replace(/^\/+|\/+$/g, '')`).
fn trim_slashes(s: &str) -> &str {
    s.trim_matches('/')
}

/// v4 `buildStorageKey` (`manager.ts:736`):
/// `{projectId or '_general'}/{folderPath}/{safeFilename}`.
pub fn build_storage_key(
    filename: &str,
    project_id: Option<&str>,
    folder_path: Option<&str>,
) -> String {
    let safe = safe_filename(filename);
    let project_path = match project_id {
        // JS truthiness: an empty projectId is falsy → '_general'.
        Some(p) if !p.is_empty() => p,
        _ => "_general",
    };
    let folder = folder_path.map(trim_slashes).unwrap_or("");
    if folder.is_empty() {
        format!("{project_path}/{safe}")
    } else {
        format!("{project_path}/{folder}/{safe}")
    }
}

/// v4 `buildFolderStoragePath` (`manager.ts:707`).
pub fn build_folder_storage_path(project_id: Option<&str>, folder_path: &str) -> String {
    let project_path = match project_id {
        Some(p) if !p.is_empty() => p,
        _ => "_general",
    };
    let folder = trim_slashes(folder_path);
    if folder.is_empty() {
        project_path.to_string()
    } else {
        format!("{project_path}/{folder}")
    }
}

/// v4 `buildThumbnailStorageKey` (`thumbnail-utils.ts:51`) —
/// `_thumbnails/{fileId}_{size}.webp` (the deprecated `userId` param is ignored
/// in v4 and dropped here).
pub fn build_thumbnail_storage_key(file_id: &str, size: i64) -> String {
    format!("_thumbnails/{file_id}_{size}.webp")
}

/// v4 `isMountBlobStorageKey` (`project-store-bridge.ts:70`).
pub fn is_mount_blob_storage_key(storage_key: Option<&str>) -> bool {
    matches!(storage_key, Some(k) if k.starts_with(MOUNT_BLOB_STORAGE_KEY_PREFIX))
}

/// v4 `parseMountBlobStorageKey` (`project-store-bridge.ts:78`) —
/// `(mountPointId, blobId)`, or `None` for keys that don't match the scheme.
pub fn parse_mount_blob_storage_key(storage_key: &str) -> Option<(String, String)> {
    if !storage_key.starts_with(MOUNT_BLOB_STORAGE_KEY_PREFIX) {
        return None;
    }
    let rest = &storage_key[MOUNT_BLOB_STORAGE_KEY_PREFIX.len()..];
    let sep = rest.find(':')?;
    // v4: `sep < 1 || sep === rest.length - 1` → null.
    if sep < 1 || sep == rest.len() - 1 {
        return None;
    }
    Some((rest[..sep].to_string(), rest[sep + 1..].to_string()))
}

/// v4 `buildMountBlobStorageKey` (`project-store-bridge.ts:94`).
pub fn build_mount_blob_storage_key(mount_point_id: &str, blob_id: &str) -> String {
    format!("{MOUNT_BLOB_STORAGE_KEY_PREFIX}{mount_point_id}:{blob_id}")
}

/// v4 `getFileApiPath` (`images-v2.ts:63`).
pub fn get_file_api_path(file_id: &str) -> String {
    format!("/api/v1/files/{file_id}")
}

/// JS `encodeURIComponent` over the storage key (v4 `getProxyUrl` /
/// `getFileUrl`). Unreserved set: `A-Za-z0-9 - _ . ! ~ * ' ( )`; everything
/// else percent-encodes its UTF-8 bytes. (A private sibling of the verified
/// `doc_edit::qtap_uri` encoder, which is not exported.)
fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        let c = *b as char;
        if b.is_ascii_alphanumeric()
            || matches!(c, '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')')
        {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// v4 `getFileUrl` (`manager.ts:517`) / `getProxyUrl` — both storage modes
/// serve through the same proxy route.
pub fn get_file_url(entry: &FileEntry) -> Result<String, String> {
    let key = effective_storage_key(entry)
        .ok_or_else(|| "File has no storage key. Cannot generate URL.".to_string())?;
    Ok(format!("/api/v1/files/proxy/{}", encode_uri_component(key)))
}

/// v4 `getEffectiveStorageKey` — `storageKey` when present (JS-truthy: an empty
/// string is falsy → `None`).
fn effective_storage_key(entry: &FileEntry) -> Option<&str> {
    entry.storage_key.as_deref().filter(|s| !s.is_empty())
}

// ============================================================================
// The pixel-codec seam + the WebP policies
// ============================================================================

/// The low-level pixel seam (Sharp's actual pixel work). Production:
/// `quilltap-host::HostImageCodec` (the `image` + `webp` crates). The WebP
/// *policies* (which mimes convert, rewrites, failure shape) live above this
/// seam in core — see [`convert_to_webp`] / [`transcode_to_webp`].
pub trait PixelCodec: Send + Sync {
    /// One WebP encode: v4 `sharp(buffer[, {animated}]).webp({quality[, effort]})
    /// .toBuffer()`. `Err` = the decode/encode failed (v4's sharp throw — the
    /// policy layer catches and passes the original bytes through).
    fn encode_webp(
        &self,
        bytes: &[u8],
        quality: i64,
        effort: Option<i64>,
        animated: bool,
    ) -> Result<Vec<u8>, String>;

    /// v4 `sharp(buffer).metadata()` width/height — best-effort; failure →
    /// `(None, None)` (v4's `measureDimensions` catch → `{}`).
    fn measure(&self, bytes: &[u8]) -> (Option<i64>, Option<i64>);
}

/// A codec for an instance with no host pixel stack wired: every encode fails
/// (→ the policy layers pass the original bytes through, v4's sharp-failure
/// shape) and nothing measures.
#[derive(Clone, Copy, Debug, Default)]
pub struct NotConfiguredPixelCodec;
impl PixelCodec for NotConfiguredPixelCodec {
    fn encode_webp(
        &self,
        _bytes: &[u8],
        _quality: i64,
        _effort: Option<i64>,
        _animated: bool,
    ) -> Result<Vec<u8>, String> {
        Err("no image codec is wired on this host".to_string())
    }
    fn measure(&self, _bytes: &[u8]) -> (Option<i64>, Option<i64>) {
        (None, None)
    }
}

/// A deterministic pass-through codec for the differential harness (mirrors the
/// oracle's sharp mock): every encode returns the input bytes; `measure`
/// returns the fixed dimensions the mock reports.
#[derive(Clone, Copy, Debug)]
pub struct PassthroughPixelCodec {
    pub width: i64,
    pub height: i64,
}
impl PixelCodec for PassthroughPixelCodec {
    fn encode_webp(
        &self,
        bytes: &[u8],
        _quality: i64,
        _effort: Option<i64>,
        _animated: bool,
    ) -> Result<Vec<u8>, String> {
        Ok(bytes.to_vec())
    }
    fn measure(&self, _bytes: &[u8]) -> (Option<i64>, Option<i64>) {
        (Some(self.width), Some(self.height))
    }
}

/// v4 `CONVERTIBLE_MIME_TYPES` (`webp-conversion.ts`) — exact-match set (no
/// lowercasing, faithful to the `Set.has`).
const CONVERTIBLE_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/jpg",
    "image/png",
    "image/gif",
    "image/avif",
];

/// v4 `needsWebPConversion` (`webp-conversion.ts:76`).
pub fn needs_webp_conversion(mime_type: &str) -> bool {
    CONVERTIBLE_MIME_TYPES.contains(&mime_type)
}

/// v4 `replaceExtensionWithWebP` (`webp-conversion.ts:85`) — swap the extension
/// for `.webp` (append when there's no extension; a leading-dot name like
/// `.env` counts as no extension, `dotIndex > 0`).
pub fn replace_extension_with_webp(filename: &str) -> String {
    match filename.rfind('.') {
        Some(i) if i > 0 => format!("{}.webp", &filename[..i]),
        _ => format!("{filename}.webp"),
    }
}

/// v4 `WebPConversionResult`.
#[derive(Clone, Debug)]
pub struct WebpConversionResult {
    pub buffer: Vec<u8>,
    pub mime_type: String,
    pub filename: String,
    pub was_converted: bool,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

/// v4 `convertToWebP` (`webp-conversion.ts:108`, quality 90) — the POLICY layer
/// over the pixel seam. Non-convertible mimes pass through (SVG unmeasured,
/// everything else best-effort measured); convertible mimes encode to WebP
/// (extension rewritten, OUTPUT bytes measured); an encode failure passes the
/// original through with measured dims (v4's warn-and-keep-original catch).
pub fn convert_to_webp(
    codec: &dyn PixelCodec,
    buffer: &[u8],
    mime_type: &str,
    filename: &str,
) -> WebpConversionResult {
    if !needs_webp_conversion(mime_type) {
        let (width, height) = if mime_type == "image/svg+xml" {
            (None, None)
        } else {
            codec.measure(buffer)
        };
        return WebpConversionResult {
            buffer: buffer.to_vec(),
            mime_type: mime_type.to_string(),
            filename: filename.to_string(),
            was_converted: false,
            width,
            height,
        };
    }

    match codec.encode_webp(buffer, CONVERT_WEBP_QUALITY, None, false) {
        Ok(webp) => {
            let (width, height) = codec.measure(&webp);
            WebpConversionResult {
                buffer: webp,
                mime_type: "image/webp".to_string(),
                filename: replace_extension_with_webp(filename),
                was_converted: true,
                width,
                height,
            }
        }
        Err(_) => {
            let (width, height) = codec.measure(buffer);
            WebpConversionResult {
                buffer: buffer.to_vec(),
                mime_type: mime_type.to_string(),
                filename: filename.to_string(),
                was_converted: false,
                width,
                height,
            }
        }
    }
}

/// v4 `TRANSCODABLE_MIME_TYPES` (`blob-transcode.ts`) — matched on the
/// LOWERCASED input mime (faithful to `originalMimeType.toLowerCase()`);
/// `image/webp` deliberately absent (already-WebP stored as-is).
const TRANSCODABLE_MIME_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/gif",
    "image/heic",
    "image/heif",
    "image/tiff",
    "image/avif",
];

/// v4 `TranscodeResult` (`blob-transcode.ts`).
#[derive(Clone, Debug)]
pub struct BlobTranscodeResult {
    pub data: Vec<u8>,
    pub stored_mime_type: String,
    pub size_bytes: usize,
    pub sha256: String,
}

/// v4 `transcodeToWebP` (`blob-transcode.ts:44`, quality 85, effort 4,
/// `animated: true`) — the Scriptorium blob-upload transcode policy. Non-
/// transcodable mimes (and encode failures — v4's warn-and-store-original
/// catch) pass the original bytes through; the sha is always computed over the
/// bytes actually returned.
pub fn transcode_to_webp(
    codec: &dyn PixelCodec,
    input: &[u8],
    original_mime_type: &str,
    quality: i64,
) -> BlobTranscodeResult {
    let passthrough = |bytes: &[u8]| BlobTranscodeResult {
        data: bytes.to_vec(),
        stored_mime_type: original_mime_type.to_string(),
        size_bytes: bytes.len(),
        sha256: sha256_hex(bytes),
    };

    if !TRANSCODABLE_MIME_TYPES.contains(&original_mime_type.to_lowercase().as_str()) {
        return passthrough(input);
    }
    match codec.encode_webp(input, quality, Some(TRANSCODE_WEBP_EFFORT), true) {
        Ok(webp) => BlobTranscodeResult {
            sha256: sha256_hex(&webp),
            size_bytes: webp.len(),
            stored_mime_type: "image/webp".to_string(),
            data: webp,
        },
        Err(_) => passthrough(input),
    }
}

/// v4 `normaliseBlobRelativePath` (`blob-transcode.ts:91`): force a `.webp`
/// extension when the stored mime is `image/webp`.
pub fn normalise_blob_relative_path(relative_path: &str, stored_mime_type: &str) -> String {
    if stored_mime_type != "image/webp" {
        return relative_path.to_string();
    }
    if relative_path.to_lowercase().ends_with(".webp") {
        return relative_path.to_string();
    }
    let last_dot = relative_path.rfind('.');
    let last_slash = relative_path.rfind('/');
    match last_dot {
        Some(dot) if last_slash.map(|s| dot > s).unwrap_or(true) => {
            format!("{}.webp", &relative_path[..dot])
        }
        _ => format!("{relative_path}.webp"),
    }
}

/// Lowercase-hex SHA-256 (v4 `sha256OfBuffer` / `createHash('sha256')`).
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ============================================================================
// The disk-backend seam (v4 LocalFileStorageBackend)
// ============================================================================

/// The disk half of the byte layer — v4's `FileStorageBackend` subset the
/// manager drives (`upload`/`download`/`delete`/`exists` by storage key).
/// Production: `quilltap-host::LocalStorageBackend` (the `<base>/files/**`
/// layout with the traversal guard + fs retry). Errors are v4's thrown
/// message strings.
pub trait StorageBackend: Send + Sync {
    fn upload(&self, key: &str, content: &[u8], content_type: &str) -> Result<(), String>;
    fn download(&self, key: &str) -> Result<Vec<u8>, String>;
    /// Idempotent (a missing file succeeds — v4's ENOENT-tolerant delete).
    fn delete(&self, key: &str) -> Result<(), String>;
    fn exists(&self, key: &str) -> Result<bool, String>;
}

/// A backend for an instance with no disk layer wired: every op fails loud
/// (faithful to "file storage is not configured"). Instances whose files are
/// all mount-blob-backed never reach it.
#[derive(Clone, Copy, Debug, Default)]
pub struct NotConfiguredStorageBackend;
impl StorageBackend for NotConfiguredStorageBackend {
    fn upload(&self, _key: &str, _content: &[u8], _content_type: &str) -> Result<(), String> {
        Err("file storage is not configured on this host".to_string())
    }
    fn download(&self, _key: &str) -> Result<Vec<u8>, String> {
        Err("file storage is not configured on this host".to_string())
    }
    fn delete(&self, _key: &str) -> Result<(), String> {
        Err("file storage is not configured on this host".to_string())
    }
    fn exists(&self, _key: &str) -> Result<bool, String> {
        Err("file storage is not configured on this host".to_string())
    }
}

// ============================================================================
// Manager ops (v4 FileStorageManager) — reads over the Db read pool
// ============================================================================

/// v4 `readMountBlob` (`project-store-bridge.ts:174`) — the bytes for a
/// mount-blob storage key, `None` when the key is malformed or the blob row is
/// gone. Reads off the mount-index read pool (safe from any thread, including
/// the writer).
fn read_mount_blob(db: &Db, storage_key: &str) -> Result<Option<Vec<u8>>, DbError> {
    let Some((_mp, blob_id)) = parse_mount_blob_storage_key(storage_key) else {
        return Ok(None);
    };
    db.read_mount_index(move |c| DocMountBlobsRepository::new(c).read_data(&blob_id))
}

/// v4 `mountBlobExists` (`project-store-bridge.ts:184`).
fn mount_blob_exists(db: &Db, storage_key: &str) -> Result<bool, DbError> {
    let Some((_mp, blob_id)) = parse_mount_blob_storage_key(storage_key) else {
        return Ok(false);
    };
    Ok(db
        .read_mount_index(move |c| DocMountBlobsRepository::new(c).find_by_id(&blob_id))?
        .is_some())
}

/// v4 `FileStorageManager.downloadFile` (`manager.ts:378`) — dispatch on the
/// storage key: `mount-blob:` → the ported `doc_mount_blobs` read, else the
/// disk backend. Error strings match v4's wrapper.
pub fn download_file(
    db: &Db,
    backend: &dyn StorageBackend,
    entry: &FileEntry,
) -> Result<Vec<u8>, String> {
    let inner = || -> Result<Vec<u8>, String> {
        let key = effective_storage_key(entry)
            .ok_or_else(|| "File has no storage key. Cannot download.".to_string())?;
        if is_mount_blob_storage_key(Some(key)) {
            let bytes = read_mount_blob(db, key).map_err(|e| e.to_string())?;
            return bytes.ok_or_else(|| format!("Mount-blob not found for storageKey: {key}"));
        }
        backend.download(key)
    };
    inner().map_err(|msg| {
        format!(
            "Failed to download file '{}': {msg}",
            entry.original_filename
        )
    })
}

/// v4 `FileStorageManager.fileExists` (`manager.ts:554`).
pub fn file_exists(
    db: &Db,
    backend: &dyn StorageBackend,
    entry: &FileEntry,
) -> Result<bool, String> {
    let Some(key) = effective_storage_key(entry) else {
        // v4 warns "assuming does not exist" and returns false.
        return Ok(false);
    };
    storage_key_exists(db, backend, key)
}

/// v4 `FileStorageManager.storageKeyExists` (`manager.ts:590`).
pub fn storage_key_exists(
    db: &Db,
    backend: &dyn StorageBackend,
    key: &str,
) -> Result<bool, String> {
    if is_mount_blob_storage_key(Some(key)) {
        return mount_blob_exists(db, key)
            .map_err(|e| format!("Failed to check if storage key exists: {e}"));
    }
    backend
        .exists(key)
        .map_err(|msg| format!("Failed to check if storage key exists: {msg}"))
}

/// v4 `FileStorageManager.uploadRaw` (`manager.ts:467`) — write at an explicit
/// key (thumbnail caching), disk-only.
pub fn upload_raw(
    backend: &dyn StorageBackend,
    storage_key: &str,
    content: &[u8],
    content_type: &str,
) -> Result<(), String> {
    backend
        .upload(storage_key, content, content_type)
        .map_err(|msg| format!("Failed to upload raw content at '{storage_key}': {msg}"))
}

/// v4 `FileStorageManager.deleteRaw` (`manager.ts:492`).
pub fn delete_raw(backend: &dyn StorageBackend, storage_key: &str) -> Result<(), String> {
    backend
        .delete(storage_key)
        .map_err(|msg| format!("Failed to delete raw content at '{storage_key}': {msg}"))
}

/// v4 `deleteMountBlob` (`project-store-bridge.ts:198`) — drop every link to
/// the blob's file with GC (shared bytes survive while any other link
/// references them). Conn-level (a mount-index WRITE — callers run it on the
/// writer). v4's best-effort `refreshStats` is not ported (see module docs).
pub fn delete_mount_blob_conn(mount: &Connection, storage_key: &str) -> Result<(), DbError> {
    let Some((_mp, blob_id)) = parse_mount_blob_storage_key(storage_key) else {
        return Ok(());
    };
    let blobs = DocMountBlobsRepository::new(mount);
    let Some(blob) = blobs.find_by_id(&blob_id)? else {
        return Ok(());
    };
    let links = DocMountFileLinksRepository::new(mount);
    for link in links.find_by_file_id(&blob.file_id)? {
        links.delete_with_gc(&link.id)?;
    }
    Ok(())
}

/// v4 `FileStorageManager.deleteFile` (`manager.ts:418`) — mount-blob keys go
/// through the GC-aware delete (a writer round-trip); disk keys through the
/// backend (idempotent). A missing storage key is a warn-and-skip.
pub async fn delete_file(
    db: &Db,
    backend: &dyn StorageBackend,
    entry: &FileEntry,
) -> Result<(), String> {
    let Some(key) = effective_storage_key(entry) else {
        return Ok(()); // v4: warn + skip.
    };
    if is_mount_blob_storage_key(Some(key)) {
        let key = key.to_string();
        return db
            .write(move |ws| {
                let mount = ws.mount_index().ok_or_else(|| {
                    DbError::Key("mount-blob delete requires the mount-index database".to_string())
                })?;
                delete_mount_blob_conn(mount.connection(), &key)
            })
            .await
            .map_err(|e| format!("Failed to delete file '{}': {e}", entry.original_filename));
    }
    backend
        .delete(key)
        .map_err(|msg| format!("Failed to delete file '{}': {msg}", entry.original_filename))
}

/// v4 `deleteFileCompletely` (`cascade-delete.ts:26`) — the GC-safe delete
/// chokepoint: storage bytes first (best-effort — a storage failure is logged
/// and the metadata delete proceeds, faithful to v4's catch), then the `files`
/// metadata row. Returns whether the row was deleted (`false` when the file
/// doesn't exist).
///
/// The W4.8 maintenance sweep currently deletes the metadata row only (its
/// seeded files carry NULL storage keys, so v4's storage delete no-ops there);
/// routing the sweep through this chokepoint is a one-line unification edit
/// tracked as a handoff.
pub async fn delete_file_completely(
    db: &Db,
    backend: &dyn StorageBackend,
    file_id: &str,
) -> Result<bool, DbError> {
    let fid = file_id.to_string();
    let entry = db.read_main(move |c| FilesRepository::new(c).find_by_id(&fid))?;
    let Some(entry) = entry else {
        return Ok(false);
    };

    if effective_storage_key(&entry).is_some() {
        // Best-effort: v4 logs the storage failure and continues to the
        // metadata delete.
        let _ = delete_file(db, backend, &entry).await;
    }

    let fid = file_id.to_string();
    db.write(move |ws| FilesRepository::new(ws.main().connection()).delete(&fid))
        .await
}

// ============================================================================
// The storeMountFile blob branch (v4 store-file.ts, database storage)
// ============================================================================

/// What the blob store pipeline produced (the `StoreFileResult` subset the
/// bridges consume).
#[derive(Clone, Debug)]
pub struct StoredBlob {
    pub mount_point_id: String,
    pub relative_path: String,
    pub file_id: String,
    pub link_id: String,
    pub blob_id: String,
    pub sha256: String,
    pub size_bytes: usize,
    pub stored_mime_type: String,
}

impl StoredBlob {
    /// The `mount-blob:` storage key the bridges build from the result.
    pub fn storage_key(&self) -> String {
        build_mount_blob_storage_key(&self.mount_point_id, &self.blob_id)
    }
}

/// v4 `detectBlobFileType` (`store-file.ts:117`).
fn detect_blob_file_type(relative_path: &str) -> String {
    let base = relative_path.rsplit('/').next().unwrap_or(relative_path);
    let ext = match base.rfind('.') {
        Some(i) if i > 0 => base[i..].to_lowercase(),
        _ => String::new(),
    };
    match ext.as_str() {
        ".pdf" => "pdf".to_string(),
        ".docx" => "docx".to_string(),
        _ => "blob".to_string(),
    }
}

/// v4 `resolveUniqueRelativePath` (`bridge-path-helpers.ts`, the
/// `unique-suffix` strategy): bump `(2)`, `(3)`, … to 999. The sha1-tagged
/// fallback past 999 is a documented non-deterministic seam (never hit — a
/// same-name 1000-way collision).
fn resolve_unique_relative_path(
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
    let (dir, base) = match desired.rfind('/') {
        Some(i) if i > 0 => (Some(&desired[..i]), &desired[i + 1..]),
        _ => (None, desired),
    };
    let (stem, ext) = match base.rfind('.') {
        Some(i) if i > 0 => (&base[..i], &base[i..]),
        _ => (base, ""),
    };
    let prefix = dir.map(|d| format!("{d}/")).unwrap_or_default();
    for attempt in 2..=999u32 {
        let candidate = format!("{prefix}{stem} ({attempt}){ext}");
        if blobs
            .find_by_mount_point_and_path(mount_point_id, &candidate)?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    Ok(format!("{prefix}{stem}-dup{ext}"))
}

/// Input to [`store_mount_blob`] — the `StoreFileInput` subset the bridges use
/// (`collisionStrategy: 'unique-suffix'`, `treatNativeTextAsDocument: false`,
/// `extractText: false`, `enqueueEmbedding: false`, `assetStorage: 'database'`).
pub struct StoreMountBlobInput<'a> {
    pub mount_point_id: &'a str,
    pub relative_path: &'a str,
    pub data: &'a [u8],
    pub original_mime_type: &'a str,
    pub original_file_name: &'a str,
    pub description: Option<&'a str>,
    /// v4 `transcodeImages` (both bridges pass `true`).
    pub transcode_images: bool,
}

/// v4 `storeMountFile`'s database binary-blob branch (`store-file.ts:247+`)
/// with the bridges' fixed options: transcode → extension normalise →
/// unique-suffix collision bump → folder ensure → stale-chunk drop on an
/// existing same-path link → `linkBlobContent`. The PDF/DOCX text extraction
/// (`extractText`) and the reindex/embedding enqueue are out of the bridges'
/// option set; `refreshStats` is unported (module docs).
pub fn store_mount_blob(
    mount: &Connection,
    codec: &dyn PixelCodec,
    input: &StoreMountBlobInput<'_>,
) -> Result<StoredBlob, DbError> {
    let rel = normalise_relative_path(input.relative_path)?;

    let transcoded = if input.transcode_images {
        transcode_to_webp(
            codec,
            input.data,
            input.original_mime_type,
            TRANSCODE_WEBP_QUALITY,
        )
    } else {
        BlobTranscodeResult {
            data: input.data.to_vec(),
            stored_mime_type: input.original_mime_type.to_string(),
            size_bytes: input.data.len(),
            sha256: sha256_hex(input.data),
        }
    };

    let mut final_path = normalise_blob_relative_path(&rel, &transcoded.stored_mime_type);
    final_path = resolve_unique_relative_path(mount, input.mount_point_id, &final_path)?;

    let links = DocMountFileLinksRepository::new(mount);
    if let Some(dir) = final_path.rfind('/').and_then(|i| {
        if i > 0 {
            Some(final_path[..i].to_string())
        } else {
            None
        }
    }) {
        links.ensure_folder_path(input.mount_point_id, &dir)?;
    }

    // 'overwrite'/force re-point an existing link at new content — drop its
    // stale chunks first (v4 store-file.ts; under unique-suffix an existing
    // LINK at a blob-free path can still collide — e.g. a text document there).
    if let Some(existing) = links.find_by_mount_point_and_path(input.mount_point_id, &final_path)? {
        links.delete_chunks_by_link_id(&existing.id)?;
    }

    let file_name = final_path
        .rsplit('/')
        .next()
        .unwrap_or(&final_path)
        .to_string();
    let written = links.link_blob_content(&LinkBlobInput {
        mount_point_id: input.mount_point_id.to_string(),
        relative_path: final_path.clone(),
        file_name,
        file_type: Some(detect_blob_file_type(&final_path)),
        original_file_name: input.original_file_name.to_string(),
        original_mime_type: input.original_mime_type.to_string(),
        stored_mime_type: transcoded.stored_mime_type.clone(),
        sha256: transcoded.sha256.clone(),
        data: transcoded.data.clone(),
        // v4 `description: input.description ?? ''` — LinkBlobInput's None → ''.
        description: input.description.map(str::to_string),
        conversion_status: None,
        extracted_text: None,
        extracted_text_sha256: None,
        extraction_status: None,
    })?;

    Ok(StoredBlob {
        mount_point_id: input.mount_point_id.to_string(),
        relative_path: final_path,
        file_id: written.file_id,
        link_id: written.link_id,
        blob_id: written.blob_id,
        sha256: transcoded.sha256,
        size_bytes: transcoded.size_bytes,
        stored_mime_type: transcoded.stored_mime_type,
    })
}

// ============================================================================
// The user-uploads bridge (v4 user-uploads-bridge.ts)
// ============================================================================

/// v4 `getUserUploadsStore` (`user-uploads-bridge.ts:52`) — the provisioned
/// "Quilltap Uploads" mount id, or `None` (unprovisioned / deleted /
/// non-database). Fails soft (v4's catch → null).
pub fn get_user_uploads_store(main: &Connection, mount: &Connection) -> Option<String> {
    let id = crate::db::instance_settings::get_user_uploads_mount_point_id(main)
        .ok()
        .flatten()?;
    let row = DocMountPointsRepository::new(mount)
        .find_by_id_for_docedit(&id)
        .ok()
        .flatten()?;
    if row.mount_type != "database" {
        return None;
    }
    Some(id)
}

/// v4 `writeUserUploadToMountStore` (`user-uploads-bridge.ts:94`) — land a
/// project-less upload in the Quilltap Uploads mount under `<subfolder>/`.
/// `Err` when the mount is unprovisioned (v4's throw). (The argument list
/// mirrors v4's input bag; kept flat like the image_job_storage precedent.)
#[allow(clippy::too_many_arguments)]
pub fn write_user_upload_to_mount_store(
    main: &Connection,
    mount: &Connection,
    codec: &dyn PixelCodec,
    filename: &str,
    content: &[u8],
    content_type: &str,
    subfolder: &str,
    description: Option<&str>,
) -> Result<StoredBlob, DbError> {
    let Some(mount_point_id) = get_user_uploads_store(main, mount) else {
        return Err(DbError::Key(
            "Quilltap Uploads mount has not been provisioned".to_string(),
        ));
    };
    let safe_name = sanitize_leaf_name(filename);
    let desired = format!("{subfolder}/{safe_name}");
    store_mount_blob(
        mount,
        codec,
        &StoreMountBlobInput {
            mount_point_id: &mount_point_id,
            relative_path: &desired,
            data: content,
            original_mime_type: content_type,
            original_file_name: &safe_name,
            description,
            transcode_images: true,
        },
    )
}

// ============================================================================
// The project-store bridge (v4 project-store-bridge.ts) + ProjectImageUpload
// ============================================================================

/// v4 `normaliseFolderDir` (`project-store-bridge.ts`, internal): trim slashes,
/// backslashes → `/`, per-segment leaf sanitize, drop empty segments.
fn normalise_folder_dir(folder_path: Option<&str>) -> String {
    let Some(folder_path) = folder_path else {
        return String::new();
    };
    let replaced = folder_path.replace('\\', "/");
    let trimmed = replaced.trim_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed
        .split('/')
        .map(|seg| {
            seg.chars()
                .map(|c| if is_unsafe_filename_char(c) { '_' } else { c })
                .collect::<String>()
        })
        .filter(|seg| !seg.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// v4 `getProjectDocumentStore` (`project-store-bridge.ts:43`) — the primary
/// database-backed store linked to a project, or `None`. Fails soft.
pub fn get_project_document_store(
    main: &Connection,
    mount: &Connection,
    project_id: &str,
) -> Option<String> {
    if project_id.is_empty() {
        return None;
    }
    let link_ids = crate::db::project_doc_mount_links::ProjectDocMountLinksRepository::new(main)
        .find_by_project_id(project_id)
        .ok()?;
    if link_ids.is_empty() {
        return None;
    }
    let points = DocMountPointsRepository::new(mount);
    let mut stores = Vec::new();
    for mp_id in &link_ids {
        if let Ok(Some(row)) = points.find_store_naming_by_id(mp_id) {
            stores.push(row);
        }
    }
    crate::db::project_store_naming::pick_primary_project_store(&stores).map(|s| s.id.clone())
}

/// v4 `writeProjectFileToMountStore` (`project-store-bridge.ts:127`). `Err`
/// when the project has no linked database-backed store (v4's throw). (Flat
/// v4-input-bag argument list, the image_job_storage precedent.)
#[allow(clippy::too_many_arguments)]
pub fn write_project_file_to_mount_store(
    main: &Connection,
    mount: &Connection,
    codec: &dyn PixelCodec,
    project_id: &str,
    filename: &str,
    content: &[u8],
    content_type: &str,
    folder_path: Option<&str>,
    description: Option<&str>,
) -> Result<StoredBlob, DbError> {
    let Some(mount_point_id) = get_project_document_store(main, mount, project_id) else {
        return Err(DbError::Key(format!(
            "Project {project_id} has no linked database-backed document store"
        )));
    };
    let safe_name = sanitize_leaf_name(filename);
    let folder_dir = normalise_folder_dir(folder_path);
    let desired = if folder_dir.is_empty() {
        safe_name.clone()
    } else {
        format!("{folder_dir}/{safe_name}")
    };
    store_mount_blob(
        mount,
        codec,
        &StoreMountBlobInput {
            mount_point_id: &mount_point_id,
            relative_path: &desired,
            data: content,
            original_mime_type: content_type,
            original_file_name: &safe_name,
            description,
            transcode_images: true,
        },
    )
}

/// The production [`crate::services::image_job_common::ProjectImageUpload`] —
/// v4 `fileStorageManager.uploadFile`'s project route (`manager.ts:296`):
/// `writeProjectFileToMountStore` over the ported mount-store write path,
/// returning the post-transcode mime/size.
///
/// The frozen seam trait is INFALLIBLE while v4's `uploadFile` throws (failing
/// the job); on an upload failure this impl returns a
/// `fs-seam:error:<message>` sentinel storage key — a flagged trait-shape gap
/// (handoff: widen `ProjectImageUpload` to `Result` at a unification pass; the
/// job corpora keep the database-backed branches primary).
pub struct RealProjectImageUpload {
    pub db: Db,
    pub codec: Arc<dyn PixelCodec>,
}

impl crate::services::image_job_common::ProjectImageUpload for RealProjectImageUpload {
    async fn upload(
        &self,
        filename: &str,
        content: &[u8],
        content_type: &str,
        project_id: &str,
        folder_path: &str,
    ) -> crate::services::image_job_common::ProjectUploadResult {
        let filename = filename.to_string();
        let content_vec = content.to_vec();
        let content_type_owned = content_type.to_string();
        let project_id = project_id.to_string();
        let folder_path = folder_path.to_string();
        let codec = Arc::clone(&self.codec);
        let result = self
            .db
            .write(move |ws| {
                let mount = ws
                    .mount_index()
                    .ok_or_else(|| {
                        DbError::Key(
                            "project image upload requires the mount-index database".to_string(),
                        )
                    })?
                    .connection();
                let main = ws.main().connection();
                write_project_file_to_mount_store(
                    main,
                    mount,
                    codec.as_ref(),
                    &project_id,
                    &filename,
                    &content_vec,
                    &content_type_owned,
                    Some(&folder_path),
                    None,
                )
            })
            .await;
        match result {
            Ok(stored) => crate::services::image_job_common::ProjectUploadResult {
                storage_key: stored.storage_key(),
                stored_mime_type: stored.stored_mime_type,
                size_bytes: stored.size_bytes,
            },
            Err(e) => crate::services::image_job_common::ProjectUploadResult {
                storage_key: format!("fs-seam:error:{e}"),
                stored_mime_type: content_type.to_string(),
                size_bytes: content.len(),
            },
        }
    }
}

// ============================================================================
// createFile / ingestImageBuffer (v4 images-v2.ts)
// ============================================================================

/// v4 `CreateFileParams` (`images-v2.ts:67`), the ingest subset.
pub struct IngestParams {
    pub buffer: Vec<u8>,
    pub original_filename: String,
    pub mime_type: String,
    pub user_id: String,
    pub linked_to: Vec<String>,
    /// Explicit tags merged AHEAD of the inherited ones (v4 `tags` — the
    /// ingest path always passes `[]`).
    pub tags: Vec<String>,
    /// v4 `source` (`ingestImageBuffer` default `'IMPORTED'`).
    pub source: String,
    pub description: Option<String>,
}

/// What `createFile` did (for the differential's per-op record; v4 returns the
/// FileEntry only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateFileOutcome {
    /// A dedup hit returned the existing entry unchanged.
    DedupExisting,
    /// A dedup hit merged new `linkedTo` ids (mints `updatedAt`).
    DedupLinkedToMerged,
    /// A fresh row was created (`orphan_cleaned` when a stale metadata row for
    /// the same sha was deleted first — v4's storage-existence recheck).
    Created { orphan_cleaned: bool },
}

/// Read a file row's `linkedTo` JSON array (the Zod `.default([])` shape).
fn read_linked_to(main: &Connection, file_id: &str) -> Result<Vec<String>, DbError> {
    let raw: Option<String> = main
        .query_row(
            "SELECT linkedTo FROM files WHERE id = ?1",
            params![file_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .map(|o| o.unwrap_or_default())
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(raw
        .map(|s| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default())
        .unwrap_or_default())
}

/// Conn-level `fileExists` (the dedup recheck inside `createFile`): mount-blob
/// keys check `doc_mount_blobs` on the held mount connection; disk keys the
/// backend. Any error → `false` (v4's try/catch around the recheck).
fn file_exists_conn(mount: &Connection, backend: &dyn StorageBackend, entry: &FileEntry) -> bool {
    let Some(key) = effective_storage_key(entry) else {
        return false;
    };
    if let Some((_mp, blob_id)) = parse_mount_blob_storage_key(key) {
        return matches!(
            DocMountBlobsRepository::new(mount).find_by_id(&blob_id),
            Ok(Some(_))
        );
    }
    backend.exists(key).unwrap_or(false)
}

/// v4 `getInheritedTags` (`lib/files/tag-inheritance.ts`) — merge tags from
/// each linked entity, trying character → chat → connection profile → image
/// profile (first owner-matched hit wins per id; per-entity errors swallowed).
/// The embedding-profile tier is a graceful fall-through (its scoped v5 read
/// carries no tags; a non-matching id falls off the chain exactly as v4's
/// not-found does) — the same documented seam as the W4.9a copy.
pub(crate) fn get_inherited_tags(
    main: &Connection,
    mount: &Connection,
    linked_entity_ids: &[String],
    user_id: &str,
) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut add_tags = |v: &Value| {
        if let Some(tags) = v.get("tags").and_then(Value::as_array) {
            for t in tags {
                if let Some(s) = t.as_str() {
                    if seen.insert(s.to_string()) {
                        all.push(s.to_string());
                    }
                }
            }
        }
    };
    for entity_id in linked_entity_ids {
        if let Ok(Some(c)) = crate::db::characters_read::find_by_id(main, mount, entity_id) {
            if c.get("userId").and_then(Value::as_str) == Some(user_id) {
                add_tags(&c);
                continue;
            }
        }
        if let Ok(Some(chat)) = crate::db::chats_read::find_by_id(main, entity_id) {
            if chat.get("userId").and_then(Value::as_str) == Some(user_id) {
                add_tags(&chat);
                continue;
            }
        }
        if let Ok(Some(cp)) = crate::db::connection_profiles::find_by_id(main, entity_id) {
            if cp.get("userId").and_then(Value::as_str) == Some(user_id) {
                add_tags(&cp);
                continue;
            }
        }
        if let Ok(Some(ip)) = crate::db::image_profiles::find_by_id(main, entity_id) {
            if ip.get("userId").and_then(Value::as_str) == Some(user_id) {
                add_tags(&ip);
                continue;
            }
        }
    }
    all
}

/// v4 `mergeTags` — `Set([...existing, ...inherited])` insertion order.
fn merge_tags(existing: &[String], inherited: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for t in existing.iter().chain(inherited.iter()) {
        if seen.insert(t.as_str()) {
            out.push(t.clone());
        }
    }
    out
}

/// v4 `createFile` (`images-v2.ts:87`) — the images-v2 ingest engine:
/// auto-`convertToWebP` (raster non-SVG), sha over the (possibly converted)
/// bytes, hash-dedup with the storage-existence recheck (bytes gone → the
/// orphaned metadata row is deleted and a fresh upload proceeds), the
/// user-uploads bridge write (`subfolder: 'images'`), tag inheritance, then the
/// `files` metadata row recording the POST-transcode mime/size + the
/// mount-blob storage key. Mints the file id (`crypto.randomUUID`) +
/// timestamps (the differential's remap form). Conn-level — runs on the
/// writer; `dims` is the pre-convert probe `getImageDimensions` result
/// (`width || null`: a zero measures as absent).
#[allow(clippy::too_many_arguments)]
pub fn create_file_conns(
    main: &Connection,
    mount: &Connection,
    codec: &dyn PixelCodec,
    backend: &dyn StorageBackend,
    params: &IngestParams,
    category: &str,
    dims: (Option<i64>, Option<i64>),
) -> Result<(FileEntry, CreateFileOutcome), DbError> {
    let mut buffer = params.buffer.clone();
    let mut original_filename = params.original_filename.clone();
    let mut mime_type = params.mime_type.clone();

    // Auto-convert raster images to WebP (SVG passes through unchanged).
    if mime_type.starts_with("image/") && mime_type != "image/svg+xml" {
        let converted = convert_to_webp(codec, &buffer, &mime_type, &original_filename);
        if converted.was_converted {
            buffer = converted.buffer;
            mime_type = converted.mime_type;
            original_filename = converted.filename;
        }
    }

    let sha256 = sha256_hex(&buffer);
    let files = FilesRepository::new(main);

    // Hash dedup with the storage-existence recheck.
    let existing_files = files.find_by_sha256(&sha256)?;
    let mut orphan_cleaned = false;
    if let Some(existing) = existing_files.first() {
        let exists = file_exists_conn(mount, backend, existing);
        if exists {
            let current = read_linked_to(main, &existing.id)?;
            let updated = merge_tags(&current, &params.linked_to);
            if updated.len() > current.len() {
                let now = crate::clock::now_iso();
                files.update(
                    &existing.id,
                    &FileUpdate {
                        linked_to: Some(updated),
                        updated_at: now,
                        ..Default::default()
                    },
                )?;
                let refreshed = files
                    .find_by_id(&existing.id)?
                    .unwrap_or_else(|| existing.clone());
                return Ok((refreshed, CreateFileOutcome::DedupLinkedToMerged));
            }
            return Ok((existing.clone(), CreateFileOutcome::DedupExisting));
        }
        // Bytes are missing — delete the orphaned metadata and re-upload.
        files.delete(&existing.id)?;
        orphan_cleaned = true;
    }

    let file_id = uuid::Uuid::new_v4().to_string();

    // Project-less by construction — the images/ subfolder of Quilltap Uploads.
    let written = write_user_upload_to_mount_store(
        main,
        mount,
        codec,
        &original_filename,
        &buffer,
        &mime_type,
        "images",
        None,
    )?;

    let inherited = get_inherited_tags(main, mount, &params.linked_to, &params.user_id);
    let final_tags = merge_tags(&params.tags, &inherited);

    let now = crate::clock::now_iso();
    files.create(
        &FileCreate {
            user_id: params.user_id.clone(),
            sha256: sha256.clone(),
            original_filename: original_filename.clone(),
            // Record the STORED mime/size (the bridge may have transcoded).
            mime_type: written.stored_mime_type.clone(),
            size: written.size_bytes as f64,
            // v4 `width || null` — zero is falsy.
            width: dims.0.filter(|w| *w != 0).map(|w| w as f64),
            height: dims.1.filter(|h| *h != 0).map(|h| h as f64),
            is_plain_text: None,
            linked_to: params.linked_to.clone(),
            source: params.source.clone(),
            category: category.to_string(),
            generation_prompt: None,
            generation_model: None,
            generation_revised_prompt: None,
            // v4 `description || null` — an empty string is falsy.
            description: params.description.clone().filter(|d| !d.is_empty()),
            tags: final_tags,
            project_id: None,
            folder_path: None,
            storage_key: Some(written.storage_key()),
            file_status: "ok".to_string(),
        },
        &CreateOptions {
            id: file_id.clone(),
            created_at: now.clone(),
            updated_at: now,
        },
    )?;

    let entry = files
        .find_by_id(&file_id)?
        .ok_or_else(|| DbError::Key("createFile: created row not found".to_string()))?;
    Ok((entry, CreateFileOutcome::Created { orphan_cleaned }))
}

/// v4 `ingestImageBuffer` (`images-v2.ts:329`) — probe the ORIGINAL bytes'
/// dimensions (best-effort), then `createFile` with `category: 'IMAGE'` and
/// `source ?? 'IMPORTED'`.
pub fn ingest_image_buffer_conns(
    main: &Connection,
    mount: &Connection,
    codec: &dyn PixelCodec,
    backend: &dyn StorageBackend,
    params: &IngestParams,
) -> Result<(FileEntry, CreateFileOutcome), DbError> {
    let dims = codec.measure(&params.buffer);
    create_file_conns(main, mount, codec, backend, params, "IMAGE", dims)
}

// ============================================================================
// The production FileBytesStore (both seams) — v4 fileStorageManager + images-v2
// ============================================================================

/// The production byte store implementing BOTH `FileBytesStore` seams:
///
/// * [`crate::services::chat_files::FileBytesStore`] — `download_file` = v4
///   `fileStorageManager.downloadFile(entry)` (mount-blob → the ported
///   `doc_mount_blobs` read off the read pool; else the disk backend);
/// * [`crate::photos::save_image_to_album::FileBytesStore`] —
///   `read_image_buffer` = v4 `readImageBuffer` over the same dispatch, and
///   `ingest_image_buffer` = the v4 `ingestImageBuffer` flow.
///
/// **Writer-thread caveat (tracked handoff):** `ingest_image_buffer` submits a
/// write via `write_blocking`, so it must NOT be invoked from the writer
/// thread itself — the `keep_image` mount-blob-fallback ingest runs inside a
/// `Db::write` closure, where this impl fails LOUD (a typed error, never a
/// deadlock). Closing that path needs the tool executor to hand the photo
/// handlers a connection-scoped ingest (an executor-owner wiring edit).
pub struct ProductionFileBytes {
    pub db: Db,
    pub backend: Arc<dyn StorageBackend>,
    pub codec: Arc<dyn PixelCodec>,
}

impl crate::services::chat_files::FileBytesStore for ProductionFileBytes {
    fn download_file(&self, entry: &FileEntry) -> Result<Vec<u8>, String> {
        download_file(&self.db, self.backend.as_ref(), entry)
    }
}

impl crate::photos::save_image_to_album::FileBytesStore for ProductionFileBytes {
    fn read_image_buffer(&self, entry: &FileEntry) -> Result<Option<Vec<u8>>, String> {
        // v4 readImageBuffer throws on any failure; the photos caller folds a
        // missing/failed read into EMPTY_BYTES either way.
        download_file(&self.db, self.backend.as_ref(), entry).map(Some)
    }

    fn ingest_image_buffer(
        &self,
        req: &crate::photos::save_image_to_album::IngestImageRequest,
    ) -> Result<FileEntry, String> {
        if std::thread::current().name() == Some("quilltap-writer") {
            return Err(
                "ingest_image_buffer cannot run on the writer thread (a keep_image \
                 mount-blob-fallback ingest needs a connection-scoped store — see \
                 ProductionFileBytes docs)"
                    .to_string(),
            );
        }
        let params = IngestParams {
            buffer: req.buffer.clone(),
            original_filename: req.original_filename.clone(),
            mime_type: req.mime_type.clone(),
            user_id: req.user_id.clone(),
            linked_to: Vec::new(),
            tags: Vec::new(),
            source: "IMPORTED".to_string(),
            description: None,
        };
        let backend = Arc::clone(&self.backend);
        let codec = Arc::clone(&self.codec);
        self.db
            .write_blocking(move |ws| {
                let mount = ws
                    .mount_index()
                    .ok_or_else(|| {
                        DbError::Key("image ingest requires the mount-index database".to_string())
                    })?
                    .connection();
                let main = ws.main().connection();
                ingest_image_buffer_conns(main, mount, codec.as_ref(), backend.as_ref(), &params)
                    .map(|(entry, _)| entry)
            })
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_filename_matches_v4() {
        assert_eq!(safe_filename("photo one?.png"), "photo one_.png");
        assert_eq!(safe_filename("a/b\\c:d.png"), "a_b_c_d.png");
        // Collapse (`__` → `_`) runs BEFORE the edge trim in v4.
        assert_eq!(safe_filename("__lead__.trail.."), "lead_.trail");
        assert_eq!(safe_filename("<<<>>>"), "unnamed");
        assert_eq!(safe_filename(""), "unnamed");
        // Keeps case and spaces.
        assert_eq!(safe_filename("My File.PNG"), "My File.PNG");
    }

    #[test]
    fn sanitize_leaf_strips_directories_safe_filename_does_not() {
        assert_eq!(sanitize_leaf_name("dir/sub/file.png"), "file.png");
        assert_eq!(safe_filename("dir/sub/file.png"), "dir_sub_file.png");
    }

    #[test]
    fn storage_key_shapes() {
        assert_eq!(
            build_storage_key("a.png", Some("p1"), Some("/fold/er/")),
            "p1/fold/er/a.png"
        );
        assert_eq!(build_storage_key("a.png", None, None), "_general/a.png");
        assert_eq!(build_storage_key("a.png", Some(""), None), "_general/a.png");
        assert_eq!(build_folder_storage_path(Some("p1"), "/x/"), "p1/x");
        assert_eq!(build_folder_storage_path(None, ""), "_general");
        assert_eq!(
            build_thumbnail_storage_key("f1", 150),
            "_thumbnails/f1_150.webp"
        );
    }

    #[test]
    fn mount_blob_key_roundtrip() {
        let key = build_mount_blob_storage_key("mp1", "b1");
        assert_eq!(key, "mount-blob:mp1:b1");
        assert!(is_mount_blob_storage_key(Some(&key)));
        assert!(!is_mount_blob_storage_key(Some("p1/a.png")));
        assert!(!is_mount_blob_storage_key(None));
        assert_eq!(
            parse_mount_blob_storage_key(&key),
            Some(("mp1".to_string(), "b1".to_string()))
        );
        // Malformed: no separator / empty sides.
        assert_eq!(parse_mount_blob_storage_key("mount-blob:justone"), None);
        assert_eq!(parse_mount_blob_storage_key("mount-blob::b"), None);
        assert_eq!(parse_mount_blob_storage_key("mount-blob:mp:"), None);
    }

    #[test]
    fn webp_conversion_policy() {
        let codec = PassthroughPixelCodec {
            width: 10,
            height: 20,
        };
        // Convertible → converted (mime + filename rewritten even when the
        // pixel seam is a passthrough — the policy is core-side, like v4's JS).
        let r = convert_to_webp(&codec, b"png-bytes", "image/png", "pic.png");
        assert!(r.was_converted);
        assert_eq!(r.mime_type, "image/webp");
        assert_eq!(r.filename, "pic.webp");
        assert_eq!(r.width, Some(10));
        // Already WebP → passthrough, still measured.
        let r = convert_to_webp(&codec, b"webp", "image/webp", "pic.webp");
        assert!(!r.was_converted);
        assert_eq!(r.width, Some(10));
        // SVG → passthrough, unmeasured.
        let r = convert_to_webp(&codec, b"<svg/>", "image/svg+xml", "pic.svg");
        assert!(!r.was_converted);
        assert_eq!(r.width, None);
        // Encode failure → warn-and-keep-original (measured).
        let r = convert_to_webp(&NotConfiguredPixelCodec, b"png", "image/png", "pic.png");
        assert!(!r.was_converted);
        assert_eq!(r.mime_type, "image/png");
        assert_eq!(r.filename, "pic.png");
    }

    #[test]
    fn blob_transcode_policy() {
        let codec = PassthroughPixelCodec {
            width: 1,
            height: 1,
        };
        // Transcodable (case-insensitive) → webp.
        let r = transcode_to_webp(&codec, b"x", "image/PNG", TRANSCODE_WEBP_QUALITY);
        assert_eq!(r.stored_mime_type, "image/webp");
        // webp → as-is.
        let r = transcode_to_webp(&codec, b"x", "image/webp", TRANSCODE_WEBP_QUALITY);
        assert_eq!(r.stored_mime_type, "image/webp");
        assert_eq!(r.sha256, sha256_hex(b"x"));
        // Non-image → as-is.
        let r = transcode_to_webp(&codec, b"x", "application/pdf", TRANSCODE_WEBP_QUALITY);
        assert_eq!(r.stored_mime_type, "application/pdf");
        // Failure → original bytes + original mime.
        let r = transcode_to_webp(
            &NotConfiguredPixelCodec,
            b"x",
            "image/png",
            TRANSCODE_WEBP_QUALITY,
        );
        assert_eq!(r.stored_mime_type, "image/png");
    }

    #[test]
    fn blob_relative_path_normalisation() {
        assert_eq!(
            normalise_blob_relative_path("images/a.png", "image/webp"),
            "images/a.webp"
        );
        assert_eq!(
            normalise_blob_relative_path("images/a.png", "image/png"),
            "images/a.png"
        );
        assert_eq!(
            normalise_blob_relative_path("images/noext", "image/webp"),
            "images/noext.webp"
        );
        assert_eq!(
            normalise_blob_relative_path("a.WEBP", "image/webp"),
            "a.WEBP"
        );
    }

    #[test]
    fn replace_extension_shapes() {
        assert_eq!(replace_extension_with_webp("a.png"), "a.webp");
        assert_eq!(replace_extension_with_webp("noext"), "noext.webp");
        assert_eq!(replace_extension_with_webp(".env"), ".env.webp");
    }

    #[test]
    fn encode_uri_component_matches_js() {
        assert_eq!(encode_uri_component("a b/c"), "a%20b%2Fc");
        assert_eq!(encode_uri_component("a-_.!~*'()"), "a-_.!~*'()");
        assert_eq!(encode_uri_component("é"), "%C3%A9");
        assert_eq!(encode_uri_component("mount-blob:m:b"), "mount-blob%3Am%3Ab");
    }

    #[test]
    fn folder_dir_normalisation() {
        assert_eq!(normalise_folder_dir(Some("/a/b/")), "a/b");
        assert_eq!(normalise_folder_dir(Some("a\\b")), "a/b");
        assert_eq!(normalise_folder_dir(Some("a/<bad>/c")), "a/_bad_/c");
        assert_eq!(normalise_folder_dir(Some("///")), "");
        assert_eq!(normalise_folder_dir(None), "");
    }

    #[test]
    fn merge_tags_insertion_order() {
        let merged = merge_tags(
            &["a".into(), "b".into()],
            &["b".into(), "c".into(), "a".into()],
        );
        assert_eq!(merged, vec!["a", "b", "c"]);
    }

    // ------------------------------------------------------------------
    // The production dispatch (ProductionFileBytes / manager ops) over a
    // real single-writer Db with the mount partition.
    // ------------------------------------------------------------------

    /// An in-memory recording backend for the disk branch.
    struct MemBackend {
        files: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
    }
    impl MemBackend {
        fn new() -> Self {
            MemBackend {
                files: std::sync::Mutex::new(std::collections::HashMap::new()),
            }
        }
    }
    impl StorageBackend for MemBackend {
        fn upload(&self, key: &str, content: &[u8], _ct: &str) -> Result<(), String> {
            self.files
                .lock()
                .unwrap()
                .insert(key.to_string(), content.to_vec());
            Ok(())
        }
        fn download(&self, key: &str) -> Result<Vec<u8>, String> {
            self.files
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .ok_or_else(|| format!("ENOENT: {key}"))
        }
        fn delete(&self, key: &str) -> Result<(), String> {
            self.files.lock().unwrap().remove(key);
            Ok(())
        }
        fn exists(&self, key: &str) -> Result<bool, String> {
            Ok(self.files.lock().unwrap().contains_key(key))
        }
    }

    fn test_db() -> (Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let pepper = "cXVpbGx0YXAtdGVzdC1wZXBwZXItMzItYnl0ZXMhIQ==";
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: dir.path().join("main.db"),
                mount_index: Some(dir.path().join("mount.db")),
                llm_logs: None,
            },
            pepper,
        )
        .unwrap();
        // Minimal tables for the dispatch paths.
        db.write_blocking(|ws| {
            ws.main().connection().execute_batch(
                "CREATE TABLE files (id TEXT PRIMARY KEY, sha256 TEXT, originalFilename TEXT, \
                 mimeType TEXT, size REAL, width REAL, height REAL, category TEXT, \
                 generationPrompt TEXT, generationModel TEXT, generationRevisedPrompt TEXT, \
                 description TEXT, storageKey TEXT);",
            )?;
            let mount = ws.mount_index().unwrap().connection();
            mount.execute_batch(
                "CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY);",
            )?;
            mount.execute_batch(crate::db::doc_mount_blobs::CREATE_TABLE_SQL)?;
            mount.execute_batch(
                "INSERT INTO doc_mount_files (id) VALUES ('f1'); \
                 INSERT INTO doc_mount_blobs (id, fileId, sha256, sizeBytes, storedMimeType, data, createdAt, updatedAt) \
                 VALUES ('blob1', 'f1', 'aa', 3, 'image/webp', X'010203', '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z');",
            )?;
            Ok(())
        })
        .unwrap();
        (db, dir)
    }

    fn entry_with_key(key: Option<&str>) -> FileEntry {
        FileEntry {
            id: "e1".into(),
            sha256: "aa".into(),
            original_filename: "pic.webp".into(),
            mime_type: "image/webp".into(),
            size: 3,
            width: None,
            height: None,
            category: "IMAGE".into(),
            generation_prompt: None,
            generation_model: None,
            generation_revised_prompt: None,
            description: None,
            storage_key: key.map(str::to_string),
        }
    }

    #[test]
    fn download_dispatches_on_storage_key() {
        let (db, _dir) = test_db();
        let backend = MemBackend::new();
        backend
            .upload("p1/pic.webp", b"disk!", "image/webp")
            .unwrap();

        // mount-blob key → the doc_mount_blobs read.
        let bytes =
            download_file(&db, &backend, &entry_with_key(Some("mount-blob:mp1:blob1"))).unwrap();
        assert_eq!(bytes, vec![1, 2, 3]);
        // missing blob → v4's "Mount-blob not found" wrapped error.
        let err =
            download_file(&db, &backend, &entry_with_key(Some("mount-blob:mp1:nope"))).unwrap_err();
        assert!(err.contains("Mount-blob not found"), "{err}");
        // disk key → the backend.
        let bytes = download_file(&db, &backend, &entry_with_key(Some("p1/pic.webp"))).unwrap();
        assert_eq!(bytes, b"disk!".to_vec());
        // no key (and JS-falsy empty key) → the no-storage-key error.
        assert!(download_file(&db, &backend, &entry_with_key(None))
            .unwrap_err()
            .contains("no storage key"));
        assert!(download_file(&db, &backend, &entry_with_key(Some("")))
            .unwrap_err()
            .contains("no storage key"));
        // fileExists mirrors the dispatch.
        assert!(file_exists(&db, &backend, &entry_with_key(Some("mount-blob:mp1:blob1"))).unwrap());
        assert!(!file_exists(&db, &backend, &entry_with_key(Some("mount-blob:mp1:nope"))).unwrap());
        assert!(!file_exists(&db, &backend, &entry_with_key(None)).unwrap());
    }

    #[test]
    fn production_ingest_refuses_the_writer_thread() {
        let (db, _dir) = test_db();
        let store = ProductionFileBytes {
            db: db.clone(),
            backend: Arc::new(MemBackend::new()),
            codec: Arc::new(NotConfiguredPixelCodec),
        };
        let req = crate::photos::save_image_to_album::IngestImageRequest {
            buffer: vec![1],
            original_filename: "x.png".into(),
            mime_type: "image/png".into(),
            user_id: "u1".into(),
        };
        // From a thread named like the writer, the guard fails LOUD (never a
        // deadlock) — the documented keep_image mount-blob-fallback handoff.
        let store = std::sync::Arc::new(store);
        let s2 = std::sync::Arc::clone(&store);
        let handle = std::thread::Builder::new()
            .name("quilltap-writer".to_string())
            .spawn(move || {
                use crate::photos::save_image_to_album::FileBytesStore as _;
                s2.ingest_image_buffer(&req).unwrap_err()
            })
            .unwrap();
        let err = handle.join().unwrap();
        assert!(err.contains("writer thread"), "{err}");
    }

    #[tokio::test]
    async fn delete_file_completely_chokepoint() {
        // test_db seeds via write_blocking — run it off the runtime worker.
        let (db, _dir) = tokio::task::spawn_blocking(test_db).await.unwrap();
        let backend = MemBackend::new();
        backend
            .upload("p1/gone.bin", b"x", "application/octet-stream")
            .unwrap();
        // `db.write(...)` (async) — write_blocking would block the tokio worker.
        db.write(|ws| {
            ws.main().connection().execute_batch(
                "INSERT INTO files (id, sha256, originalFilename, mimeType, size, category, storageKey) \
                 VALUES ('e-disk', 'bb', 'gone.bin', 'application/octet-stream', 1, 'IMAGE', 'p1/gone.bin'); \
                 INSERT INTO files (id, sha256, originalFilename, mimeType, size, category, storageKey) \
                 VALUES ('e-nokey', 'cc', 'meta-only.bin', 'application/octet-stream', 1, 'IMAGE', NULL);",
            )?;
            Ok(())
        })
        .await
        .unwrap();

        // Disk-backed: bytes deleted + row deleted.
        assert!(delete_file_completely(&db, &backend, "e-disk")
            .await
            .unwrap());
        assert!(!backend.exists("p1/gone.bin").unwrap());
        // NULL storage key: v4 warns + skips the bytes, deletes the row.
        assert!(delete_file_completely(&db, &backend, "e-nokey")
            .await
            .unwrap());
        // Missing row → false.
        assert!(!delete_file_completely(&db, &backend, "e-disk")
            .await
            .unwrap());
        let remaining: i64 = db
            .read_main(|c| {
                c.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
                    .map_err(crate::db::DbError::from)
            })
            .unwrap();
        assert_eq!(remaining, 0);
    }
}
