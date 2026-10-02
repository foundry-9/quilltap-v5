//! `GET /api/v1/chats/{id}/qtap-target` — stream a `qtap://` target's raw bytes
//! (P4.6w). Ports v4 `app/api/v1/chats/[id]/qtap-target/route.ts`: resolve the
//! `{filePath, scope, mountPoint}` through the SAME chat access rules as the
//! Salon's Document Mode (the operator override), then stream the bytes. The
//! global qtap image viewer uses it on non-Salon surfaces.
//!
//! A binary payload is a real URL (D4 — never enum dispatch), so this is a
//! dedicated byte route alongside `files_routes`. It reuses the proven
//! `documents::resolve_operator_doc_path` (the operator override) + the same
//! database-mount byte read as `mount_file_get` (fs mounts are the standing
//! FsSeam; the resolver never yields one).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use serde_json::{json, Value};

use quilltap_core::db::doc_mount_blobs::DocMountBlobsRepository;
use quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository;
use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::{chats_read, DbError};
use quilltap_core::doc_edit::path_resolver::ResolveError;
use quilltap_core::doc_edit::DocEditScope;
use quilltap_core::documents::{self, mime_for_extension, DocumentAccessContext};

use crate::files_routes::db_and_backend;
use crate::state::SharedState;

/// The resolved qtap-target read outcome.
enum Outcome {
    Ok { bytes: Vec<u8>, mime: String },
    ChatMissing,
    NotFound,
    ServerError,
}

fn error_json(status: StatusCode, message: &str) -> AxumResponse {
    (status, axum::Json(json!({ "error": message }))).into_response()
}

fn scope_from_str(s: &str) -> DocEditScope {
    match s {
        "document_store" => DocEditScope::DocumentStore,
        "general" => DocEditScope::General,
        _ => DocEditScope::Project,
    }
}

pub async fn qtap_target_get(
    State(state): State<SharedState>,
    Path(chat_id): Path<String>,
    Query(query): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    let (db, _backend) = match db_and_backend(&state) {
        Ok(v) => v,
        Err(resp) => return *resp,
    };

    // v4 `querySchema`: filePath (min 1), scope enum default 'project', mountPoint?.
    let Some(file_path) = crate::query::first(&query, "filePath")
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return error_json(
            StatusCode::BAD_REQUEST,
            "Invalid query: filePath is required",
        );
    };
    let scope = scope_from_str(crate::query::first(&query, "scope").unwrap_or("project"));
    let mount_point = crate::query::first(&query, "mountPoint").map(str::to_string);

    let db = &db;
    let outcome: Result<Outcome, DbError> = db.read_main(move |main| {
        db.read_mount_index(move |mount| {
            let Some(chat) = chats_read::find_by_id(main, &chat_id)? else {
                return Ok(Outcome::ChatMissing);
            };
            let ctx = access_context(&chat);
            let resolved = match documents::resolve_operator_doc_path(
                main,
                mount,
                &ctx,
                scope,
                &file_path,
                mount_point.as_deref(),
                None,
            ) {
                Ok(r) => r,
                // v4's catch maps ENOENT/SOURCE_NOT_FOUND → 404, else 500. A
                // NOT_FOUND path error → 404; the host-fs seam / other → 500.
                Err(ResolveError::Path {
                    code: quilltap_core::doc_edit::path_resolver::PathErrorCode::NotFound,
                    ..
                }) => return Ok(Outcome::NotFound),
                Err(_) => return Ok(Outcome::ServerError),
            };
            // Database-mount byte read (fs = the FsSeam; resolver never yields it).
            let Some(mp_id) = resolved.mount_point_id.as_deref() else {
                return Ok(Outcome::ServerError);
            };
            read_resolved_target(mount, mp_id, &resolved.relative_path)
        })
    });

    match outcome {
        Ok(Outcome::Ok { bytes, mime }) => (
            StatusCode::OK,
            [
                ("content-type", mime),
                ("content-length", bytes.len().to_string()),
                ("cache-control", "private, max-age=3600".to_string()),
            ],
            bytes,
        )
            .into_response(),
        Ok(Outcome::ChatMissing) => error_json(StatusCode::BAD_REQUEST, "Chat not found"),
        Ok(Outcome::NotFound) => error_json(StatusCode::NOT_FOUND, "File not found"),
        Ok(Outcome::ServerError) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to stream qtap target",
        ),
        Err(_) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to stream qtap target",
        ),
    }
}

