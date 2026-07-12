//! Port of v4 `lib/mount-index/file-op-status.ts` — maps mount-point
//! file-operation error codes (`FileOpError` ∪ `DatabaseStoreError`) to HTTP
//! status codes. Shared by every route that surfaces these errors so the mapping
//! stays in one place. Anything unrecognised defaults to 500 so genuine bugs
//! surface as server errors.

use crate::db::database_store::DbStoreErrorCode;
use crate::services::mount_index::file_op_error::FileOpErrorCode;

/// Resolve the HTTP status for a mount-index file-op error code token. `None`
/// (an unrecognised / non-typed error) → 500, matching v4's default arm.
pub fn file_op_status_for_code(code: Option<&str>) -> u16 {
    match code {
        Some("MOUNT_NOT_FOUND") | Some("SOURCE_NOT_FOUND") | Some("NOT_FOUND") => 404,
        Some("DEST_EXISTS") | Some("CONFLICT") | Some("NOT_EMPTY") => 409,
        Some("INVALID_PATH") | Some("INVALID") | Some("UNSUPPORTED") => 400,
        // VERIFY_FAILED and everything else → 500.
        _ => 500,
    }
}

/// Status for a typed [`FileOpError`] code.
pub fn file_op_status(code: FileOpErrorCode) -> u16 {
    file_op_status_for_code(Some(code.as_str()))
}

/// The wire code string v4 puts on a `DatabaseStoreError` body — the union arm
/// the SPA error translation keys on alongside `FileOpError`.
pub fn db_store_code_str(code: DbStoreErrorCode) -> &'static str {
    match code {
        DbStoreErrorCode::NotFound => "NOT_FOUND",
        DbStoreErrorCode::Unsupported => "UNSUPPORTED",
        DbStoreErrorCode::Conflict => "CONFLICT",
        DbStoreErrorCode::Invalid => "INVALID",
        DbStoreErrorCode::NotEmpty => "NOT_EMPTY",
    }
}

/// Status for a typed [`DbStoreErrorCode`].
pub fn database_store_status(code: DbStoreErrorCode) -> u16 {
    file_op_status_for_code(Some(db_store_code_str(code)))
}
