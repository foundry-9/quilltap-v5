//! The mount-point file-operations dispatch surface (P4.6v) — the boundary
//! functions over `services::mount_index`. This unit lands the read path:
//! `mount_files_list` (the files-list route body, consumed by the Scriptorium
//! SPA's DocumentPicker) and `mount_file_read` (the per-file GET JSON envelope).
//!
//! File-op failures map through `file_op_status` to the matching `ErrorKind`;
//! the error message carries v4's `{error}` text (the SPA error translation also
//! keys on the `FileOpError`/`DatabaseStoreError` `code`, surfaced in the message
//! for now — the exact `{error, code}` envelope is a web-edge follow-up).

use crate::db::doc_mount_points::{DocMountPointsRepository, MountServiceInfo};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::mount_index::converters::default_text_extractor;
use crate::services::mount_index::embedding_scheduler::enqueue_embedding_jobs_for_mount_point;
use crate::services::mount_index::file_op_status::file_op_status_for_code;
use crate::services::mount_index::list::mount_files_list as svc_list;
use crate::services::mount_index::read_file::{
    read_mount_file, FileEncoding, MountFileError, ReadMountFileOptions,
};
use crate::services::mount_index::scanner::scan_mount_point;

use super::types::{ErrorKind, Response};

/// Map a resolved HTTP status (from `file_op_status`) to the boundary `ErrorKind`.
fn kind_for_status(status: u16) -> ErrorKind {
    match status {
        404 => ErrorKind::NotFound,
        409 => ErrorKind::Conflict,
        400 => ErrorKind::BadRequest,
        _ => ErrorKind::Internal,
    }
}

/// Turn a service error into the boundary error response, mapping the
/// `FileOpError` ∪ `DatabaseStoreError` code to its HTTP status → `ErrorKind`.
fn map_error(e: MountFileError) -> Response {
    let (status, msg) = match &e {
        MountFileError::FileOp(fe) => (
            file_op_status_for_code(Some(fe.code.as_str())),
            fe.to_string(),
        ),
        MountFileError::Db(de) => (500, de.to_string()),
        MountFileError::Other(m) => (file_op_status_for_code(None), m.clone()),
    };
    Response::error(kind_for_status(status), msg)
}

/// v4 `GET /api/v1/mount-points/[id]/files` — `{ files, folders }`.
pub fn mount_files_list(db: &Db, mount_point_id: &str) -> Response {
    match svc_list(db, mount_point_id) {
        Ok(list) => Response::MountFile(list.to_json()),
        Err(e) => map_error(e),
    }
}

/// The mount-index write connection off a `WriterSet` (the mount-file mutation
/// family is mount-index-partitioned; job enqueues ride `ws.main()`).
pub(crate) fn mount_conn(
    ws: &crate::db::runtime::WriterSet,
) -> Result<&rusqlite::Connection, DbError> {
    Ok(ws
        .mount_index()
        .ok_or_else(|| DbError::Key("mount files require the mount-index database".to_string()))?
        .connection())
}

/// Load the mount's service row inside a write closure, mapping absence to v4's
/// `notFound('Mount point')`.
pub(crate) fn load_mount_service_info(
    conn: &rusqlite::Connection,
    mount_point_id: &str,
) -> Result<Option<MountServiceInfo>, DbError> {
    DocMountPointsRepository::new(conn).find_service_info_by_id(mount_point_id)
}

/// v4 `POST ?action=scan` (`handleScan`) — run the scan synchronously, then
/// enqueue embedding jobs for the freshly-indexed content. Response body:
/// `{ scanResult, embeddingJobsEnqueued }`.
pub async fn mount_scan(db: &Db, mount_point_id: &str) -> Response {
    let id = mount_point_id.to_string();
    let write = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            let Some(mp) = load_mount_service_info(mount, &id)? else {
                return Ok(None);
            };
            let extractor = default_text_extractor();
            let scan_result = scan_mount_point(mount, &mp, extractor.as_ref());
            let main = ws.main().connection();
            let enqueued = enqueue_embedding_jobs_for_mount_point(main, mount, &id)?;
            Ok(Some((scan_result, enqueued)))
        })
        .await;
    match write {
        Ok(Some((scan_result, enqueued))) => {
            // v4 enqueueJob wakes the pump per job; one post-batch wake is
            // equivalent (idempotent hook).
            if enqueued > 0 {
                crate::services::queue_service::ensure_processor_running();
            }
            Response::MountFile(serde_json::json!({
                "scanResult": scan_result.to_json(),
                "embeddingJobsEnqueued": enqueued,
            }))
        }
        Ok(None) => Response::error(ErrorKind::NotFound, "Mount point not found"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error scanning mount point: {e}");
            Response::error(ErrorKind::Internal, "Failed to scan mount point")
        }
    }
}

/// v4 `GET /api/v1/mount-points/[id]/files/[...path]` JSON envelope form —
/// `readMountFile` serialized. `encoding` is `"utf-8"` / `"base64"`.
pub fn mount_file_read(
    db: &Db,
    mount_point_id: &str,
    path: &str,
    encoding: Option<&str>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Response {
    let encoding = match encoding {
        Some("utf-8") => Some(FileEncoding::Utf8),
        Some("base64") => Some(FileEncoding::Base64),
        Some(other) => {
            return Response::error(ErrorKind::BadRequest, format!("invalid encoding: {other}"))
        }
        None => None,
    };
    let opts = ReadMountFileOptions {
        encoding,
        offset,
        limit,
    };
    match read_mount_file(db, mount_point_id, path, opts) {
        Ok(r) => Response::MountFile(r.to_json()),
        Err(e) => map_error(e),
    }
}
