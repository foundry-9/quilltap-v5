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
