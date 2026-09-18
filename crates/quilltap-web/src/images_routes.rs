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
//! ## The JSON body ceiling (P4.76, the P4.73 review's item (d))
//!
//! Both JSON legs read the body with `usize::MAX` rather than axum's 2 MB
//! default, and that is v4's posture MEASURED rather than assumed: v4's
//! request-path ceiling is `next.config.js:66`'s
//! `proxyClientMaxBodySize: '10gb'` (the `bodySizeLimit: '100mb'` two lines
//! above governs Server Actions, not route handlers — dogfood findings #36/#63
//! settled that distinction, and `files_write_routes`'s
//! `import_body_over_the_old_100mb_ceiling_reaches_the_handler` is its pin).
//! Ten gigabytes is not `usize::MAX`, so the two are not identical — but no
//! payload this route can carry (a URL, a prompt, a tag list) comes within
//! nine orders of magnitude of either, and the alternative — a v5-invented 413
//! at 2 MB where v4 answers 200 — is the divergence that would actually be
//! observable. RECORDED, not silently inherited.
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
        return images_generate(state, req).await;
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

/// The `?action=generate` leg (P4.76) — v4 `handleGenerateImage`
/// (`route.ts:177-408`), wrapped by v4 in `trackActivity('image', …)`.
///
/// ⚠ There is NO content-type gate on this leg. v4 goes straight to
/// `await request.json()`, so a multipart or text body reaching `?action=generate`
/// throws a SyntaxError, not the upload leg's `Invalid content type` — the flat
/// 500 the middleware's final arm answers. The upload/import fall-through's
/// content-type dispatch is reached only when the action is NOT `generate`.
///
/// The five body keys cross RAW: v4 Zod-parses them in the handler, so the
/// refusals answer identical bytes on this transport and on Tauri IPC.
///
/// **P4.98 — ONE decoder for every transport.** This edge used to hand-build
/// the variant from `map.get(..).cloned()`, which is a SECOND spelling of the
/// absent / explicit-`null` / value tri-state, and the two spellings had
/// disagreed since P4.76: the hand-build preserved a `null` (so this edge was
/// v4-faithful) while dispatch's plain `Option<Value>` collapsed it to ABSENT
/// (so `{"chatId": null}` generated an image v4 refuses). Rather than
/// re-mirror `double_option` by hand — which is how the class recurs — the
/// five keys are lifted into a dispatch-shaped envelope and handed to the
/// SAME `serde_json::from_value::<CoreRequest>` decode the dispatch route
/// runs, so the tri-state has exactly one implementation and the two
/// transports cannot drift apart again. Pinned by
/// `images_edge_and_dispatch_decode_the_five_keys_identically` below.
///
/// **P4.102 generalized this envelope into `crate::request_envelope::
/// request_envelope`** — the shared home for every REST edge whose `Request`
/// variant carries `Option<Option<Value>>` fields, retiring the hand-rolled
/// `tri()` helpers in `subprompts_routes.rs` and `prompt_templates_routes.rs`
/// onto the same code this edge already ran. `images_generate_request` below
/// is now a one-line call onto it; the class is held shut by
/// `tri_state_edges_share_the_decoder.rs`'s census guard.
///
/// Unknown keys are dropped on the way in, which is v4's own behaviour: its
/// `generateImageSchema` is a `z.object`, and a `z.object` STRIPS undeclared
/// keys rather than refusing them.
async fn images_generate(state: SharedState, req: axum::extract::Request) -> AxumResponse {
    let body = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(_) => return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
    };
    let parsed = match serde_json::from_slice::<Value>(&body) {
        Ok(v) => v,
        Err(_) => return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
    };
    let request = match images_generate_request(&parsed) {
        Some(r) => r,
        // Unreachable while the five stay raw `Option<Option<Value>>`: every
        // JSON shape decodes. A failure here means one has been re-typed, at
        // which point the refusal belongs to the DECODE again and this edge
        // would be answering serde's sentence where v4 answers Zod's — the
        // divergence `dispatch_wrong_type_census`'s `IMAGES_GENERATE_RAW_FIVE`
        // and `images_generate_dispatch_wire.rs` exist to keep impossible.
        None => return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
    };
    match dispatch_core(&state, request).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::CREATED),
        Err(r) => r,
    }
}

