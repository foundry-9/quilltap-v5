//! The `/api/v1/images` COLLECTION REST edges (P4.73) — v4
//! `app/api/v1/images/route.ts` plus the `[id]` DELETE arm.
//!
//! ## The POST is v4's FIRST dispatch shape, not the envelope shape
//!
//! `route.ts:161-171` reads `getActionParam(request)` and runs the generate leg
//! only on the literal `'generate'`. There is no `withActionDispatch`, so there
//! is no `Unknown action: …` envelope and no `Action parameter required`
//! refusal: **every other value — an unknown action, `?action=` (empty), and no
//! `action` key at all — falls through to the upload/import leg.**
//! `?action=bogus` uploads. That is why this module reads the action through
//! [`crate::query::action`] (v4's own `''`-is-falsy fold) and then compares to
//! the one literal, rather than reaching for `unknown_action_response`.
//!
//! Registered in `query_param_semantics_equivalence`'s `ENDPOINTS` as
//! `images_collection_post`, whose `unknown` / `empty` probes are exactly this
//! fall-through.
//!
//! ## `?tagId=`
//!
//! v4 `searchParams.get('tagId')` (FIRST-wins) then `if (tagId)` — JS-falsy, so
//! `?tagId=` is the same as absent. The fold happens here so the verb's field
//! carries only a meaningful value.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};

use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::multipart::FormData;

/// Base64 for the dispatch boundary. `files_routes` has an identical private
/// helper; §G grants this lane only that file's `tags` block, so the two-liner
/// is repeated here rather than widening a neighbour's visibility.
fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap the images family's envelope. Every verb in the family answers
/// [`CoreResponse::Images`] with v4's literal body, so the edge only chooses the
/// success status.
///
/// ⚠ A variant missing from this match answers 500 `Unexpected core response`
/// on every SUCCESS — the defect `text_replacements_routes.rs:70-84` records
/// against `BrahmaConsole`. Every images verb must land in the arm below.
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::Images(v) => (
            success_status,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        // v4's `badRequest(message, details)` puts BOTH keys on the wire
        // (`responses.ts:errorResponse` appends `details` whenever it is not
        // undefined). The shared `error_to_http` renders only `{error}`, so the
        // details-bearing refusal — the images DELETE's `Image is in use` — is
        // rendered here rather than by widening a helper this lane does not own.
        CoreResponse::Error(e) => match e.validation_wire_body() {
            Some(body) => (
                StatusCode::BAD_REQUEST,
                [("content-type", "application/json")],
                body.to_string(),
            )
                .into_response(),
            None => error_to_http(e),
        },
        _ => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
    }
}

