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

use crate::files_routes::error_json;
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
