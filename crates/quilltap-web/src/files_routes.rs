//! The binary resource GETs (D4 subset, this round):
//!
//! - `GET /api/v1/files/proxy/{*key}` — bytes by storage key (v4
//!   `files/proxy/[...key]/route.ts`).
//! - `GET /api/v1/files/{id}` (+ `?action=thumbnail&size=`) — bytes / cached
//!   WebP thumbnail by file id (v4 `files/[id]` GET handlers).
//! - `GET /api/v1/mount-points/{id}/files/{*path}` — the RAW byte form
//!   (`?raw=1` or `Accept: application/octet-stream`); the JSON read envelope
//!   is P4.4 surface (with the rest of the doc REST tree).
//! - `GET /api/v1/mount-points/{id}/blobs/{*path}` — blob bytes with the
//!   documents-table fallback (v4 `blobs/[...path]` GET).
//!
//! Headers per the v4 routes: `Cache-Control: public, max-age=31536000,
//! immutable` for immutable file bytes, `private, max-age=3600` for mount
//! paths, RFC 5987 `Content-Disposition: inline`, `X-Frame-Options:
//! SAMEORIGIN`, CSP `frame-ancestors 'self'`, and the
//! `X-File-Sha256`/`X-Blob-Sha256` hashes. No Range support (v4 has none).
//! Themes assets/fonts + `characters/{id}/photos` are P4.4 deferrals.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::db::doc_mount_blobs::DocMountBlobsRepository;
use quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository;
use quilltap_core::db::doc_mount_file_links::{
    normalise_relative_path, DocMountFileLinksRepository,
};
use quilltap_core::db::files::FilesRepository;
use quilltap_core::db::runtime::Db;
use quilltap_core::files::image_processing::can_resize_image;
use quilltap_core::services::file_storage::{
    build_thumbnail_storage_key, download_file, storage_key_exists, upload_raw,
};
use quilltap_host::{HostImageCodec, LocalStorageBackend};
use serde_json::json;

use crate::state::SharedState;

/// v4 `DEFAULT_THUMBNAIL_SIZE` / `MAX_THUMBNAIL_SIZE` (`thumbnail-utils.ts`).
const DEFAULT_THUMBNAIL_SIZE: i64 = 150;
const MAX_THUMBNAIL_SIZE: i64 = 300;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

pub(crate) fn error_json(status: StatusCode, message: &str) -> AxumResponse {
    (
        status,
        [("content-type", "application/json")],
        json!({ "error": message }).to_string(),
    )
        .into_response()
}

pub(crate) fn not_found(resource: &str) -> AxumResponse {
    error_json(StatusCode::NOT_FOUND, &format!("{resource} not found"))
}

/// The ready `Db` + disk backend, or the locked/failed refusal.
pub(crate) fn db_and_backend(
    state: &SharedState,
) -> Result<(Db, LocalStorageBackend), Box<AxumResponse>> {
    let Some(host) = state.host() else {
        return Err(Box::new(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "server failed to start",
        )));
    };
    let Some(db) = host.core().db() else {
        return Err(Box::new(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "The database is locked. Unlock it to continue.",
        )));
    };
    let backend = LocalStorageBackend::new(state.base_dir.join("files"));
    Ok((db, backend))
}

/// v4 `buildContentDisposition` — RFC 5987 for non-ASCII filenames.
fn build_content_disposition(filename: &str) -> String {
    let has_non_ascii = filename.bytes().any(|b| !b.is_ascii());
    if !has_non_ascii {
        return format!("inline; filename=\"{filename}\"");
    }
    let ascii: String = filename
        .chars()
        .map(|c| if c.is_ascii() { c } else { '_' })
        .collect();
    format!(
        "inline; filename=\"{ascii}\"; filename*=UTF-8''{}",
        encode_uri_component(filename)
    )
}

/// JS `encodeURIComponent` over the unreserved set it keeps
/// (`A–Z a–z 0–9 - _ . ! ~ * ' ( )`).
fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(*b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The immutable file-bytes response (proxy + download routes).
fn file_bytes_response(mime: &str, filename: &str, bytes: Vec<u8>) -> AxumResponse {
    (
        StatusCode::OK,
        [
            ("content-type", mime.to_string()),
            ("content-length", bytes.len().to_string()),
            ("content-disposition", build_content_disposition(filename)),
            (
                "cache-control",
                "public, max-age=31536000, immutable".to_string(),
            ),
            ("x-frame-options", "SAMEORIGIN".to_string()),
            (
                "content-security-policy",
                "frame-ancestors 'self'".to_string(),
            ),
        ],
        bytes,
    )
        .into_response()
}

/// v4 `mimeForExtension` (`lib/mount-index/path-utils.ts`).
fn mime_for_extension(relative_path: &str) -> &'static str {
    let ext = relative_path
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.').map(|(_, e)| e))
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "jsonl" | "ndjson" => "application/jsonl; charset=utf-8",
        "webp" => "image/webp",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
    }
}

