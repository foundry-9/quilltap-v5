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
use quilltap_core::api::{
    ErrorKind, QuilltapCore, Request as CoreRequest, Response as CoreResponse,
};
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
use quilltap_core::services::mount_index::path_utils::mime_for_extension;
use quilltap_host::{HostImageCodec, LocalStorageBackend};
use serde_json::json;

use crate::multipart::FormData;
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

/// v4 `buildContentDisposition` with its `inline` default — the two local
/// copies moved into `quilltap_core::content_disposition` when v4 `b3ee00f1`
/// deduplicated its own pair into `lib/api/content-disposition.ts`. The lift
/// also fixed a divergence this copy carried: the ASCII fallback replaced each
/// non-ASCII `char`, where JS replaces each UTF-16 code UNIT, so an astral
/// character (an emoji in a filename) got one underscore instead of two.
fn build_content_disposition(filename: &str) -> String {
    quilltap_core::content_disposition::build_content_disposition(
        filename,
        quilltap_core::content_disposition::Disposition::Inline,
    )
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

// `mime_for_extension` is shared from
// `quilltap_core::services::mount_index::path_utils` (P4.6v moved the v4
// `mimeForExtension` port into core so the web edge and the mount-index service
// layer share one `path.extname`-faithful implementation) — imported at the top.

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

    // v4 readMountFileBytes, the filesystem-mount branch (P4.6y): resolve the
    // path under the mount's basePath (the boundary-escape guard) and stream
    // the on-disk bytes.
    let mp_info = db.read_mount_index({
        let id = id.clone();
        move |conn| {
            quilltap_core::db::doc_mount_points::DocMountPointsRepository::new(conn)
                .find_service_info_by_id(&id)
        }
    });
    let mp_info = match mp_info {
        Ok(Some(mp)) => mp,
        Ok(None) => return not_found("Mount point"),
        Err(_) => return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file"),
    };
    if mp_info.is_filesystem_mount() {
        let abs = match quilltap_core::services::mount_index::file_ops::resolve_fs_absolute(
            mp_info.base_path.as_deref(),
            &mp_info.id,
            &rel,
        ) {
            Ok(a) => a,
            Err(e) => return error_json(StatusCode::BAD_REQUEST, &e.to_string()),
        };
        return match std::fs::read(&abs) {
            Ok(bytes) => {
                let sha = {
                    use sha2::{Digest, Sha256};
                    hex::encode(Sha256::digest(&bytes))
                };
                (
                    StatusCode::OK,
                    [
                        ("content-type", mime_for_extension(&rel).to_string()),
                        ("content-length", bytes.len().to_string()),
                        ("cache-control", "private, max-age=3600".to_string()),
                        ("x-file-sha256", sha),
                    ],
                    bytes,
                )
                    .into_response()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => not_found("File"),
            Err(_) => error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file"),
        };
    }

    // v4 readMountFileBytes, the database-mount branch: the link row picks
    // documents (text) vs blobs.
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

// ---------------------------------------------------------------------------
// The P4.6y multipart legs — the only per-variant web routes (JSON variants
// auto-wire via POST /api/dispatch):
//   PUT  /api/v1/mount-points/{id}/files/{*path}   (JSON or multipart ingest)
//   POST /api/v1/mount-points/{id}?action=write-file  (multipart, byte-preserving)
//   POST /api/v1/mount-points/{id}/blobs            (multipart upload, 201)
// ---------------------------------------------------------------------------

/// Map a core dispatch `Response` to the v4 route shape: the payload verbatim
/// on success (`success_status`), `{error, code?}` on the typed failures.
fn core_response_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        // v4's route handlers return these payloads FLAT (`{file}`, `{duplicate,…}`,
        // `{data: FileEntry}`, `{success: true}`) — the dispatch envelope tag must
        // not leak to the REST edge (the LOCKED SPA client reads the raw body).
        CoreResponse::MountFile(v) | CoreResponse::Files(v) | CoreResponse::ChatMedia(v) => (
            success_status,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        CoreResponse::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
                ErrorKind::NotFound => StatusCode::NOT_FOUND,
                ErrorKind::Conflict => StatusCode::CONFLICT,
                ErrorKind::Forbidden => StatusCode::FORBIDDEN,
                ErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
                ErrorKind::Unprocessable => StatusCode::UNPROCESSABLE_ENTITY,
                ErrorKind::Locked => StatusCode::SERVICE_UNAVAILABLE,
                ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let mut body = serde_json::Map::new();
            body.insert("error".to_string(), json!(e.message));
            if let Some(code) = e.code {
                body.insert("code".to_string(), json!(code));
            }
            // v4's FILE_HAS_ASSOCIATIONS itemized payload (P4.6ah) rides the flat
            // error body when present.
            if let Some(assoc) = e.associations {
                body.insert(
                    "associations".to_string(),
                    serde_json::to_value(assoc).unwrap_or(serde_json::Value::Null),
                );
            }
            (
                status,
                [("content-type", "application/json")],
                serde_json::Value::Object(body).to_string(),
            )
                .into_response()
        }
        other => {
            let body = serde_json::to_value(&other).unwrap_or(serde_json::Value::Null);
            (
                success_status,
                [("content-type", "application/json")],
                body.to_string(),
            )
                .into_response()
        }
    }
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

async fn dispatch_core(
    state: &SharedState,
    req: CoreRequest,
) -> Result<CoreResponse, AxumResponse> {
    let Some(host) = state.host() else {
        return Err(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "The engine is not running",
        ));
    };
    Ok(host.core().dispatch(req).await)
}

