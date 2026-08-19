//! The mount-point file-operations dispatch surface (P4.6v/P4.6y) — the
//! boundary functions over `services::mount_index`: the read/list keystone,
//! the whole mutation family (write/ingest, move/copy/link/delete, folder
//! ops, PATCH, blobs), the indexing verbs (scan/reindex/embed), semantic
//! search, and the refusal-armed convert/deconvert.
//!
//! File-op failures map through `file_op_status` to the matching `ErrorKind`
//! and carry v4's `{error, code}` body on the envelope (`CoreError.code` —
//! the `FileOpError` ∪ `DatabaseStoreError` union the SPA error translation
//! keys on).

use crate::db::doc_mount_points::{DocMountPointsRepository, MountServiceInfo};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::mount_index::base_path_availability::BasePathUnavailableError;
use crate::services::mount_index::converters::default_text_extractor;
use crate::services::mount_index::embedding_scheduler::enqueue_embedding_jobs_for_mount_point;
use crate::services::mount_index::file_op_status::file_op_status_for_code;
use crate::services::mount_index::list::mount_files_list as svc_list;
use crate::services::mount_index::read_file::{
    read_mount_file, FileEncoding, MountFileError, ReadMountFileOptions,
};
use crate::services::mount_index::scanner::{scan_mount_point, CreateFolderError};

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
/// `FileOpError` ∪ `DatabaseStoreError` code to its HTTP status → `ErrorKind`
/// and carrying the code on the envelope (v4's `{error, code}` body — the SPA
/// error translation keys on it).
fn map_error(e: MountFileError) -> Response {
    use crate::services::mount_index::file_op_status::db_store_code_str;
    match &e {
        MountFileError::FileOp(fe) => Response::error_coded(
            kind_for_status(file_op_status_for_code(Some(fe.code.as_str()))),
            fe.to_string(),
            fe.code.as_str(),
        ),
        MountFileError::Store(se) => Response::error_coded(
            kind_for_status(file_op_status_for_code(Some(db_store_code_str(se.code)))),
            se.to_string(),
            db_store_code_str(se.code),
        ),
        MountFileError::Db(de) => Response::error(ErrorKind::Internal, de.to_string()),
        MountFileError::Other(m) => {
            Response::error(kind_for_status(file_op_status_for_code(None)), m.clone())
        }
    }
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
        .ok_or_else(|| {
            DbError::Internal("mount files require the mount-index database".to_string())
        })?
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

/// v4's per-handler catch split: the FILE-op handlers
/// (`handleMoveFile`/`Copy`/`Link`/`WriteFile`/`DeleteFile` + the item-route
/// DELETE) catch ONLY `FileOpError` into the `{error, code}` body — a
/// `DatabaseStoreError` falls to the generic 500. The FOLDER handlers catch
/// both. Reproduced exactly.
fn map_file_op_only(e: MountFileError, failed_msg: &str) -> Response {
    match &e {
        MountFileError::FileOp(fe) => Response::error_coded(
            kind_for_status(file_op_status_for_code(Some(fe.code.as_str()))),
            fe.to_string(),
            fe.code.as_str(),
        ),
        other => {
            eprintln!("[Mount Points v1] {failed_msg}: {other}");
            Response::error(ErrorKind::Internal, failed_msg)
        }
    }
}

/// The folder handlers' arm: `FileOpError` OR `DatabaseStoreError` → coded.
fn map_file_or_store(e: MountFileError, failed_msg: &str) -> Response {
    match &e {
        MountFileError::FileOp(_) | MountFileError::Store(_) => map_error(e),
        other => {
            eprintln!("[Mount Points v1] {failed_msg}: {other}");
            Response::error(ErrorKind::Internal, failed_msg)
        }
    }
}

/// Run a mount-partition mutation on the writer, returning the service-level
/// result out of the closure (infrastructure `DbError`s become the generic
/// failure message).
async fn run_mount_op<F>(
    db: &Db,
    f: F,
) -> Result<Result<serde_json::Value, MountFileError>, DbError>
where
    F: FnOnce(&rusqlite::Connection) -> Result<serde_json::Value, MountFileError> + Send + 'static,
{
    db.write(move |ws| {
        let mount = mount_conn(ws)?;
        Ok(f(mount))
    })
    .await
}

/// v4 `?action=move-file` (`handleMoveFile`) — `moveFile` (same-mount rename or
/// cross-mount relocate). Body: the v4 `FileOpResult`.
pub async fn mount_file_move(
    db: &Db,
    mount_point_id: &str,
    source_path: &str,
    dest_mount_point_id: &str,
    dest_path: &str,
) -> Response {
    let (mp, sp, dmp, dp) = (
        mount_point_id.to_string(),
        source_path.to_string(),
        dest_mount_point_id.to_string(),
        dest_path.to_string(),
    );
    let out = run_mount_op(db, move |conn| {
        let extractor = default_text_extractor();
        crate::services::mount_index::file_ops::move_file(
            conn,
            &mp,
            &sp,
            &dmp,
            &dp,
            extractor.as_ref(),
        )
        .map(|r| r.to_json())
    })
    .await;
    match out {
        Ok(Ok(v)) => Response::MountFile(v),
        Ok(Err(e)) => map_file_op_only(e, "Failed to move file"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error moving file: {e}");
            Response::error(ErrorKind::Internal, "Failed to move file")
        }
    }
}

/// v4 `?action=copy-file` (`handleCopyFile`). Body: the v4 `FileOpResult`.
pub async fn mount_file_copy(
    db: &Db,
    mount_point_id: &str,
    source_path: &str,
    dest_mount_point_id: &str,
    dest_path: &str,
    force: Option<bool>,
) -> Response {
    let opts = crate::services::mount_index::file_ops::CopyOpts {
        source_mount_point_id: mount_point_id.to_string(),
        source_path: source_path.to_string(),
        dest_mount_point_id: dest_mount_point_id.to_string(),
        dest_path: dest_path.to_string(),
        force: force.unwrap_or(false),
    };
    let out = run_mount_op(db, move |conn| {
        let extractor = default_text_extractor();
        crate::services::mount_index::file_ops::copy_file(conn, &opts, extractor.as_ref())
            .map(|r| r.to_json())
    })
    .await;
    match out {
        Ok(Ok(v)) => Response::MountFile(v),
        Ok(Err(e)) => map_file_op_only(e, "Failed to copy file"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error copying file: {e}");
            Response::error(ErrorKind::Internal, "Failed to copy file")
        }
    }
}

/// v4 `?action=link-file` (`handleLinkFile`) — a true hard link; cross-storage
/// and cross-device are `UNSUPPORTED`, never a silent byte copy.
pub async fn mount_file_link(
    db: &Db,
    mount_point_id: &str,
    source_path: &str,
    dest_mount_point_id: &str,
    dest_path: &str,
) -> Response {
    let (mp, sp, dmp, dp) = (
        mount_point_id.to_string(),
        source_path.to_string(),
        dest_mount_point_id.to_string(),
        dest_path.to_string(),
    );
    let out = run_mount_op(db, move |conn| {
        let extractor = default_text_extractor();
        crate::services::mount_index::file_ops::link_file(
            conn,
            &mp,
            &sp,
            &dmp,
            &dp,
            extractor.as_ref(),
        )
        .map(|r| r.to_json())
    })
    .await;
    match out {
        Ok(Ok(v)) => Response::MountFile(v),
        Ok(Err(e)) => map_file_op_only(e, "Failed to link file"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error linking file: {e}");
            Response::error(ErrorKind::Internal, "Failed to link file")
        }
    }
}

/// v4 `?action=delete-file` (`handleDeleteFile`) + the item-route DELETE share
/// `deleteFile`; the item route 404s a `deleted:false` result while the action
/// verb returns it verbatim — the WEB EDGE maps the two shapes, this variant is
/// the action-verb behavior (`{deleted, mountPointId, path}`).
pub async fn mount_file_delete(db: &Db, mount_point_id: &str, path: &str) -> Response {
    let (mp, p) = (mount_point_id.to_string(), path.to_string());
    let out = run_mount_op(db, move |conn| {
        crate::services::mount_index::file_ops::delete_file(conn, &mp, &p)
    })
    .await;
    match out {
        Ok(Ok(v)) => Response::MountFile(v),
        Ok(Err(e)) => map_file_op_only(e, "Failed to delete file"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error deleting file: {e}");
            Response::error(ErrorKind::Internal, "Failed to delete file")
        }
    }
}

/// v4 `?action=delete-folder` (`handleDeleteFolder`) — refuses non-empty
/// folders (`CONFLICT`/`NOT_EMPTY`).
pub async fn mount_folder_delete(db: &Db, mount_point_id: &str, path: &str) -> Response {
    let (mp, p) = (mount_point_id.to_string(), path.to_string());
    let out = run_mount_op(db, move |conn| {
        crate::services::mount_index::folder_ops::delete_folder(conn, &mp, &p)
    })
    .await;
    match out {
        Ok(Ok(v)) => Response::MountFile(v),
        Ok(Err(e)) => map_file_or_store(e, "Failed to delete folder"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error deleting folder: {e}");
            Response::error(ErrorKind::Internal, "Failed to delete folder")
        }
    }
}

/// v4 `?action=move-folder` (`handleMoveFolder`) — same-mount only; fs mounts
/// rename on disk + rewrite the affected links' path prefixes.
pub async fn mount_folder_move(
    db: &Db,
    mount_point_id: &str,
    from_path: &str,
    to_path: &str,
) -> Response {
    let (mp, fp, tp) = (
        mount_point_id.to_string(),
        from_path.to_string(),
        to_path.to_string(),
    );
    let out = run_mount_op(db, move |conn| {
        crate::services::mount_index::folder_ops::move_folder(conn, &mp, &fp, &tp)
    })
    .await;
    match out {
        Ok(Ok(v)) => Response::MountFile(v),
        Ok(Err(e)) => map_file_or_store(e, "Failed to move folder"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error moving folder: {e}");
            Response::error(ErrorKind::Internal, "Failed to move folder")
        }
    }
}

/// v4 item-route PATCH — rename and/or set description. Rename runs FIRST (so
/// a description update lands on the new path); descriptions are BLOB-only (a
/// text document 400s). Body: `{mountPointId, relativePath, renamed,
/// descriptionUpdated, fileType?}`.
pub async fn mount_file_update(
    db: &Db,
    mount_point_id: &str,
    path: &str,
    description: Option<String>,
    rename: Option<String>,
) -> Response {
    if description.is_none() && rename.is_none() {
        // v4's zod `.refine` message, verbatim.
        return Response::error(
            ErrorKind::BadRequest,
            "PATCH requires \"description\" or \"rename\"",
        );
    }
    let (mp, p) = (mount_point_id.to_string(), path.to_string());
    let desc = description.clone();
    let ren = rename.clone();
    let out = run_mount_op(db, move |conn| {
        let mut current_path = p.clone();
        if let Some(rename_to) = &ren {
            let extractor = default_text_extractor();
            let moved = crate::services::mount_index::file_ops::move_file(
                conn,
                &mp,
                &current_path,
                &mp,
                rename_to,
                extractor.as_ref(),
            )?;
            current_path = moved.dest_path;
        }
        if let Some(d) = &desc {
            let blobs = crate::db::doc_mount_blobs::DocMountBlobsRepository::new(conn);
            let Some(meta) = blobs.find_by_mount_point_and_path(&mp, &current_path)? else {
                // v4's badRequest text, verbatim (NOT a FileOpError).
                return Err(MountFileError::Other(
                    "__BLOB_ONLY_DESCRIPTION__".to_string(),
                ));
            };
            blobs.update_description(&meta.id, d, Some(&meta.link_id))?;
        }
        Ok(serde_json::json!({ "currentPath": current_path }))
    })
    .await;
    match out {
        Ok(Ok(v)) => {
            let current_path = v["currentPath"].as_str().unwrap_or(path).to_string();
            // v4: re-read for the fileType (base64, 1 line), error-swallowed.
            let updated = read_mount_file(
                db,
                mount_point_id,
                &current_path,
                ReadMountFileOptions {
                    encoding: Some(FileEncoding::Base64),
                    offset: None,
                    limit: Some(1),
                },
            )
            .ok();
            let mut body = serde_json::Map::new();
            body.insert("mountPointId".into(), mount_point_id.into());
            body.insert("relativePath".into(), current_path.into());
            body.insert("renamed".into(), rename.is_some().into());
            body.insert("descriptionUpdated".into(), description.is_some().into());
            if let Some(u) = updated {
                body.insert("fileType".into(), u.file_type.into());
            }
            Response::MountFile(serde_json::Value::Object(body))
        }
        Ok(Err(MountFileError::Other(m))) if m == "__BLOB_ONLY_DESCRIPTION__" => Response::error(
            ErrorKind::BadRequest,
            "Descriptions are only supported for binary (blob) files",
        ),
        Ok(Err(e)) => map_error(e),
        Err(e) => {
            eprintln!("[Mount Points v1] Error update file: {e}");
            Response::error(ErrorKind::Internal, "Failed to update file")
        }
    }
}

/// v4 folders-route `normaliseRelativePath` (the route-LOCAL simple form — no
/// dot-collapsing, unlike `path_utils::normalise_relative_path`).
fn folders_route_normalise(input: &str) -> String {
    input
        .trim_start_matches('/')
        .trim_end_matches('/')
        .replace('\\', "/")
}

/// v4 folders-route `isPathSafe` — rejects absolute paths, `.`/`..`/empty
/// segments, control chars, `<>:"|?*\`, and >200-char segments. Unit-pinned.
pub fn is_path_safe(rel: &str) -> bool {
    if rel.is_empty() {
        return false;
    }
    if rel.starts_with('/') {
        return false;
    }
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return false;
        }
        if seg.chars().any(|c| ('\x00'..='\x1f').contains(&c)) {
            return false;
        }
        if seg
            .chars()
            .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\'))
        {
            return false;
        }
        if seg.chars().count() > 200 {
            return false;
        }
    }
    true
}