/// v4 `mimeForDocument` (the blobs route's documents fallback).
fn mime_for_document(file_type: &str) -> &'static str {
    match file_type {
        "markdown" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "jsonl" => "application/jsonl; charset=utf-8",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/files/proxy/{*key}
// ---------------------------------------------------------------------------

pub async fn files_proxy(
    State(state): State<SharedState>,
    Path(key): Path<String>,
) -> AxumResponse {
    let (db, backend) = match db_and_backend(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    let storage_key = key;
    let k = storage_key.clone();
    let entry = match db.read_main(move |conn| FilesRepository::new(conn).find_by_storage_key(&k)) {
        Ok(Some(e)) => e,
        Ok(None) => return not_found("File"),
        Err(_) => return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to serve file"),
    };
    match download_file(&db, &backend, &entry) {
        Ok(bytes) => file_bytes_response(&entry.mime_type, &entry.original_filename, bytes),
        Err(_) => error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to download file"),
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/files/{id} (+ ?action=thumbnail&size=)
// ---------------------------------------------------------------------------

pub async fn files_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> AxumResponse {
    let (db, backend) = match db_and_backend(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    let fid = id.clone();
    let entry = match db.read_main(move |conn| FilesRepository::new(conn).find_by_id(&fid)) {
        Ok(Some(e)) => e,
        Ok(None) => return not_found("File"),
        Err(_) => return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to serve file"),
    };

    if query.get("action").map(String::as_str) == Some("thumbnail") {
        // v4 handleGetThumbnail: parse + clamp size, gate on resizable image.
        let mut size = DEFAULT_THUMBNAIL_SIZE;
        if let Some(raw) = query.get("size") {
            match raw.parse::<i64>() {
                Ok(v) if v >= 1 => size = v.min(MAX_THUMBNAIL_SIZE),
                _ => return error_json(StatusCode::BAD_REQUEST, "Invalid size parameter"),
            }
        }
        if !(entry.mime_type.starts_with("image/") && can_resize_image(&entry.mime_type)) {
            return error_json(StatusCode::BAD_REQUEST, "File is not a resizable image");
        }
        // Cache read at the canonical key (v4 generateThumbnail).
        let thumb_key = build_thumbnail_storage_key(&entry.id, size);
        let mut cache_entry = entry.clone();
        cache_entry.storage_key = Some(thumb_key.clone());
        let cached = storage_key_exists(&db, &backend, &thumb_key).unwrap_or(false);
        let bytes = if cached {
            match download_file(&db, &backend, &cache_entry) {
                Ok(b) => b,
                Err(_) => {
                    return error_json(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to generate thumbnail",
                    )
                }
            }
        } else {
            let original = match download_file(&db, &backend, &entry) {
                Ok(b) => b,
                Err(_) => {
                    return error_json(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to generate thumbnail",
                    )
                }
            };
            let codec = HostImageCodec;
            match codec.thumbnail_webp(&original, size) {
                Ok(thumb) => {
                    // Best-effort cache write (v4 caches at the canonical key).
                    let _ = upload_raw(&backend, &thumb_key, &thumb, "image/webp");
                    thumb
                }
                Err(_) => {
                    return error_json(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to generate thumbnail",
                    )
                }
            }
        };
        return (
            StatusCode::OK,
            [
                ("content-type", "image/webp".to_string()),
                ("content-length", bytes.len().to_string()),
                (
                    "cache-control",
                    "public, max-age=31536000, immutable".to_string(),
                ),
            ],
            bytes,
        )
            .into_response();
    }

    // Plain download (v4 handleDownloadFile).
    if entry.storage_key.as_deref().unwrap_or("").is_empty() {
        return error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "File not available - storage key missing",
        );
    }
    match download_file(&db, &backend, &entry) {
        Ok(bytes) => file_bytes_response(&entry.mime_type, &entry.original_filename, bytes),
        Err(_) => error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to serve file"),
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/mount-points/{id}/files/{*path} — raw form
// ---------------------------------------------------------------------------

pub async fn mount_file_get(
    State(state): State<SharedState>,
    Path((id, path)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> AxumResponse {
    let (db, _backend) = match db_and_backend(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    let wants_raw = query.get("raw").map(String::as_str) == Some("1")
        || query.get("raw").map(String::as_str) == Some("true")
        || headers
            .get("accept")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.contains("application/octet-stream"))
            .unwrap_or(false);
    if !wants_raw {
        // The JSON read envelope (v4 readMountFile) is P4.4 surface, with the
        // rest of the doc REST tree.
        return error_json(
            StatusCode::BAD_REQUEST,
            "Only the raw byte form is available (send ?raw=1 or Accept: application/octet-stream)",
        );
    }
    let rel = match normalise_relative_path(&path) {
        Ok(r) => r,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid path"),
    };

    // v4 readMountFileBytes, the database-mount branch (fs mounts are the
    // standing FsSeam): the link row picks documents (text) vs blobs.
    #[allow(clippy::type_complexity)]
    let read: Result<Option<(Vec<u8>, String, String)>, quilltap_core::db::DbError> = db
        .read_mount_index({
            let id = id.clone();
            let rel = rel.clone();
            move |conn| {
                let links = DocMountFileLinksRepository::new(conn);
                let Some(link) = links.find_by_mount_point_and_path(&id, &rel)? else {
                    return Ok(None);
                };
                let is_text = matches!(
                    link.file_type.as_str(),
                    "markdown" | "txt" | "json" | "jsonl"
                );
                if is_text {
                    let docs = DocMountDocumentsRepository::new(conn);
                    let Some(content) = docs.find_by_mount_point_and_path(&id, &rel)? else {
                        return Ok(None);
                    };
                    return Ok(Some((
                        content.into_bytes(),
                        mime_for_extension(&rel).to_string(),
                        link.sha256,
                    )));
                }
                let blobs = DocMountBlobsRepository::new(conn);
                let Some(bytes) = blobs.read_data_by_file_id(&link.file_id)? else {
                    return Ok(None);
                };
                let (mime, sha) = match blobs.find_by_mount_point_and_path(&id, &rel)? {
                    Some(meta) => (meta.stored_mime_type, meta.sha256),
                    None => (mime_for_extension(&rel).to_string(), link.sha256),
                };
                Ok(Some((bytes, mime, sha)))
            }
        });

    match read {
        Ok(Some((bytes, mime, sha))) => (
            StatusCode::OK,
            [
                ("content-type", mime),
                ("content-length", bytes.len().to_string()),
                ("cache-control", "private, max-age=3600".to_string()),
                ("x-file-sha256", sha),
            ],
            bytes,
        )
            .into_response(),
        Ok(None) => not_found("File"),
        Err(_) => error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file"),
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/mount-points/{id}/blobs/{*path}
// ---------------------------------------------------------------------------

pub async fn mount_blob_get(
    State(state): State<SharedState>,
    Path((id, path)): Path<(String, String)>,
) -> AxumResponse {
    let (db, _backend) = match db_and_backend(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };
    #[allow(clippy::type_complexity)]
    let read: Result<Option<(Vec<u8>, String, String, usize)>, quilltap_core::db::DbError> = db
        .read_mount_index({
            let id = id.clone();
            let path = path.clone();
            move |conn| {
                // The blob branch, error-tolerant on a store with no
                // doc_mount_blobs table yet (v4's hand-rolled repo creates it
                // lazily on first WRITE; a read-only route must not).
                let blobs = DocMountBlobsRepository::new(conn);
                if let Ok(Some(meta)) = blobs.find_by_mount_point_and_path(&id, &path) {
                    let Some(data) = blobs.read_data(&meta.id)? else {
                        return Ok(None);
                    };
                    let len = meta.size_bytes.max(0) as usize;
                    return Ok(Some((data, meta.stored_mime_type, meta.sha256, len)));
                }
                // The documents fallback (a text file addressed via /blobs —
                // v4's uploads-store back-compat).
                let links = DocMountFileLinksRepository::new(conn);
                let Some(link) = links.find_by_mount_point_and_path(&id, &path)? else {
                    return Ok(None);
                };
                let docs = DocMountDocumentsRepository::new(conn);
                let Some(content) = docs.find_by_mount_point_and_path(&id, &path)? else {
                    return Ok(None);
                };
                let bytes = content.into_bytes();
                let len = bytes.len();
                Ok(Some((
                    bytes,
                    mime_for_document(&link.file_type).to_string(),
                    link.sha256,
                    len,
                )))
            }
        });

    match read {
        Ok(Some((bytes, mime, sha, len))) => (
            StatusCode::OK,
            [
                ("content-type", mime),
                ("content-length", len.to_string()),
                ("cache-control", "private, max-age=3600".to_string()),
                ("x-blob-sha256", sha),
            ],
            bytes,
        )
            .into_response(),
        Ok(None) => not_found("Blob"),
        Err(_) => error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to serve blob"),
    }
}
