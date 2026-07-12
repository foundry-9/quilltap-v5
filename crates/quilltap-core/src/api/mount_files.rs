//! The mount-point file-operations dispatch surface (P4.6v) — the boundary
//! functions over `services::mount_index`. This unit lands the read path:
//! `mount_files_list` (the files-list route body, consumed by the Scriptorium
//! SPA's DocumentPicker) and `mount_file_read` (the per-file GET JSON envelope).
//!
//! File-op failures map through `file_op_status` to the matching `ErrorKind`;
//! the error message carries v4's `{error}` text (the SPA error translation also
//! keys on the `FileOpError`/`DatabaseStoreError` `code`, surfaced in the message
//! for now — the exact `{error, code}` envelope is a web-edge follow-up).

use crate::db::runtime::Db;
use crate::services::mount_index::file_op_status::file_op_status_for_code;
use crate::services::mount_index::list::mount_files_list as svc_list;
use crate::services::mount_index::read_file::{
    read_mount_file, FileEncoding, MountFileError, ReadMountFileOptions,
};

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