/// The database-mount byte read for a resolved target: the link row picks
/// documents (text) vs blobs. The link and document reads are v4's FALLBACKS —
/// a failed read is the not-found arm, never the 500 (P4.142, G1; a named fn so
/// the fallback can be pinned without a web state).
fn read_resolved_target(
    mount: &rusqlite::Connection,
    mp_id: &str,
    rel: &str,
) -> Result<Outcome, DbError> {
    let links = DocMountFileLinksRepository::new(mount);
    // v4's fallback reads — a failed read is the not-found arm (P4.142).
    let Some(link) = links.find_by_mount_point_and_path_or_none(mp_id, rel) else {
        return Ok(Outcome::NotFound);
    };
    let is_text = matches!(
        link.file_type.as_str(),
        "markdown" | "txt" | "json" | "jsonl"
    );
    if is_text {
        let docs = DocMountDocumentsRepository::new(mount);
        let Some(content) = docs.find_by_mount_point_and_path_or_none(mp_id, rel) else {
            return Ok(Outcome::NotFound);
        };
        return Ok(Outcome::Ok {
            bytes: content.into_bytes(),
            mime: mime_for_extension(rel).to_string(),
        });
    }
    let blobs = DocMountBlobsRepository::new(mount);
    let Some(bytes) = blobs.read_data_by_file_id(&link.file_id)? else {
        return Ok(Outcome::NotFound);
    };
    let mime = match blobs.find_by_mount_point_and_path(mp_id, rel)? {
        Some(meta) => meta.stored_mime_type,
        None => mime_for_extension(rel).to_string(),
    };
    Ok(Outcome::Ok { bytes, mime })
}

/// v4 `getProjectId` + `getParticipantCharacterIds` — the chat's access context.
fn access_context(chat: &Value) -> DocumentAccessContext {
    let project_id = chat
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut seen = std::collections::HashSet::new();
    let mut character_ids = Vec::new();
    if let Some(parts) = chat.get("participants").and_then(Value::as_array) {
        for p in parts {
            if let Some(cid) = p.get("characterId").and_then(Value::as_str) {
                if seen.insert(cid.to_string()) {
                    character_ids.push(cid.to_string());
                }
            }
        }
    }
    DocumentAccessContext {
        project_id,
        character_ids,
        // P4.6bg S2: the operator qtap-target byte route is database-only today;
        // `None` preserves the FsSeam refusal (no general-scope byte target).
        files_dir: None,
    }
}

#[cfg(test)]
mod fallback_read_tests {
    use super::{read_resolved_target, Outcome};
    use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// P4.142 (G1, no family drives this route — a capture unit): the resolved
    /// target's link and document reads are v4's FALLBACKS, so a broken links or
    /// documents table answers the not-found arm (404 `File not found`) with
    /// v4's repository line, never `Failed to stream qtap target`. The silence
    /// leg first.
    #[test]
    fn the_target_read_falls_back_to_not_found() {
        let dir = tempfile::tempdir().unwrap();
        quilltap_core::services::provisioning::provision_fresh_instance(dir.path(), PEPPER)
            .unwrap();
        let w = quilltap_core::db::Writer::open_writable(
            &dir.path().join("quilltap-mount-index.db"),
            PEPPER,
        )
        .unwrap();
        let mp: String = w
            .connection()
            .query_row(
                "SELECT id FROM doc_mount_points WHERE mountType = 'database' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        DocMountFileLinksRepository::new(w.connection())
            .write_database_document(&mp, "notes/a.md", "hello")
            .unwrap();
        let read = || {
            quilltap_core::test_support::captured_with(|| {
                read_resolved_target(w.connection(), &mp, "notes/a.md")
            })
        };
        let (got, lines) = read();
        assert!(matches!(got, Ok(Outcome::Ok { ref bytes, .. }) if bytes == b"hello"));
        assert!(lines.is_empty(), "{lines:?}");

        w.connection()
            .execute_batch(
                "ALTER TABLE \"doc_mount_documents\" RENAME COLUMN \"content\" TO \"content_x\"",
            )
            .unwrap();
        let (got, lines) = read();
        assert!(matches!(got, Ok(Outcome::NotFound)));
        assert_eq!(
            lines,
            vec![format!("ERROR quilltap::db Error finding document by mount point and path collection=doc_mount_documents mountPointId={mp} relativePath=notes/a.md error=no such column: d.content")]
        );

        w.connection()
            .execute_batch("ALTER TABLE \"doc_mount_file_links\" RENAME COLUMN \"originalMimeType\" TO \"originalMimeType_x\"")
            .unwrap();
        let (got, lines) = read();
        assert!(matches!(got, Ok(Outcome::NotFound)));
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error querying joined file links collection=doc_mount_file_links whereClause=WHERE l.mountPointId = ? AND LOWER(l.relativePath) = LOWER(?) error=no such column: l.originalMimeType".to_string()]
        );
    }
}