/// v4 `POST /api/v1/mount-points/[id]/folders` — create a folder (database →
/// an explicit `doc_mount_folders` row; fs → mkdir with the escape guard).
/// Body: `{path}`.
///
/// How the create can refuse. v4 discriminates by `instanceof` inside the
/// route's catch; the write closure has to carry the shape back out.
enum FolderCreateRefusal {
    NotFound,
    BadRequest(String),
    /// [ed8934f1] The store's own root is unreachable, so this is a
    /// configuration problem rather than a bad request or a server fault. Hand
    /// back the diagnosis — it names the remedy, which a bare 500 never could.
    BasePathUnavailable(BasePathUnavailableError),
}

pub async fn mount_folder_create(db: &Db, mount_point_id: &str, path: &str) -> Response {
    if path.is_empty() || path.chars().count() > 1024 {
        return Response::error(ErrorKind::BadRequest, "Folder path is required");
    }
    let rel = folders_route_normalise(path);
    if !is_path_safe(&rel) {
        return Response::error(
            ErrorKind::BadRequest,
            "Folder path contains invalid characters or segments",
        );
    }
    let id = mount_point_id.to_string();
    let out = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            let Some(mp) = load_mount_service_info(mount, &id)? else {
                return Ok(Err(FolderCreateRefusal::NotFound));
            };
            if !mp.enabled {
                return Ok(Err(FolderCreateRefusal::BadRequest(
                    "Mount point is disabled".to_string(),
                )));
            }
            if mp.mount_type == "database" {
                let (_, created) =
                    crate::db::database_store::create_database_folder(mount, &id, &rel)?;
                return Ok(Ok(created));
            }
            let Some(base) = mp.base_path.as_deref().filter(|b| !b.is_empty()) else {
                return Ok(Err(FolderCreateRefusal::BadRequest(
                    "Mount point has no base path configured".to_string(),
                )));
            };
            match crate::services::mount_index::scanner::create_filesystem_folder(base, &id, &rel) {
                Ok(()) => Ok(Ok(rel.clone())),
                Err(CreateFolderError::Escapes) => Ok(Err(FolderCreateRefusal::BadRequest(
                    "Folder path escapes mount point boundary".to_string(),
                ))),
                Err(CreateFolderError::BasePathUnavailable(e)) => {
                    Ok(Err(FolderCreateRefusal::BasePathUnavailable(e)))
                }
                Err(CreateFolderError::Io(msg)) => Err(crate::db::DbError::Internal(msg)),
            }
        })
        .await;
    match out {
        Ok(Ok(created)) => Response::MountFile(serde_json::json!({ "path": created })),
        Ok(Err(FolderCreateRefusal::NotFound)) => {
            Response::error(ErrorKind::NotFound, "Mount point not found")
        }
        Ok(Err(FolderCreateRefusal::BadRequest(m))) => Response::error(ErrorKind::BadRequest, m),
        Ok(Err(FolderCreateRefusal::BasePathUnavailable(e))) => {
            tracing::warn!(
                mount_point_id = %mount_point_id,
                base_path = %e.base_path,
                reason = %e.reason.as_str(),
                containerized = e.containerized,
                "[Mount Points v1] Folder creation refused — base path unavailable"
            );
            Response::error(ErrorKind::Conflict, e.message)
        }
        Err(e) => {
            eprintln!("[Mount Points v1] Error creating folder in mount point: {e}");
            Response::error(ErrorKind::Internal, "Failed to create folder")
        }
    }
}

