//! P4.9G5 REST edges — the byte-level backup legs.
//!
//! - `GET /api/v1/system/backup/{id}` — the SINGLE-USE archive download.
//! - `POST /api/v1/system/backup` — v4-URL parity for the create verb (which
//!   also rides `POST /api/dispatch` as `systemBackupCreate`); v4 answers 201.
//! - `POST /api/v1/system/restore?action=upload|preview|<none>` — v4's
//!   three-way action dispatch. The **upload** leg is web-edge-only (a raw
//!   octet-stream body streamed to a temp zip, which is what the SPA's XHR
//!   sends); the other two unwrap to the `systemRestorePreview` /
//!   `systemRestoreExecute` verbs, which is how the SPA actually reaches them.
//!
//! The download has no dispatch verb because it streams bytes rather than JSON
//! (the qtap-target byte route and the fs raw read are the precedents). It
//! reaches the host's single-use temp store directly through
//! [`quilltap_host::Host::backup_services`].
//!
//! v4's handler (`app/api/v1/system/backup/[id]/route.ts`) does four things this
//! port keeps: `retrieveTemporaryBackup` REMOVES the entry (one download per
//! backup), the response carries `Content-Length`, the filename is minted at
//! download time as `quilltap-backup-<ISO with [:.] → ->.zip`, and when the
//! stream closes the zip is unlinked and its temp directory removed.
//!
//! **One divergence, recorded:** v4 streams from disk and deletes on stream
//! close; v5 reads the archive into memory, deletes immediately, and serves the
//! bytes. The observable result is identical (same bytes, same headers, file
//! gone afterwards) and it removes a class of failure v4 has — a client that
//! disconnects mid-stream leaves v4's zip on disk until the 30-minute sweep. It
//! does hold one archive in memory for the duration of the response; the
//! restore side's upload leg (unported) is the place that decision gets
//! revisited if archives get large enough to matter.

use std::collections::HashMap;
use std::io::Write;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use serde_json::Value;
use tokio_stream::StreamExt;

use quilltap_core::api::Request as CoreRequest;
use quilltap_core::services::backup::BackupHost;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::system_data_routes::system_body_public;
use crate::text_replacements_routes::dispatch_core;

/// `POST /api/v1/system/backup` — v4 answers **201**.
///
/// v4 `route.ts:23-28` (`7189a968`): the body is `{compact?: boolean}` and
/// OPTIONAL; a malformed body is treated as ABSENT rather than rejected
/// (matching how this route has always behaved), and only JSON literal `true`
/// engages compact.
pub async fn system_backup_post(
    State(state): State<SharedState>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let compact = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("compact").cloned())
        == Some(serde_json::Value::Bool(true));
    match dispatch_core(&state, CoreRequest::SystemBackupCreate { compact }).await {
        Ok(resp) => system_body_public(resp, StatusCode::CREATED),
        Err(r) => r,
    }
}

/// `GET /api/v1/system/backup/{id}` — the single-use download.
pub async fn system_backup_download(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(host) = state.host() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "Server is not ready");
    };
    // Single-use: the entry is gone whether or not the bytes still exist.
    let Some(zip_path) = host.backup_services().take_backup(&id) else {
        return error_json(StatusCode::NOT_FOUND, "Backup not found or has expired");
    };
    let bytes = match std::fs::read(&zip_path) {
        Ok(b) => b,
        Err(_) => {
            quilltap_host::backup_services::remove_zip_and_dir(&zip_path);
            return error_json(StatusCode::NOT_FOUND, "Backup file not found on disk");
        }
    };
    quilltap_host::backup_services::remove_zip_and_dir(&zip_path);

    let filename = format!(
        "quilltap-backup-{}.zip",
        crate::backup_routes::download_stamp(host.backup_services().now_ms())
    );
    let len = bytes.len();
    (
        StatusCode::OK,
        [
            ("content-type", "application/zip".to_string()),
            (
                "content-disposition",
                format!("attachment; filename=\"{filename}\""),
            ),
            ("content-length", len.to_string()),
        ],
        bytes,
    )
        .into_response()
}

