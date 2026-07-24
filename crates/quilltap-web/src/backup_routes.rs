//! P4.9G5 REST edges — the byte-level backup legs.
//!
//! - `GET /api/v1/system/backup/{id}` — the SINGLE-USE archive download.
//! - `POST /api/v1/system/backup` — v4-URL parity for the create verb (which
//!   also rides `POST /api/dispatch` as `systemBackupCreate`); v4 answers 201.
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

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};

use quilltap_core::api::Request as CoreRequest;
use quilltap_core::services::backup::BackupHost;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::system_data_routes::system_body_public;
use crate::text_replacements_routes::dispatch_core;

/// `POST /api/v1/system/backup` — v4 answers **201**.
pub async fn system_backup_post(State(state): State<SharedState>) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::SystemBackupCreate).await {
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

/// v4 `new Date().toISOString().replace(/[:.]/g, '-')` for the download
/// filename (`[id]/route.ts:47`).
fn download_stamp(now_ms: i64) -> String {
    quilltap_core::api::system_backup::iso_from_millis(now_ms)
        .chars()
        .map(|c| if c == ':' || c == '.' { '-' } else { c })
        .collect()
}