/// Node `Buffer.from(s, 'base64')`-style forgiving decode: non-alphabet chars
/// are skipped, padding is optional, a trailing partial quantum is dropped.
fn node_base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine;
    let filtered: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/'))
        .collect();
    let usable = filtered.len() - (filtered.len() % 4).min(filtered.len() % 4);
    let mut out = Vec::new();
    // Decode the longest valid prefix; Node keeps whole quantums plus a final
    // 2-or-3-char partial (which yields 1-2 bytes).
    let whole = &filtered[..usable - (usable % 4)];
    if let Ok(mut b) = base64::engine::general_purpose::STANDARD_NO_PAD.decode(whole) {
        out.append(&mut b);
    }
    let rest = &filtered[whole.len()..];
    if rest.len() >= 2 {
        if let Ok(mut b) = base64::engine::general_purpose::STANDARD_NO_PAD.decode(rest) {
            out.append(&mut b);
        }
    }
    out
}

/// The write payload → raw bytes (v4 `Buffer.from(content, encoding)`).
fn decode_write_content(content: &str, encoding: Option<&str>) -> Result<Vec<u8>, Response> {
    match encoding.unwrap_or("utf-8") {
        "utf-8" => Ok(content.as_bytes().to_vec()),
        "base64" => Ok(node_base64_decode(content)),
        other => Err(Response::error(
            ErrorKind::BadRequest,
            format!("Invalid body: invalid encoding {other}"),
        )),
    }
}