/// `POST /api/v1/system/restore` — v4's three-way `?action=` dispatch
/// (`app/api/v1/system/restore/route.ts:246`).
///
/// The body is taken as an `axum::body::Body` rather than `Bytes` so the upload
/// leg can stream it to disk; the two JSON actions collect it first.
pub async fn system_restore_post(
    State(state): State<SharedState>,
    Query(q): Query<HashMap<String, String>>,
    body: axum::body::Body,
) -> AxumResponse {
    match q.get("action").map(String::as_str).unwrap_or("") {
        "upload" => handle_upload(&state, body).await,
        "preview" => {
            let Some(parsed) = collect_json(body).await else {
                return error_json(StatusCode::BAD_REQUEST, "Invalid JSON body");
            };
            let Some(upload_id) = required_upload_id(&parsed) else {
                return error_json(StatusCode::BAD_REQUEST, "uploadId is required");
            };
            match dispatch_core(&state, CoreRequest::SystemRestorePreview { upload_id }).await {
                Ok(resp) => system_body_public(resp, StatusCode::OK),
                Err(r) => r,
            }
        }
        // Default: restore.
        _ => {
            let Some(parsed) = collect_json(body).await else {
                return error_json(StatusCode::BAD_REQUEST, "Invalid JSON body");
            };
            let Some(upload_id) = required_upload_id(&parsed) else {
                return error_json(StatusCode::BAD_REQUEST, "uploadId is required");
            };
            // v4 validates the mode at the route (`:202`) before it ever looks
            // the upload up; the core arm validates it again for the dispatch
            // entrance, which is the one the SPA actually uses.
            let mode = parsed
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if mode != "replace" && mode != "new-account" {
                return error_json(
                    StatusCode::BAD_REQUEST,
                    "mode must be \"replace\" or \"new-account\"",
                );
            }
            match dispatch_core(
                &state,
                CoreRequest::SystemRestoreExecute { upload_id, mode },
            )
            .await
            {
                Ok(resp) => system_body_public(resp, StatusCode::OK),
                Err(r) => r,
            }
        }
    }
}

/// v4 `handleUpload` (`route.ts:86`) — the raw octet-stream body streamed to
/// `<temp>/quilltap-restore-<uuid>.zip`, answering `{success, uploadId, size}`.
///
/// **The back-pressure, in v5's idiom.** v4 has to `await writeStream.once
/// ('drain')` because Node's `write` buffers without bound; a synchronous
/// `write_all` per chunk simply cannot outrun the disk, which is the same
/// guarantee with none of the bookkeeping. The body is never fully resident.
async fn handle_upload(state: &SharedState, body: axum::body::Body) -> AxumResponse {
    let Some(host) = state.host() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "Server is not ready");
    };
    let services = host.backup_services();
    let upload_id = uuid::Uuid::new_v4().to_string();
    let temp_zip_path = services
        .temp_dir()
        .join(format!("quilltap-restore-{upload_id}.zip"));

    let mut file = match std::fs::File::create(&temp_zip_path) {
        Ok(f) => std::io::BufWriter::new(f),
        Err(e) => {
            tracing::error!(error = %e, "restore upload: could not open the temp file");
            return error_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to upload backup file",
            );
        }
    };

    let mut stream = body.into_data_stream();
    let mut total: u64 = 0;
    let mut empty = true;
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                drop(file);
                let _ = std::fs::remove_file(&temp_zip_path);
                tracing::error!(error = %e, upload_id, "restore upload failed");
                return error_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to upload backup file",
                );
            }
        };
        empty = false;
        total += chunk.len() as u64;
        if let Err(e) = file.write_all(&chunk) {
            drop(file);
            let _ = std::fs::remove_file(&temp_zip_path);
            tracing::error!(error = %e, upload_id, "restore upload failed");
            return error_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to upload backup file",
            );
        }
    }
    if let Err(e) = file.flush() {
        drop(file);
        let _ = std::fs::remove_file(&temp_zip_path);
        tracing::error!(error = %e, upload_id, "restore upload failed");
        return error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to upload backup file",
        );
    }
    drop(file);

    // v4 `if (!body) return badRequest('No request body')` (`:90`). A hyper body
    // is never absent, so the observable equivalent is a body that yielded no
    // bytes at all — which is what a `fetch` with no payload produces.
    if empty {
        let _ = std::fs::remove_file(&temp_zip_path);
        return error_json(StatusCode::BAD_REQUEST, "No request body");
    }

    services.store_upload(&upload_id, &temp_zip_path);
    tracing::info!(
        upload_id,
        size = total,
        "[System Restore v1] Upload complete"
    );

    (
        StatusCode::OK,
        [("content-type", "application/json")],
        serde_json::json!({ "success": true, "uploadId": upload_id, "size": total }).to_string(),
    )
        .into_response()
}

/// v4's `await req.json()` inside its `try` — `None` is `badRequest('Invalid
/// JSON body')`.
async fn collect_json(body: axum::body::Body) -> Option<Value> {
    let bytes = axum::body::to_bytes(body, 8 * 1024 * 1024).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// v4's `const { uploadId } = body; if (!uploadId) …` — JS falsiness, so an
/// empty string is "missing" too.
fn required_upload_id(parsed: &Value) -> Option<String> {
    parsed
        .get("uploadId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// v4 `new Date().toISOString().replace(/[:.]/g, '-')` for the download
/// filename (`[id]/route.ts:47`).
fn download_stamp(now_ms: i64) -> String {
    quilltap_core::api::system_backup::iso_from_millis(now_ms)
        .chars()
        .map(|c| if c == ':' || c == '.' { '-' } else { c })
        .collect()
}
