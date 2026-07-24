//! P4.9G4 — the `.qtap` export/import dispatch surface. Each free function
//! returns v4's RAW route body inside the `Response::System(Value)` carrier (the
//! same shape `api::system_data` uses), so the REST edges can unwrap it to a
//! byte-identical v4 response.
//!
//! The streaming export itself is a **web-edge-only** leg (no dispatch verb — a
//! `Response` cannot carry a byte stream); [`export_stream`] is the core-side
//! producer the edge calls, returning the NDJSON body plus the three headers v4
//! sets.

use serde_json::Value;

use super::types::{ErrorKind, Response};
use crate::db::runtime::Db;
use crate::services::qtap_export::{self, ExportError, ExportOptions};
use crate::services::quilltap_import::preview::preview_import;
use crate::services::quilltap_import::QuilltapExport;

/// Run a read across BOTH partitions (main + mount-index), the idiom every
/// store-backed read in `api/` uses.
fn with_both<T>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, crate::db::DbError>,
) -> Result<T, crate::db::DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

fn db_error(e: crate::db::DbError) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

/// v4 `handleExportEntities` (`route.ts:440`). A type outside v4's eight-arm
/// switch answers `400 Unknown entity type: <t>` — including `groups` and
/// `document-stores`, which `previewExport` DOES handle (v4's own asymmetry).
pub fn export_entities(db: &Db, user_id: &str, entity_type: &str) -> Response {
    let uid = user_id.to_string();
    let t = entity_type.to_string();
    match with_both(db, move |main, mount| {
        Ok(qtap_export::export_entities(main, mount, &uid, &t))
    }) {
        Ok(Ok(body)) => Response::System(body),
        Ok(Err(ExportError::UnknownType(t))) => Response::error(
            ErrorKind::BadRequest,
            qtap_export::unknown_entity_type_message(&t),
        ),
        // v4's catch → `serverError('Failed to fetch entities')`.
        Ok(Err(_)) => Response::error(ErrorKind::Internal, "Failed to fetch entities"),
        Err(e) => db_error(e),
    }
}

/// v4 `handleExportPreview` (`route.ts:559`). The `Unknown export type` throw
/// escapes `previewExport` into the route's catch →
/// `serverError('Failed to preview export')`.
pub fn export_preview(db: &Db, user_id: &str, options: ExportOptions) -> Response {
    let uid = user_id.to_string();
    match with_both(db, move |main, mount| {
        Ok(qtap_export::preview_export(main, mount, &uid, &options))
    }) {
        Ok(Ok(body)) => Response::System(body),
        Ok(Err(_)) => Response::error(ErrorKind::Internal, "Failed to preview export"),
        Err(e) => db_error(e),
    }
}

/// The rendered `.qtap` download: the NDJSON body plus v4's three response
/// headers (`route.ts:422-428`), verbatim.
pub struct ExportDownload {
    pub body: String,
    pub content_type: &'static str,
    pub content_disposition: String,
    pub cache_control: &'static str,
}

/// v4 `handleExport` (`route.ts:396`) minus the transport: build the whole NDJSON
/// body and the headers. A writer throw becomes v4's
/// `serverError('Failed to create export')`. `app_version` is the host's crate
/// version (v4 reads `packageJson.version`), which rides the manifest.
///
/// ⚠ v4 STREAMS this (`createNdjsonStream` → a `ReadableStream` body) so a
/// multi-gigabyte export never lands in one string. v5 materializes the body
/// before writing it — see the module note in `services::qtap_export`. The bytes
/// on the wire are identical; only the memory profile differs, and the
/// per-record chunking that made streaming necessary in Node (V8's ~512 MB
/// string ceiling) has no analog in Rust.
pub fn export_stream(
    db: &Db,
    user_id: &str,
    options: ExportOptions,
    app_version: &str,
) -> Result<ExportDownload, Response> {
    let uid = user_id.to_string();
    let now_iso = crate::clock::now_iso();
    let app_version = app_version.to_string();
    let entity_type = options.entity_type.clone();
    let created_at = now_iso.clone();

    let records = match with_both(db, move |main, mount| {
        Ok(qtap_export::stream_export_records(
            main,
            mount,
            &uid,
            &options,
            &created_at,
            &app_version,
        ))
    }) {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => {
            return Err(Response::error(
                ErrorKind::Internal,
                "Failed to create export",
            ))
        }
        Err(e) => return Err(db_error(e)),
    };

    let filename = qtap_export::export_filename(&entity_type, &now_iso);
    Ok(ExportDownload {
        body: qtap_export::records_to_ndjson(&records),
        content_type: qtap_export::QTAP_NDJSON_CONTENT_TYPE,
        content_disposition: format!("attachment; filename=\"{filename}\""),
        cache_control: "no-store",
    })
}

/// Parse the `?action=export` POST body into [`ExportOptions`] with v4's
/// coercions (`scope || 'all'`, `includeMemories || false`).
pub fn export_options_from_body(body: &Value) -> ExportOptions {
    ExportOptions {
        entity_type: body
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        scope: body
            .get("scope")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("all")
            .to_string(),
        selected_ids: body
            .get("selectedIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        include_memories: body
            .get("includeMemories")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// v4 `handleImportPreview`'s JSON leg (`route.ts:685-694`) — the dispatch verb
/// carries `{exportData}` directly. The shallow `validateExportFile` gate runs
/// first (v4 :696) with its one fixed message; `previewImport` then counts and
/// flags conflicts without writing.
pub fn import_preview(db: &Db, user_id: &str, export_data: &Value) -> Response {
    let export = QuilltapExport {
        manifest: export_data.get("manifest").cloned().unwrap_or(Value::Null),
        data: export_data.get("data").cloned().unwrap_or(Value::Null),
    };
    let manifest_ok = export
        .manifest
        .as_object()
        .map(|m| {
            m.get("format").and_then(Value::as_str) == Some("quilltap-export")
                && m.get("version").and_then(Value::as_str) == Some("1.0")
        })
        .unwrap_or(false);
    if !manifest_ok {
        return Response::error(
            ErrorKind::BadRequest,
            "Invalid export file format. Expected quilltap-export v1.0 format.",
        );
    }
    let uid = user_id.to_string();
    match with_both(db, move |main, mount| {
        preview_import(main, mount, &uid, &export)
    }) {
        Ok(body) => Response::System(body),
        // v4's catch → `serverError('Failed to preview import')`.
        Err(_) => Response::error(ErrorKind::Internal, "Failed to preview import"),
    }
}

/// The `not yet available` body the unlanded import verbs answer with, so a
/// client gets a named refusal rather than a silent no-op.
pub fn import_not_available(what: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!(
            "{what} is recognized but not yet available (the P4.9G4 import pipeline \
             is not landed; export works)."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn body_coercions_match_v4() {
        let o = export_options_from_body(&json!({"type":"tags"}));
        assert_eq!(o.scope, "all");
        assert!(!o.include_memories);
        assert!(o.selected_ids.is_empty());

        let o = export_options_from_body(
            &json!({"type":"chats","scope":"selected","selectedIds":["a"],"includeMemories":true}),
        );
        assert_eq!(o.scope, "selected");
        assert_eq!(o.selected_ids, vec!["a".to_string()]);
        assert!(o.include_memories);
    }
}