/// v4 item-route PUT — the canonical ingest write (`storeMountFile` with
/// `collisionStrategy: 'overwrite'`). Body: `{mountPointId, relativePath,
/// kind, fileType, sha256, sizeBytes, mimeType, mtime}`. The multipart PUT leg
/// maps onto the same variant at the web edge (originalMimeType/FileName ride
/// along).
#[allow(clippy::too_many_arguments)]
pub async fn mount_file_write(
    db: &Db,
    mount_point_id: &str,
    path: &str,
    content: &str,
    encoding: Option<&str>,
    expected_mtime: Option<i64>,
    force: Option<bool>,
    original_mime_type: Option<String>,
    original_file_name: Option<String>,
    webp: Option<std::sync::Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>>,
) -> Response {
    let data = match decode_write_content(content, encoding) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut input =
        crate::services::mount_index::store_file::StoreFileInput::new(mount_point_id, path, data);
    input.original_mime_type = original_mime_type;
    input.original_file_name = original_file_name;
    input.expected_mtime = expected_mtime;
    input.force = force.unwrap_or(false);
    input.collision_strategy =
        crate::services::mount_index::store_file::CollisionStrategy::Overwrite;

    let out = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            let main = ws.main().connection();
            let extractor = default_text_extractor();
            // P4.6bf S1 wire: the host's live WebP transcoder when present, else the
            // store-original refusal fallback (behavior unchanged on `None`).
            let refusing = crate::services::mount_index::blob_transcode::RefusingWebpTranscoder;
            let webp_ref: &dyn crate::services::mount_index::blob_transcode::WebpTranscoder =
                match &webp {
                    Some(w) => w.as_ref(),
                    None => &refusing,
                };
            Ok(crate::services::mount_index::store_file::store_mount_file(
                main,
                mount,
                &input,
                extractor.as_ref(),
                webp_ref,
            ))
        })
        .await;
    match out {
        Ok(Ok(r)) => Response::MountFile(serde_json::json!({
            "mountPointId": r.mount_point_id,
            "relativePath": r.relative_path,
            "kind": r.kind,
            "fileType": r.file_type,
            "sha256": r.sha256,
            "sizeBytes": r.size_bytes,
            "mimeType": r.stored_mime_type,
            "mtime": r.mtime,
        })),
        // The item route's handleFileOpError codes FileOpError AND
        // DatabaseStoreError.
        Ok(Err(e @ (MountFileError::FileOp(_) | MountFileError::Store(_)))) => map_error(e),
        Ok(Err(e)) => {
            eprintln!("[Mount Points v1] Error write file: {e}");
            Response::error(ErrorKind::Internal, "Failed to write file")
        }
        Err(e) => {
            eprintln!("[Mount Points v1] Error write file: {e}");
            Response::error(ErrorKind::Internal, "Failed to write file")
        }
    }
}