/// v4 item-route PUT — the `storeMountFile` ingest (JSON body or multipart
/// `file` + `expected_mtime` + `force`), mapped onto `MountFileWrite`.
pub async fn mount_file_put(
    State(state): State<SharedState>,
    Path((id, path)): Path<(String, String)>,
    req: axum::extract::Request,
) -> AxumResponse {
    let content_type = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let core_req = if content_type.contains("multipart/form-data") {
        let form = match FormData::from_request(req, &state).await {
            Ok(f) => f,
            Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid multipart body"),
        };
        let Some(file) = form.file("file") else {
            return error_json(
                StatusCode::BAD_REQUEST,
                "Missing \"file\" field in multipart body",
            );
        };
        let expected_mtime = match form.text("expected_mtime") {
            Some(raw) if !raw.is_empty() => match raw.parse::<i64>() {
                Ok(v) if v >= 0 => Some(v),
                _ => {
                    return error_json(
                        StatusCode::BAD_REQUEST,
                        "expected_mtime must be a non-negative integer",
                    )
                }
            },
            _ => None,
        };
        let force = form
            .text("force")
            .map(|f| f.to_lowercase() == "true")
            .unwrap_or(false);
        CoreRequest::MountFileWrite {
            mount_point_id: id,
            path,
            content: base64_of(&file.bytes),
            encoding: Some("base64".to_string()),
            expected_mtime,
            force: Some(force),
            original_mime_type: file.content_type.clone().filter(|t| !t.is_empty()),
            original_file_name: file.filename.clone().filter(|n| !n.is_empty()),
        }
    } else {
        let bytes = match axum::body::to_bytes(req.into_body(), 64 * 1024 * 1024).await {
            Ok(b) => b,
            Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid request body"),
        };
        let body: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => return error_json(StatusCode::BAD_REQUEST, &format!("Invalid body: {e}")),
        };
        let Some(content) = body.get("content").and_then(|c| c.as_str()) else {
            return error_json(StatusCode::BAD_REQUEST, "Invalid body: content is required");
        };
        CoreRequest::MountFileWrite {
            mount_point_id: id,
            path,
            content: content.to_string(),
            encoding: body
                .get("encoding")
                .and_then(|e| e.as_str())
                .map(|s| s.to_string()),
            expected_mtime: body.get("expected_mtime").and_then(|m| m.as_i64()),
            force: body.get("force").and_then(|f| f.as_bool()),
            original_mime_type: None,
            original_file_name: None,
        }
    };
    match dispatch_core(&state, core_req).await {
        Ok(resp) => core_response_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

/// v4 `POST /api/v1/mount-points/{id}?action=…` — ONLY the multipart
/// `write-file` leg lives at this edge (the JSON verbs ride `/api/dispatch`);
/// other actions get a loud pointer, not a silent 404.
pub async fn mount_point_action_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    req: axum::extract::Request,
) -> AxumResponse {
    let action = query.get("action").map(String::as_str).unwrap_or("");
    if action != "write-file" {
        return error_json(
            StatusCode::BAD_REQUEST,
            "Only the multipart 'write-file' action is served on this route; JSON mount actions ride POST /api/dispatch",
        );
    }
    let content_type = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("multipart/form-data") {
        return error_json(StatusCode::BAD_REQUEST, "Expected multipart/form-data");
    }
    let form = match FormData::from_request(req, &state).await {
        Ok(f) => f,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid multipart body"),
    };
    let Some(file) = form.file("file") else {
        return error_json(
            StatusCode::BAD_REQUEST,
            "Missing \"file\" field in multipart body",
        );
    };
    let path = form.text("path").unwrap_or_default().trim().to_string();
    if path.is_empty() {
        return error_json(StatusCode::BAD_REQUEST, "Missing \"path\" field");
    }
    let force = form
        .text("force")
        .map(|f| f.to_lowercase() == "true")
        .unwrap_or(false);
    let core_req = CoreRequest::MountFileWriteRaw {
        mount_point_id: id,
        path,
        data: base64_of(&file.bytes),
        force: Some(force),
    };
    match dispatch_core(&state, core_req).await {
        Ok(resp) => core_response_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

/// v4 `POST /api/v1/mount-points/{id}/blobs` — the multipart blob upload
/// (201; the single ingest pipeline with `assetStorage: 'database'`).
pub async fn mount_blobs_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    req: axum::extract::Request,
) -> AxumResponse {
    let content_type = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("multipart/form-data") {
        return error_json(StatusCode::BAD_REQUEST, "Expected multipart/form-data");
    }
    let form = match FormData::from_request(req, &state).await {
        Ok(f) => f,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid multipart body"),
    };
    let Some(file) = form.file("file") else {
        return error_json(
            StatusCode::BAD_REQUEST,
            "Missing \"file\" field in multipart body",
        );
    };
    let path = form.text("path").unwrap_or_default().trim().to_string();
    if path.is_empty() {
        return error_json(StatusCode::BAD_REQUEST, "Missing \"path\" field");
    }
    let description = form.text("description").unwrap_or_default();
    let core_req = CoreRequest::MountBlobUpload {
        mount_point_id: id,
        path,
        description: Some(description),
        data: base64_of(&file.bytes),
        original_mime_type: file.content_type.clone().filter(|t| !t.is_empty()),
        original_file_name: file.filename.clone().filter(|n| !n.is_empty()),
    };
    match dispatch_core(&state, core_req).await {
        Ok(resp) => core_response_to_http(resp, StatusCode::CREATED),
        Err(r) => r,
    }
}

// ---------------------------------------------------------------------------
// P4.6ah: the files write + maintenance REST legs (JSON verbs ride /api/dispatch)
// ---------------------------------------------------------------------------

/// v4 `POST /api/v1/files?action=upload` — the general upload multipart leg
/// (`file`, `tags?` JSON string of `[{tagType,tagId}]`, `projectId?`,
/// `folderPath?`). Maps onto [`CoreRequest::FileUpload`] → `{data: FileEntry}`.
/// Status 201 (create) / 200 (overwrite) is derived from `createdAt == updatedAt`
/// (a create mints both equal; an overwrite bumps only `updatedAt`).
pub async fn files_upload_post(
    State(state): State<SharedState>,
    req: axum::extract::Request,
) -> AxumResponse {
    let content_type = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("multipart/form-data") {
        return error_json(
            StatusCode::BAD_REQUEST,
            "Expected multipart/form-data content type",
        );
    }
    let form = match FormData::from_request(req, &state).await {
        Ok(f) => f,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid multipart body"),
    };
    let Some(file) = form.file("file") else {
        return error_json(StatusCode::BAD_REQUEST, "No file provided");
    };
    // tags: JSON string of `[{tagType, tagId}]` → the tagIds (v4 upload.ts:55).
    let tags: Option<Vec<String>> = match form.text("tags") {
        Some(raw) if !raw.is_empty() => {
            match serde_json::from_str::<Vec<serde_json::Value>>(&raw) {
                Ok(arr) => Some(
                    arr.iter()
                        .filter_map(|t| t.get("tagId").and_then(|v| v.as_str()).map(str::to_string))
                        .collect(),
                ),
                Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid tags JSON"),
            }
        }
        _ => None,
    };
    let project_id = form.text("projectId").filter(|s| !s.is_empty());
    let folder_path = form.text("folderPath").filter(|s| !s.is_empty());

    let core_req = CoreRequest::FileUpload {
        filename: file.filename.clone().unwrap_or_default(),
        content_type: file.content_type.clone().unwrap_or_default(),
        data: base64_of(&file.bytes),
        tags,
        project_id,
        folder_path,
    };
    let resp = match dispatch_core(&state, core_req).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    // Derive 201 (create) vs 200 (overwrite) from the returned entity timestamps.
    let status = if let CoreResponse::Files(v) = &resp {
        let data = v.get("data");
        let created = data
            .and_then(|d| d.get("createdAt"))
            .and_then(|c| c.as_str());
        let updated = data
            .and_then(|d| d.get("updatedAt"))
            .and_then(|c| c.as_str());
        if created.is_some() && created == updated {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        }
    } else {
        StatusCode::CREATED
    };
    core_response_to_http(resp, status)
}

/// v4 `POST /api/v1/chats/{id}/files` — the chat-file leg. `?action=link` → a JSON
/// `{fileId}` body ([`CoreRequest::ChatFileLink`]); otherwise the multipart upload
/// (`file`, `resolution?`, `conflictingFileId?` → [`CoreRequest::ChatFileUpload`]).
/// Always 200 (v4's `NextResponse.json` default, incl. the duplicate envelope).
pub async fn chat_files_post(
    State(state): State<SharedState>,
    Path(chat_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    req: axum::extract::Request,
) -> AxumResponse {
    // P4.9E4A: v4's `?action=attach-mount-file` leg (files/route.ts:250). The
    // two fields are read as raw JSON and coerced to `""` when absent or
    // non-string, because v4's validation is hand-rolled (`!v || typeof v !==
    // 'string'`), not Zod — so both arms answer the same `badRequest`, produced
    // by the core handler AFTER its chat-404 (v4 resolves the chat first).
    if params.get("action").map(String::as_str) == Some("attach-mount-file") {
        let bytes = match axum::body::to_bytes(req.into_body(), 1024 * 1024).await {
            Ok(b) => b,
            Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid request body"),
        };
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        let str_field = |k: &str| {
            body.get(k)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let core_req = CoreRequest::ChatAttachMountFile {
            chat_id,
            mount_point_id: str_field("mountPointId"),
            relative_path: str_field("relativePath"),
        };
        return match dispatch_core(&state, core_req).await {
            Ok(resp) => core_response_to_http(resp, StatusCode::OK),
            Err(r) => r,
        };
    }
    if params.get("action").map(String::as_str) == Some("link") {
        let bytes = match axum::body::to_bytes(req.into_body(), 1024 * 1024).await {
            Ok(b) => b,
            Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid request body"),
        };
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        let Some(file_id) = body.get("fileId").and_then(|v| v.as_str()) else {
            return error_json(StatusCode::BAD_REQUEST, "fileId is required");
        };
        let core_req = CoreRequest::ChatFileLink {
            chat_id,
            file_id: file_id.to_string(),
        };
        return match dispatch_core(&state, core_req).await {
            Ok(resp) => core_response_to_http(resp, StatusCode::OK),
            Err(r) => r,
        };
    }

    let content_type = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("multipart/form-data") {
        return error_json(StatusCode::BAD_REQUEST, "Expected multipart/form-data");
    }
    let form = match FormData::from_request(req, &state).await {
        Ok(f) => f,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid multipart body"),
    };
    let Some(file) = form.file("file") else {
        return error_json(StatusCode::BAD_REQUEST, "No file provided");
    };
    let core_req = CoreRequest::ChatFileUpload {
        chat_id,
        filename: file.filename.clone().unwrap_or_default(),
        content_type: file.content_type.clone().unwrap_or_default(),
        data: base64_of(&file.bytes),
        resolution: form.text("resolution").filter(|s| !s.is_empty()),
        conflicting_file_id: form.text("conflictingFileId").filter(|s| !s.is_empty()),
    };
    match dispatch_core(&state, core_req).await {
        Ok(resp) => core_response_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

/// v4 `DELETE /api/v1/files/{id}?force=&dissociate=` — maps onto
/// [`CoreRequest::FileDelete`]. The `FILE_HAS_ASSOCIATIONS` 400 carries its
/// itemized `associations` payload through [`core_response_to_http`].
pub async fn files_delete(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> AxumResponse {
    let core_req = CoreRequest::FileDelete {
        file_id: id,
        force: params.get("force").map(String::as_str) == Some("true"),
        dissociate: params.get("dissociate").map(String::as_str) == Some("true"),
    };
    match dispatch_core(&state, core_req).await {
        Ok(resp) => core_response_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}
