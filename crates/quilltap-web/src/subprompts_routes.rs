//! P4.D163 REST edges: the five character-subprompt endpoints (v4 `2f4254b42`,
//! `app/api/v1/characters/[id]/subprompts/route.ts` +
//! `[subpromptId]/route.ts`), v4-URL faithful. Each edge dispatches the
//! corresponding `Request` and UNWRAPS the dispatch envelope to v4's RAW route
//! body (the P4.6ah lesson — a v4-shaped client reads the raw body, not the
//! tagged `Response`). The JSON verbs also ride `POST /api/dispatch`; these
//! edges give v4-URL parity and own the ONE thing the flat dispatch variant
//! cannot express — a body that is not an object at all.
//!
//! - `GET    /api/v1/characters/{id}/subprompts`                → `{ subprompts }`
//! - `POST   /api/v1/characters/{id}/subprompts`                → `{ subprompt }` (201)
//! - `GET    /api/v1/characters/{id}/subprompts/{subpromptId}`  → `{ subprompt }`
//! - `PUT    /api/v1/characters/{id}/subprompts/{subpromptId}`  → `{ subprompt }`
//! - `DELETE /api/v1/characters/{id}/subprompts/{subpromptId}`  → `{ success: true }`
//!
//! The guard ladders are the HANDLER's (`quilltap_core::api::subprompts`),
//! measured on v4's real routes by `subprompts_routes_equivalence`: POST/PUT
//! parse the body BEFORE the character lookup, GET/DELETE validate the id
//! first. This file only chooses the success status and renders the Zod
//! `{error: 'Validation error', details}` envelope the shared `error_to_http`
//! cannot (the `images_routes` precedent).
//!
//! ## The non-object body
//!
//! v4's `createSubpromptSchema.parse(body)` over `await request.json()` — a
//! body of `null` answers the middleware's `validationError` with ONE issue,
//! `invalid_type expected object received null` at path `[]` (measured:
//! `create_zod_body_null_400`). The flat dispatch variant carries `title` /
//! `content` and cannot say "the body itself was null", so that arm is
//! rendered HERE, byte-for-byte, before dispatch; every other refusal is the
//! handler's. A body that fails to parse as JSON at all is read as `{}` (the
//! `help_routes` precedent — v4's `request.json()` SyntaxError arm is not in
//! the corpus and is recorded as unmeasured).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::subprompts::body_not_object_details;
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::{json, Value};

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap a `Character` body to the raw route shape; the Zod envelope keeps
/// its `details`.
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::Character(v) => (
            success_status,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
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

fn parse_body(body: &axum::body::Bytes) -> Value {
    serde_json::from_slice::<Value>(body).unwrap_or_else(|_| Value::Object(Default::default()))
}

/// v4's `schema.parse(body)` on a non-object body — rendered here (see the
/// module doc) with the middleware's exact envelope.
fn non_object_refusal(body: &Value) -> Option<AxumResponse> {
    if body.is_object() {
        return None;
    }
    Some(
        (
            StatusCode::BAD_REQUEST,
            [("content-type", "application/json")],
            json!({ "error": "Validation error", "details": body_not_object_details(body) })
                .to_string(),
        )
            .into_response(),
    )
}

/// The flat variant's tri-state from a parsed object body: absent → `None`,
/// present-null → `Some(None)`, present → `Some(Some(v))` (the `double_option`
/// shape the dispatch decoder produces for the same wire).
fn tri(body: &Value, key: &str) -> Option<Option<Value>> {
    body.get(key)
        .map(|v| if v.is_null() { None } else { Some(v.clone()) })
}

async fn dispatch(state: &SharedState, req: CoreRequest, ok: StatusCode) -> AxumResponse {
    match dispatch_core(state, req).await {
        Ok(resp) => unwrap_to_http(resp, ok),
        Err(r) => r,
    }
}

// ===========================================================================
// GET / POST /api/v1/characters/{id}/subprompts
// ===========================================================================

pub async fn subprompts_collection_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    dispatch(
        &state,
        CoreRequest::CharacterSubpromptList { character_id: id },
        StatusCode::OK,
    )
    .await
}

pub async fn subprompts_collection_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let parsed = parse_body(&body);
    if let Some(refusal) = non_object_refusal(&parsed) {
        return refusal;
    }
    let req = CoreRequest::CharacterSubpromptCreate {
        character_id: id,
        title: tri(&parsed, "title"),
        content: tri(&parsed, "content"),
    };
    // v4 `created(...)` → 201.
    dispatch(&state, req, StatusCode::CREATED).await
}

// ===========================================================================
// GET / PUT / DELETE /api/v1/characters/{id}/subprompts/{subpromptId}
// ===========================================================================

pub async fn subprompt_item_get(
    State(state): State<SharedState>,
    Path((id, subprompt_id)): Path<(String, String)>,
) -> AxumResponse {
    dispatch(
        &state,
        CoreRequest::CharacterSubpromptGet {
            character_id: id,
            subprompt_id,
        },
        StatusCode::OK,
    )
    .await
}

pub async fn subprompt_item_put(
    State(state): State<SharedState>,
    Path((id, subprompt_id)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let parsed = parse_body(&body);
    if let Some(refusal) = non_object_refusal(&parsed) {
        return refusal;
    }
    let req = CoreRequest::CharacterSubpromptUpdate {
        character_id: id,
        subprompt_id,
        title: tri(&parsed, "title"),
        content: tri(&parsed, "content"),
    };
    dispatch(&state, req, StatusCode::OK).await
}

pub async fn subprompt_item_delete(
    State(state): State<SharedState>,
    Path((id, subprompt_id)): Path<(String, String)>,
) -> AxumResponse {
    dispatch(
        &state,
        CoreRequest::CharacterSubpromptDelete {
            character_id: id,
            subprompt_id,
        },
        StatusCode::OK,
    )
    .await
}