/// v4 `?action=write-file` (`handleWriteFile`) — the BYTE-PRESERVING
/// `file_ops::writeFile` (multipart at the web edge; distinct behavior from
/// the transcoding PUT ingest — quirk 1 of the survey). Body: the v4
/// `WriteResult` `{sha256, sizeBytes, destPath, mountPointId}`.
pub async fn mount_file_write_raw(
    db: &Db,
    mount_point_id: &str,
    path: &str,
    data_base64: &str,
    force: Option<bool>,
) -> Response {
    let data = node_base64_decode(data_base64);
    let (mp, p) = (mount_point_id.to_string(), path.to_string());
    let force = force.unwrap_or(false);
    let out = run_mount_op(db, move |conn| {
        let extractor = default_text_extractor();
        crate::services::mount_index::file_ops::write_file(
            conn,
            &mp,
            &p,
            &data,
            force,
            extractor.as_ref(),
        )
    })
    .await;
    match out {
        Ok(Ok(v)) => Response::MountFile(v),
        Ok(Err(e)) => map_file_op_only(e, "Failed to write file"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error writing file: {e}");
            Response::error(ErrorKind::Internal, "Failed to write file")
        }
    }
}

/// v4 `POST .../blobs` — the multipart blob upload through the single ingest
/// pipeline (`assetStorage: 'database'`, `collisionStrategy: 'overwrite'`).
/// Body: `{document}` for a native-text upload, `{blob}` (the full joined
/// view) otherwise; the web edge returns 201.
#[allow(clippy::too_many_arguments)]
pub async fn mount_blob_upload(
    db: &Db,
    mount_point_id: &str,
    path: &str,
    description: Option<String>,
    data_base64: &str,
    original_mime_type: Option<String>,
    original_file_name: Option<String>,
    webp: Option<std::sync::Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>>,
) -> Response {
    let data = node_base64_decode(data_base64);
    if data.is_empty() {
        return Response::error(ErrorKind::BadRequest, "Empty file payload");
    }
    // v4: `file.type || 'application/octet-stream'`,
    // `file.name || relativePath.split('/').pop() || 'blob'`.
    let mime = original_mime_type
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let name = original_file_name
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            let base = path.rsplit('/').next().unwrap_or("");
            if base.is_empty() {
                "blob".to_string()
            } else {
                base.to_string()
            }
        });
    // v4 checks the mount exists BEFORE parsing the form.
    let exists = {
        let id = mount_point_id.to_string();
        db.read_mount_index(move |conn| {
            DocMountPointsRepository::new(conn).find_service_info_by_id(&id)
        })
    };
    match exists {
        Ok(Some(_)) => {}
        Ok(None) => return Response::error(ErrorKind::NotFound, "Mount point not found"),
        Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
    }

    let mut input =
        crate::services::mount_index::store_file::StoreFileInput::new(mount_point_id, path, data);
    input.original_mime_type = Some(mime);
    input.original_file_name = Some(name);
    input.description = Some(description.unwrap_or_default());
    input.asset_storage = crate::services::mount_index::store_file::AssetStorageMode::Database;
    input.collision_strategy =
        crate::services::mount_index::store_file::CollisionStrategy::Overwrite;

    let out = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            let main = ws.main().connection();
            let extractor = default_text_extractor();
            // P4.6bf S1 wire: the host's live WebP transcoder when present, else the
            // store-original refusal fallback (behavior unchanged on `None`).
            let refusing = crate::services::mount_index::blob_transcode::RefusingWebpTranscoder;
            let webp_ref: &dyn crate::services::mount_index::blob_transcode::WebpTranscoder =
                match &webp {
                    Some(w) => w.as_ref(),
                    None => &refusing,
                };
            let stored = crate::services::mount_index::store_file::store_mount_file(
                main,
                mount,
                &input,
                extractor.as_ref(),
                webp_ref,
            );
            match stored {
                Ok(r) if r.kind == "document" => Ok(Ok(serde_json::json!({
                    "document": {
                        "id": r.file_id.clone().map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                        "mountPointId": input.mount_point_id,
                        "relativePath": r.relative_path,
                        "fileName": r.relative_path.rsplit('/').next().unwrap_or(&r.relative_path),
                        "fileType": r.file_type,
                        "sizeBytes": r.size_bytes,
                        "lastModified": crate::clock::iso_from_unix_ms(r.mtime),
                    }
                }))),
                Ok(r) => {
                    let blob = crate::db::doc_mount_blobs::DocMountBlobsRepository::new(mount)
                        .find_full_json_by_mount_point_and_path(
                            &input.mount_point_id,
                            &r.relative_path,
                        )?;
                    Ok(Ok(serde_json::json!({
                        "blob": blob.unwrap_or(serde_json::Value::Null)
                    })))
                }
                Err(e) => Ok(Err(e)),
            }
        })
        .await;
    match out {
        Ok(Ok(v)) => Response::MountFile(v),
        Ok(Err(e @ (MountFileError::FileOp(_) | MountFileError::Store(_)))) => map_error(e),
        Ok(Err(e)) => {
            eprintln!("[Mount Points v1] Error uploading blob: {e}");
            Response::error(ErrorKind::Internal, "Failed to upload blob")
        }
        Err(e) => {
            eprintln!("[Mount Points v1] Error uploading blob: {e}");
            Response::error(ErrorKind::Internal, "Failed to upload blob")
        }
    }
}