/// v4 `GET /api/v1/images` (`route.ts:69-153`) — the tagged image list.
pub async fn images_list(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // v4 `searchParams.get('tagId')` then `if (tagId)` — an empty value is
    // JS-falsy, so it never filters.
    let tag_id = crate::query::first(&pairs, "tagId")
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    match dispatch_core(&state, CoreRequest::ImagesList { tag_id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

/// v4 `DELETE /api/v1/images/{id}` (`[id]/route.ts:134-237`). Replaces the
/// P4.9a2 named refusal — see the §F note in the commit message.
pub async fn image_delete(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::ImageDelete { id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

/// v4 `POST /api/v1/images` (`route.ts:159-171` + `handleUploadOrImport`).
///
/// ⚠ The action read is v4's FIRST dispatch shape, not the envelope shape:
/// only the literal `'generate'` takes the generate leg, and EVERY other value
/// — unknown, `?action=` (empty, JS-falsy), or no key at all — falls through to
/// upload/import. There is no `Unknown action` envelope on this route.
///
/// The fall-through then dispatches on the request's own `content-type`:
/// `application/json` → import-from-URL, `multipart/form-data` → upload,
/// anything else → `badRequest('Invalid content type')`.
pub async fn images_post(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
    req: axum::extract::Request,
) -> AxumResponse {
    // `query::action` folds `?action=` into the no-action leg exactly as v4's
    // JS truthiness does; on this route BOTH land in the same place.
    if crate::query::action(&pairs) == Some("generate") {
        return images_generate_not_available();
    }

    let content_type = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // v4 `contentType.includes('application/json')` — a substring test, so a
    // charset parameter still matches.
    if content_type.contains("application/json") {
        // v4 `await request.json()` throwing (an unreadable or unparseable
        // body) is a SyntaxError, not a ZodError — `handleRouteError`'s final
        // arm (`context.ts:206-207`): a flat 500 `Internal server error`. Only
        // a body that PARSES reaches the Zod refusal (the §3 review of the
        // follow-ups round 2 caught both arms answering a 400 v4 never does).
        let body = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
            Ok(b) => b,
            Err(_) => {
                return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };
        // The body is decoded THROUGH the Request enum and validated in the
        // HANDLER (`api::images::parse_import_body`), so the `z.url()` and
        // tags-schema refusals answer identical bytes on this transport and on
        // Tauri IPC — one place, not two (the `ChatCreate` trio's lesson).
        let (url, tags) = match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Object(map)) => (map.get("url").cloned(), map.get("tags").cloned()),
            // A body that is not even an object still reaches v4's Zod parse,
            // which refuses it — the handler answers that, not the edge.
            Ok(_) => (None, None),
            Err(_) => {
                return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };
        let core_req = CoreRequest::ImageImportFromUrl { url, tags };
        return match dispatch_core(&state, core_req).await {
            Ok(resp) => unwrap_to_http(resp, StatusCode::CREATED),
            Err(r) => r,
        };
    }

    if content_type.contains("multipart/form-data") {
        let form = match FormData::from_request(req, &state).await {
            Ok(f) => f,
            // v4 `await request.formData()` throwing is the same unhandled-error
            // 500 as the JSON leg's — never a v5-invented 400 sentence.
            Err(_) => {
                return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };
        // v4 `if (!file) return badRequest('No file provided')`.
        let Some(file) = form.file("file") else {
            return error_json(StatusCode::BAD_REQUEST, "No file provided");
        };
        // v4 reads `tags` as a RAW JSON string with NO schema: unparseable is
        // `badRequest('Invalid tags JSON')`, and whatever it parses to is
        // `.map`ped for `tagId` — so `[{"tagId": 5}]` carries the number 5.
        // A FALSY parse (`null`, `0`, `false`) skips the map entirely, and a
        // TRUTHY non-array throws `.map is not a function` into the outer
        // catch, which on this route is the middleware's 500.
        let tags: Option<Vec<Value>> = match form.text("tags").filter(|s| !s.is_empty()) {
            Some(raw) => match serde_json::from_str::<Value>(&raw) {
                Err(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid tags JSON"),
                Ok(v) if !quilltap_core::api::system_qtap::js_truthy(Some(&v)) => None,
                Ok(Value::Array(arr)) => Some(arr),
                Ok(_) => {
                    return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
                }
            },
            None => None,
        };
        let core_req = CoreRequest::ImageUpload {
            filename: file.filename.clone().unwrap_or_default(),
            content_type: file.content_type.clone().unwrap_or_default(),
            data: base64_of(&file.bytes),
            tags,
        };
        return match dispatch_core(&state, core_req).await {
            Ok(resp) => unwrap_to_http(resp, StatusCode::CREATED),
            Err(r) => r,
        };
    }

    // v4's final `return badRequest('Invalid content type')`.
    error_json(StatusCode::BAD_REQUEST, "Invalid content type")
}

/// The `?action=generate` leg, pending its own unit. A NAMED refusal, never a
/// silent fall-through to upload: v4 runs a whole synchronous generate here,
/// and answering `Invalid content type` (what the upload leg would say to a
/// JSON generate body) would be a lie about what the server does.
fn images_generate_not_available() -> AxumResponse {
    error_json(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Generating an image through POST /api/v1/images?action=generate is recognized but not \
         yet available (v4's route-level handleGenerateImage — its own Concierge gate, reroute \
         rule and Lantern write — is the next P4.73 unit).",
    )
}