/// Lift v4's five `generateImageSchema` keys out of a parsed request body and
/// decode them through the dispatch envelope — see `images_generate`.
///
/// **P4.102** generalized this into `crate::request_envelope::request_envelope`,
/// the ONE helper every tri-state REST edge now shares (`subprompts_routes.rs`,
/// `prompt_templates_routes.rs`); this is a one-line call onto it. The pin
/// below (`images_edge_and_dispatch_decode_the_five_keys_identically`) is the
/// proof the generalization moved nothing: it stays green unchanged.
fn images_generate_request(parsed: &Value) -> Option<CoreRequest> {
    const KEYS: [&str; 5] = ["prompt", "profileId", "chatId", "tags", "options"];
    crate::request_envelope::request_envelope("imagesGenerate", parsed, &KEYS, &[])
}

#[cfg(test)]
mod images_generate_decoder_tests {
    use super::*;
    use serde_json::json;

    /// P4.98 item 3 — the edge and dispatch must decode v4's five body keys to
    /// the SAME `Request`, for all three states of the tri-state.
    ///
    /// This is the assertion the shared-decoder rewrite exists to make
    /// trivial: `images_generate_request` builds a dispatch envelope and runs
    /// the dispatch decode, so the two sides are the same code and the test
    /// is a regression pin rather than a coincidence check. Re-introduce a
    /// hand-build here — `map.get(k).cloned()`, or a `lift` that maps `null`
    /// to `Some(Some(Null))` where `double_option` yields `Some(None)` — and
    /// this reddens on the `null` rows (M3).
    ///
    /// `Request` already derives `PartialEq`, so the comparison is direct; no
    /// derive was added for it.
    #[test]
    fn images_edge_and_dispatch_decode_the_five_keys_identically() {
        for key in ["prompt", "profileId", "chatId", "tags", "options"] {
            for state in [
                None,                              // ABSENT
                Some(Value::Null),                 // explicit null
                Some(json!("x")),                  // a value
                Some(json!({ "nested": [1, 2] })), // a structured value
            ] {
                let mut body = serde_json::Map::new();
                if let Some(v) = state.clone() {
                    body.insert(key.to_string(), v);
                }
                let edge = images_generate_request(&Value::Object(body.clone()))
                    .unwrap_or_else(|| panic!("the edge failed to decode {key}={state:?}"));

                // What dispatch would decode from the very same keys.
                let mut envelope = body;
                envelope.insert("type".into(), Value::String("imagesGenerate".into()));
                let wire = serde_json::to_vec(&Value::Object(envelope)).unwrap();
                let dispatched = serde_json::from_slice::<CoreRequest>(&wire)
                    .unwrap_or_else(|e| panic!("dispatch failed to decode {key}={state:?}: {e}"));

                assert_eq!(
                    edge, dispatched,
                    "the REST edge and the dispatch decode disagree about \
                     `{key}` = {state:?} — the tri-state has two spellings again"
                );
            }
        }
    }

    /// The three states must stay DISTINGUISHABLE after the decode: that is
    /// the whole property, and a decoder that mapped two of them together
    /// would satisfy the equality test above while losing the evidence v4's
    /// `.optional()` refusal is built from.
    #[test]
    fn absent_null_and_value_stay_three_distinct_requests() {
        let of = |body: Value| images_generate_request(&body).expect("decodes");
        let absent = of(json!({ "prompt": "p" }));
        let null = of(json!({ "prompt": "p", "chatId": null }));
        let value = of(json!({ "prompt": "p", "chatId": "c" }));
        assert_ne!(
            absent, null,
            "an absent `chatId` and an explicit `null` collapsed together — the \
             P4.98 defect, back again"
        );
        assert_ne!(
            absent, value,
            "an absent `chatId` and a value collapsed together"
        );
        assert_ne!(
            null, value,
            "an explicit `null` and a value collapsed together"
        );
    }

    /// v4's `generateImageSchema` is a `z.object`, which STRIPS undeclared
    /// keys; and a body that parses to a non-object still reaches that parse,
    /// so it folds to all-absent and the HANDLER refuses it (the arm
    /// `images_edge_routes.rs` pins at the wire).
    #[test]
    fn unknown_keys_are_stripped_and_a_non_object_folds_to_absent() {
        assert_eq!(
            images_generate_request(&json!({ "prompt": "p", "notAKey": 1, "type": "bogus" })),
            images_generate_request(&json!({ "prompt": "p" })),
        );
        for non_object in [json!([1, 2, 3]), json!("nope"), json!(7), Value::Null] {
            assert_eq!(
                images_generate_request(&non_object),
                images_generate_request(&json!({})),
                "a non-object body must fold to all-absent: {non_object}"
            );
        }
    }
}