/// v4 `GET .../blobs` — the blob-metadata listing (`?folder=` scoped).
/// Body: `{blobs}` (the full joined view rows, nulls literal).
pub fn mount_blobs_list(db: &Db, mount_point_id: &str, folder: Option<&str>) -> Response {
    let id = mount_point_id.to_string();
    let f = folder.map(|s| s.to_string());
    let out = db.read_mount_index(move |conn| {
        let mp = DocMountPointsRepository::new(conn).find_service_info_by_id(&id)?;
        if mp.is_none() {
            return Ok(None);
        }
        let blobs = crate::db::doc_mount_blobs::DocMountBlobsRepository::new(conn)
            .list_full_json_by_mount_point(&id, f.as_deref())?;
        Ok(Some(blobs))
    });
    match out {
        Ok(Some(blobs)) => Response::MountFile(serde_json::json!({ "blobs": blobs })),
        Ok(None) => Response::error(ErrorKind::NotFound, "Mount point not found"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error listing blobs: {e}");
            Response::error(ErrorKind::Internal, "Failed to list blobs")
        }
    }
}

/// v4 blob-item DELETE — remove the blob at the path, FALLING BACK to
/// `doc_mount_documents` when no blob row matches (text docs live there; one
/// URL scheme either way). Body: `{success: true}`.
pub async fn mount_blob_delete(db: &Db, mount_point_id: &str, path: &str) -> Response {
    let (mp, p) = (mount_point_id.to_string(), path.to_string());
    let out = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            let blobs = crate::db::doc_mount_blobs::DocMountBlobsRepository::new(mount);
            if blobs.delete_by_mount_point_and_path(&mp, &p)? {
                return Ok(true);
            }
            crate::db::database_store::delete_database_document(mount, &mp, &p)
        })
        .await;
    match out {
        Ok(true) => Response::MountFile(serde_json::json!({ "success": true })),
        Ok(false) => Response::error(ErrorKind::NotFound, "Blob not found"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error deleting blob: {e}");
            Response::error(ErrorKind::Internal, "Failed to delete blob")
        }
    }
}

/// v4 blob-item PATCH — update the description (blob rows only; no documents
/// fallback here). Body: `{blob}` (the refreshed joined view).
pub async fn mount_blob_update(
    db: &Db,
    mount_point_id: &str,
    path: &str,
    description: Option<String>,
) -> Response {
    let (mp, p) = (mount_point_id.to_string(), path.to_string());
    // v4: a non-string description coerces to ''.
    let desc = description.unwrap_or_default();
    let out = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            let blobs = crate::db::doc_mount_blobs::DocMountBlobsRepository::new(mount);
            let Some(meta) = blobs.find_by_mount_point_and_path(&mp, &p)? else {
                return Ok(None);
            };
            // v4's route calls updateDescription WITHOUT a linkId (the first
            // link of the blob's file is targeted).
            let updated = blobs.update_description(&meta.id, &desc, None)?;
            match updated {
                Some(u) => Ok(Some(
                    blobs
                        .find_full_json_by_link_id(&u.link_id)?
                        .unwrap_or(serde_json::Value::Null),
                )),
                None => Ok(Some(serde_json::Value::Null)),
            }
        })
        .await;
    match out {
        Ok(Some(blob)) => Response::MountFile(serde_json::json!({ "blob": blob })),
        Ok(None) => Response::error(ErrorKind::NotFound, "Blob not found"),
        Err(e) => {
            eprintln!("[Mount Points v1] Error updating blob description: {e}");
            Response::error(ErrorKind::Internal, "Failed to update blob description")
        }
    }
}

