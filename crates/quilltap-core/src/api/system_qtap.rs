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
use crate::services::quilltap_import::{
    ConflictStrategy, ImportCounts, ImportOptions, ImportResult, QuilltapExport,
};

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

/// v4 `handleExportEntities` (`route.ts:444`, `7189a968` — the switch is now
/// exhaustive over all fifteen types; the old eight-arm asymmetry is gone). A
/// type outside it answers `400 Unknown entity type: <t>`.
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
    // The host disk backend for the `files` export type's legacy disk-style
    // storage keys (`7189a968`); `mount-blob:` keys read the mount partition.
    // `None` — no storage seam — lands disk-key files in v4's own
    // warn-and-`_bytesMissing` arm.
    storage: Option<std::sync::Arc<dyn crate::services::file_storage::StorageBackend>>,
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
            storage.as_deref(),
            &uid,
            &options,
            // v4's `?action=export` route never sets `preserveIds` — only the
            // character-archive service (round 2) does.
            false,
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

/// v4 `handleImportExecute`'s JSON leg (`route.ts:717`) — the dispatch verb
/// carries `{exportData, options}` directly. NOTE v4's execute leg does NOT
/// validate the export manifest (unlike preview): a malformed `exportData`
/// reaches `executeImport`, whose own TypeError catch answers
/// `success: false`.
pub async fn import_execute(
    db: &Db,
    user_id: &str,
    export_data: &Value,
    options: &Value,
    codec: Option<std::sync::Arc<dyn crate::services::file_storage::PixelCodec>>,
) -> Response {
    // v4 `if (!exportData)` / `if (!options)` — falsy (the JSON leg can only
    // produce `null` here) → the two fixed messages.
    if export_data.is_null() {
        return Response::error(ErrorKind::BadRequest, "Missing required field: exportData");
    }
    if options.is_null() {
        return Response::error(ErrorKind::BadRequest, "Missing required field: options");
    }
    let export = QuilltapExport {
        manifest: export_data.get("manifest").cloned().unwrap_or(Value::Null),
        data: export_data.get("data").cloned().unwrap_or(Value::Null),
    };
    // JS distinguishes a MISSING `data` key (`undefined`) from an explicit
    // `null` in the TypeError wording; `Value` cannot, so carry the flag.
    let data_key_absent = export_data.get("data").is_none();
    run_import_execute(db, user_id, export, data_key_absent, options, codec).await
}

/// The shared execute tail both legs (dispatch JSON + web-edge multipart) run
/// after loading the export: v4's `conflictStrategy` validation, the
/// `'replace'` → `'overwrite'` remap (`route.ts:780`), the `importMemories ||
/// false` truthiness, then `executeImport` on the writer.
pub async fn run_import_execute(
    db: &Db,
    user_id: &str,
    export: QuilltapExport,
    data_key_absent: bool,
    options: &Value,
    // The image codec the `files` importer's storage bridges transcode with
    // (`7189a968`'s step 9); `None` falls through to the not-configured codec.
    codec: Option<std::sync::Arc<dyn crate::services::file_storage::PixelCodec>>,
) -> Response {
    let strategy = options
        .get("conflictStrategy")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mapped = match strategy {
        "skip" => ConflictStrategy::Skip,
        // Map 'replace' to 'overwrite' for the import service (legacy compat).
        "replace" | "overwrite" => ConflictStrategy::Overwrite,
        "duplicate" => ConflictStrategy::Duplicate,
        _ => {
            return Response::error(
                ErrorKind::BadRequest,
                "Invalid conflictStrategy. Must be one of: skip, replace, overwrite, duplicate",
            )
        }
    };
    let opts = ImportOptions {
        conflict_strategy: mapped,
        // v4 `includeMemories: importMemories || false` — JS truthiness, then
        // only ever used in a boolean test.
        include_memories: js_truthy(options.get("importMemories")),
        include_related_entities: false,
        selected_ids: options.get("selectedIds").cloned(),
        // v4's import WIZARD never sets `preserveIds`: silently skipping (or
        // claiming) a colliding id there is exactly the partial application the
        // refuse rule exists to prevent. Only the character-archive rehydrate
        // path (round 2) passes them.
        preserve_ids: false,
        preserve_ids_mode: crate::services::quilltap_import::PreserveIdsMode::RefuseOnCollision,
    };

    // A missing `data` key reads as `undefined` in v4, so `executeImport`'s
    // first property access throws with the `undefined` wording; the pipeline's
    // own null arm covers the explicit-`null` wording.
    if data_key_absent {
        let result = ImportResult {
            success: false,
            imported: ImportCounts::default(),
            skipped: ImportCounts::default(),
            warnings: vec![
                "Import failed: Cannot read properties of undefined (reading 'tags')".to_string(),
            ],
            imported_character_ids: Vec::new(),
        };
        return Response::System(result.to_value());
    }

    let uid = user_id.to_string();
    let out = db
        .write(move |ws| {
            let main = ws.main().connection();
            let Some(mi) = ws.mount_index() else {
                return Err(crate::db::DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ));
            };
            crate::services::quilltap_import::execute_import(
                main,
                mi.connection(),
                &uid,
                &export,
                &opts,
                codec.as_deref(),
            )
            .map_err(|e| match e {
                crate::services::quilltap_import::ImportError::Db(db_err) => db_err,
                // Unreachable in practice: `execute_import` folds every non-parse
                // failure into the success:false result (v4's one big catch); the
                // parse-side variants never arise from an already-loaded export.
                other => crate::db::DbError::WriterSpawn(other.to_string()),
            })
        })
        .await;

    match out {
        Ok(result) => Response::System(result.to_value()),
        // v4's catch → `serverError('Failed to execute import')`.
        Err(e) => {
            tracing::error!(error = %e, "Import execution failed");
            Response::error(ErrorKind::Internal, "Failed to execute import")
        }
    }
}

/// JS truthiness over a JSON value (`importMemories || false`, `if (!options)`).
pub fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
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
