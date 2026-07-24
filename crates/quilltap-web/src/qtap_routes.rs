//! P4.9G4 — the `.qtap` export/import web-edge legs. These are the parts of
//! `/api/v1/system/tools` that a dispatch `Response` cannot carry: a byte-stream
//! download and two `multipart/form-data` uploads. They hang off the EXISTING
//! `/api/v1/system/tools` route (registered by P4.9G1) — this lane adds no path
//! to `build_router`; `system_data_routes::system_tools_post` delegates here.
//!
//! - `POST ?action=export` — the `.qtap` NDJSON download, with v4's three
//!   response headers verbatim (`route.ts:422-428`).
//! - `POST ?action=import-preview` — multipart `file` (or the legacy JSON
//!   `{exportData}` leg), the format sniff, and the read-only preview.
//! - `POST ?action=import-execute` — **NOT LANDED**; answers a NAMED refusal
//!   (the write half of the import pipeline is this lane's deferral).

use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response as AxumResponse};
use serde_json::Value;

use quilltap_core::api::system_qtap;
use quilltap_core::services::quilltap_import::ndjson::load_qtap_from_upload;
use quilltap_core::services::quilltap_import::preview::preview_import;
use quilltap_core::services::quilltap_import::QuilltapExport;

use crate::multipart::FormData;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::error_to_http;

/// `POST /api/v1/system/tools?action=export` — v4 `handleExport`.
pub async fn export_download(state: &SharedState, body: &[u8]) -> AxumResponse {
    let Some(host) = state.host() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "server failed to start");
    };
    let Some(db) = host.core().db() else {
        return error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "The database is locked. Unlock it to continue.",
        );
    };
    // v4 `await req.json()` — a body that isn't JSON throws into the route's
    // catch → `serverError('Failed to create export')`.
    let Ok(parsed) = serde_json::from_slice::<Value>(body) else {
        return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create export");
    };
    let options = system_qtap::export_options_from_body(&parsed);

    match system_qtap::export_stream(
        &db,
        quilltap_core::api::engine::SINGLE_USER_ID,
        options,
        &host.core().app_version(),
    ) {
        Ok(download) => (
            StatusCode::OK,
            [
                ("content-type", download.content_type.to_string()),
                ("content-disposition", download.content_disposition),
                ("cache-control", download.cache_control.to_string()),
            ],
            download.body,
        )
            .into_response(),
        Err(resp) => match resp {
            quilltap_core::api::Response::Error(e) => error_to_http(e),
            _ => error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create export"),
        },
    }
}

// ── the import legs ─────────────────────────────────────────────────────────

/// v4's `validateExportFile` (`route.ts:637`) — the shallow manifest gate the
/// route applies BEFORE `previewImport`/`executeImport`. Distinct from
/// `validation.ts`'s throwing `validateExportFormat`: this one returns a bool and
/// the route turns a `false` into one fixed message.
fn validate_export_file(export: &QuilltapExport) -> bool {
    let Some(manifest) = export.manifest.as_object() else {
        return false;
    };
    manifest.get("format").and_then(Value::as_str) == Some("quilltap-export")
        && manifest.get("version").and_then(Value::as_str) == Some("1.0")
}

/// Load the uploaded `.qtap` for the import legs: the multipart `file` part when
/// the request is `multipart/form-data` (v4's live path — the client always holds
/// a `File`), else the JSON body's `exportData` (v4's legacy leg).
async fn load_export(
    headers: &HeaderMap,
    body: Bytes,
    missing_json_field: &str,
) -> Result<QuilltapExport, AxumResponse> {
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("multipart/form-data") {
        // The body is already buffered, so rebuild a `Request` for the shared
        // multipart parser (v4 buffers too — `await req.formData()`).
        let mut req = Request::new(Body::from(body));
        *req.headers_mut() = headers.clone();
        let form = match FormData::from_request(req, &()).await {
            Ok(f) => f,
            Err(_) => return Err(bad_request("No file provided")),
        };
        let Some(file) = form.file("file") else {
            return Err(bad_request("No file provided"));
        };
        // v4 surfaces the loader's own message through `badRequest(err.message)`.
        return load_qtap_from_upload(&file.bytes).map_err(|e| bad_request(&e));
    }

    let parsed: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let Some(export_data) = parsed.get("exportData").filter(|v| !v.is_null()) else {
        return Err(bad_request(missing_json_field));
    };
    Ok(QuilltapExport {
        manifest: export_data.get("manifest").cloned().unwrap_or(Value::Null),
        data: export_data.get("data").cloned().unwrap_or(Value::Null),
    })
}

fn bad_request(message: &str) -> AxumResponse {
    error_json(StatusCode::BAD_REQUEST, message)
}

/// `POST /api/v1/system/tools?action=import-preview` — v4 `handleImportPreview`
/// (`route.ts:655`). Read-only: it counts what an import would do and flags
/// conflicts, and writes nothing.
pub async fn import_preview(state: &SharedState, headers: &HeaderMap, body: Bytes) -> AxumResponse {
    let export = match load_export(headers, body, "Missing required field: exportData").await {
        Ok(e) => e,
        Err(resp) => return resp,
    };
    if !validate_export_file(&export) {
        return bad_request("Invalid export file format. Expected quilltap-export v1.0 format.");
    }

    let Some(host) = state.host() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "server failed to start");
    };
    let Some(db) = host.core().db() else {
        return error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "The database is locked. Unlock it to continue.",
        );
    };

    let out = db.read_main(|main| {
        db.read_mount_index(|mount| {
            preview_import(
                main,
                mount,
                quilltap_core::api::engine::SINGLE_USER_ID,
                &export,
            )
        })
    });
    match out {
        Ok(body) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            body.to_string(),
        )
            .into_response(),
        // v4's catch → `serverError('Failed to preview import')`.
        Err(_) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to preview import",
        ),
    }
}

/// `POST /api/v1/system/tools?action=import-execute`.
///
/// **NOT LANDED — the write half of the import pipeline is this lane's deferral.**
/// It answers the same NAMED refusal the dispatch verb gives, so the Import
/// dialog reports a clear "not yet available" instead of appearing to succeed.
pub fn import_execute_not_landed() -> AxumResponse {
    match system_qtap::import_not_available("Import") {
        quilltap_core::api::Response::Error(e) => error_to_http(e),
        _ => error_json(StatusCode::INTERNAL_SERVER_ERROR, "not available"),
    }
}