/// v4 `?action=convert` (`handleConvert`) — the capability guards run LIVE
/// (already-database / mid-conversion quiesce), then the conversion itself is
/// a loud typed refusal: `conversion.ts` (convert/deconvert + the on-disk
/// migration walk) is a full future unit deferred by work order P4.6y.
pub async fn mount_convert(db: &Db, mount_point_id: &str) -> Response {
    let id = mount_point_id.to_string();
    let guard = db.read_mount_index(move |conn| {
        DocMountPointsRepository::new(conn).find_service_info_by_id(&id)
    });
    match guard {
        Ok(None) => Response::error(ErrorKind::NotFound, "Mount point not found"),
        Ok(Some(mp)) if mp.mount_type != "filesystem" && mp.mount_type != "obsidian" => {
            Response::error(
                ErrorKind::BadRequest,
                format!(
                    "Mount point is already database-backed (mountType={})",
                    mp.mount_type
                ),
            )
        }
        Ok(Some(mp))
            if mp.conversion_status == "converting" || mp.conversion_status == "deconverting" =>
        {
            Response::error(
                ErrorKind::BadRequest,
                "A conversion is already in progress for this mount point",
            )
        }
        Ok(Some(_)) => Response::error(
            ErrorKind::Internal,
            "The 'convert' mount-point action is recognized but not yet available              (the store-conversion machinery is deferred by work order P4.6y).",
        ),
        Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
    }
}

/// v4 `?action=deconvert` (`handleDeconvert`) — guards live, conversion
/// refused (see [`mount_convert`]).
pub async fn mount_deconvert(db: &Db, mount_point_id: &str, target_path: &str) -> Response {
    if target_path.is_empty() {
        return Response::error(
            ErrorKind::BadRequest,
            "Deconvert requires a non-empty targetPath",
        );
    }
    let id = mount_point_id.to_string();
    let guard = db.read_mount_index(move |conn| {
        DocMountPointsRepository::new(conn).find_service_info_by_id(&id)
    });
    match guard {
        Ok(None) => Response::error(ErrorKind::NotFound, "Mount point not found"),
        Ok(Some(mp)) if mp.mount_type != "database" => Response::error(
            ErrorKind::BadRequest,
            format!(
                "Mount point is not database-backed (mountType={}); nothing to deconvert",
                mp.mount_type
            ),
        ),
        Ok(Some(mp))
            if mp.conversion_status == "converting" || mp.conversion_status == "deconverting" =>
        {
            Response::error(
                ErrorKind::BadRequest,
                "A conversion is already in progress for this mount point",
            )
        }
        Ok(Some(_)) => Response::error(
            ErrorKind::Internal,
            "The 'deconvert' mount-point action is recognized but not yet available              (the store-conversion machinery is deferred by work order P4.6y).",
        ),
        Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
    }
}

/// v4 `POST ?action=reindex` (`handleReindex`) — `reindexLinks` synchronous
/// in-request. Body: `{mountPointId, mountName, processed, succeeded, failed,
/// skipped, errors}`.
pub async fn mount_reindex(
    db: &Db,
    mount_point_id: &str,
    path: Option<String>,
    force: Option<bool>,
) -> Response {
    let id = mount_point_id.to_string();
    let opts = crate::services::mount_index::reindex::ReindexOptions {
        path,
        force: force.unwrap_or(false),
    };
    let write = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            let Some(mp) = load_mount_service_info(mount, &id)? else {
                return Ok(None);
            };
            let extractor = default_text_extractor();
            let result = crate::services::mount_index::reindex::reindex_links(
                mount,
                &mp,
                &opts,
                extractor.as_ref(),
            )?;
            Ok(Some((mp.name, result)))
        })
        .await;
    match write {
        Ok(Some((mount_name, r))) => Response::MountFile(serde_json::json!({
            "mountPointId": mount_point_id,
            "mountName": mount_name,
            "processed": r.processed,
            "succeeded": r.succeeded,
            "failed": r.failed,
            "skipped": r.skipped,
            "errors": r.errors_json(),
        })),
        Ok(None) => Response::error(ErrorKind::NotFound, "Mount point not found"),
        Err(e) => Response::error(ErrorKind::Internal, format!("Reindex failed: {e}")),
    }
}

/// v4 `POST ?action=embed` (`handleEmbed`) — the scoped EMBEDDING_GENERATE
/// enqueue. Body: `{mountPointId, mountName, jobs, queued, skipped}`.
pub async fn mount_embed(
    db: &Db,
    mount_point_id: &str,
    path: Option<String>,
    force: Option<bool>,
) -> Response {
    use crate::services::mount_index::reindex::{
        enqueue_embedding_jobs_scoped, ReindexOptions, ScopedEnqueueError,
    };
    let id = mount_point_id.to_string();
    let opts = ReindexOptions {
        path,
        force: force.unwrap_or(false),
    };
    let write = db
        .write(move |ws| {
            let mount = mount_conn(ws)?;
            let Some(mp) = load_mount_service_info(mount, &id)? else {
                return Ok(None);
            };
            let main = ws.main().connection();
            // The Config arm carries v4's thrown message; ride it out of the
            // closure as data so the route 500 body matches byte-for-byte.
            match enqueue_embedding_jobs_scoped(main, mount, &mp, &opts) {
                Ok(out) => Ok(Some((mp.name, Ok(out)))),
                Err(ScopedEnqueueError::Config(m)) => Ok(Some((mp.name, Err(m)))),
                Err(ScopedEnqueueError::Db(e)) => Err(e),
            }
        })
        .await;
    match write {
        Ok(Some((mount_name, Ok((jobs, queued, skipped))))) => {
            if queued > 0 {
                crate::services::queue_service::ensure_processor_running();
            }
            Response::MountFile(serde_json::json!({
                "mountPointId": mount_point_id,
                "mountName": mount_name,
                "jobs": jobs,
                "queued": queued,
                "skipped": skipped,
            }))
        }
        Ok(Some((_, Err(config_msg)))) => {
            Response::error(ErrorKind::Internal, format!("Embed failed: {config_msg}"))
        }
        Ok(None) => Response::error(ErrorKind::NotFound, "Mount point not found"),
        Err(e) => Response::error(ErrorKind::Internal, format!("Embed failed: {e}")),
    }
}

/// The JS `typeof`-style name for a zod type-mismatch message.
fn zod_type_name(v: Option<&serde_json::Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(serde_json::Value::Null) => "null",
        Some(serde_json::Value::Bool(_)) => "boolean",
        Some(serde_json::Value::Number(_)) => "number",
        Some(serde_json::Value::String(_)) => "string",
        Some(serde_json::Value::Array(_)) => "array",
        Some(serde_json::Value::Object(_)) => "object",
    }
}

/// v4 collection-route `?action=semantic-search` (`handleSemanticSearch`) —
/// embed the query through the user's default profile, then
/// `searchDocumentChunks` with `includeBlocked: true` (the operator sees every
/// document). Body: `{results, count, query, embeddingModel,
/// embeddingDimensions}`.
pub async fn mount_semantic_search<P: crate::model::embedding::EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    user_id: &str,
    body: serde_json::Value,
) -> Response {
    use crate::services::knowledge_injector::document_search::{
        search_document_chunks, DocumentSearchOptions,
    };

    // v4 `semanticSearchSchema` — first-issue message semantics (zod v4 text).
    let query = match body.get("query") {
        Some(serde_json::Value::String(s)) if !s.is_empty() => s.clone(),
        Some(serde_json::Value::String(_)) => {
            return Response::error(ErrorKind::BadRequest, "Query is required")
        }
        other => {
            return Response::error(
                ErrorKind::BadRequest,
                format!(
                    "Invalid input: expected string, received {}",
                    zod_type_name(other)
                ),
            )
        }
    };
    let mount_point_ids: Option<Vec<String>> = match body.get("mountPointIds") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => match serde_json::from_value::<Vec<String>>(v.clone()) {
            Ok(ids) => Some(ids),
            Err(_) => {
                return Response::error(
                    ErrorKind::BadRequest,
                    format!(
                        "Invalid input: expected array, received {}",
                        zod_type_name(Some(v))
                    ),
                )
            }
        },
    };
    let project_id = body
        .get("projectId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let path_prefix = body
        .get("pathPrefix")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let top = body.get("top").and_then(|v| v.as_i64()).unwrap_or(20);
    let threshold = body
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);

    let embedding = match provider
        .generate_embedding_for_user(
            &query,
            user_id,
            None,
            crate::model::embedding::EmbeddingPriority::Interactive,
        )
        .await
    {
        Ok(e) => e,
        Err(e) => {
            // v4: EmbeddingError → 400 `{error, code: 'EMBEDDING_FAILED'}`.
            return Response::error_coded(ErrorKind::BadRequest, e.message, "EMBEDDING_FAILED");
        }
    };

    let opts = DocumentSearchOptions {
        project_id,
        mount_point_ids,
        limit: Some(top.max(0) as usize),
        min_score: Some(threshold),
        path_prefix,
        query: Some(query.clone()),
        apply_literal_phrase_boost: false,
        literal_boost_fraction: None,
        include_blocked: true,
    };
    let vector = embedding.embedding.clone();
    let search = db.read_mount_index(move |conn| search_document_chunks(conn, &vector, &opts));
    match search {
        Ok(results) => {
            let rows: Vec<serde_json::Value> = results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "chunkId": r.chunk_id,
                        "mountPointId": r.mount_point_id,
                        "mountPointName": r.mount_point_name,
                        "fileId": r.file_id,
                        "fileName": r.file_name,
                        "relativePath": r.relative_path,
                        "chunkIndex": r.chunk_index,
                        "headingContext": r.heading_context,
                        "content": r.content,
                        "score": r.score,
                    })
                })
                .collect();
            Response::MountFile(serde_json::json!({
                "results": rows,
                "count": rows.len(),
                "query": query,
                "embeddingModel": embedding.model,
                "embeddingDimensions": embedding.dimensions,
            }))
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.to_lowercase().contains("dimension") {
                // v4's EmbeddingDimensionMismatchError arm (the query/stored
                // dims fields are a pinned omission — recorded in the order's
                // status header; no differential case reaches this arm).
                return Response::error_coded(
                    ErrorKind::BadRequest,
                    msg,
                    "EMBEDDING_DIMENSION_MISMATCH",
                );
            }
            Response::error(ErrorKind::Internal, "Internal server error")
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
