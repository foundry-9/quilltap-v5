//! P4.83 REST edges: `GET`/`POST /api/v1/prompt-templates` and
//! `GET`/`PUT`/`DELETE /api/v1/prompt-templates/{id}` (v4 `2f4254b42`,
//! `app/api/v1/prompt-templates/route.ts` + `[id]/route.ts`), v4-URL faithful.
//! Each edge dispatches the corresponding `Request` and UNWRAPS the dispatch
//! envelope to v4's RAW route body (the P4.6ah lesson — a v4-shaped client
//! reads the raw body, not the tagged `Response`). The JSON verbs also ride
//! `POST /api/dispatch`; these edges give v4-URL parity and own the ONE thing
//! the flat dispatch variants cannot express — a body that is not an object at
//! all (`create_zod_body_null_400`, `create_zod_body_array_400`,
//! `put_zod_body_null_400`).
//!
//! The guard ladders are the HANDLER's (`quilltap_core::api::prompt_templates`),
//! measured on v4's real routes by `prompt_templates_routes_equivalence`. This
//! file only chooses the success status (201 on create, v4's `NextResponse.json
//! ({template}, {status: 201})`) and renders the Zod
//! `{error: 'Validation error', details}` envelope the shared `error_to_http`
//! cannot (the `images_routes` / `subprompts_routes` precedent).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::prompt_templates::body_not_object_details;
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::{json, Value};

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap a prompt-template body to the raw route shape; the Zod envelope keeps
/// its `details`.
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::PromptTemplates(v)
        | CoreResponse::PromptTemplate(v)
        | CoreResponse::PromptTemplateDeleted(v) => (
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

/// v4's `schema.parse(<non-object>)` — rendered here (see the module doc) with
/// the middleware's exact envelope.
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

/// The flat variants' tri-state from a parsed object body: absent → `None`,
/// present-null → `Some(None)`, present → `Some(Some(v))`.
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
// GET / POST /api/v1/prompt-templates
// ===========================================================================

pub async fn prompt_templates_collection_get(State(state): State<SharedState>) -> AxumResponse {
    dispatch(&state, CoreRequest::PromptTemplateList, StatusCode::OK).await
}

pub async fn prompt_templates_collection_post(
    State(state): State<SharedState>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let parsed = parse_body(&body);
    if let Some(refusal) = non_object_refusal(&parsed) {
        return refusal;
    }
    let req = CoreRequest::PromptTemplateCreate {
        name: tri(&parsed, "name"),
        content: tri(&parsed, "content"),
        description: tri(&parsed, "description"),
        category: tri(&parsed, "category"),
        model_hint: tri(&parsed, "modelHint"),
    };
    // v4 `NextResponse.json({ template }, { status: 201 })`.
    dispatch(&state, req, StatusCode::CREATED).await
}

// ===========================================================================
// GET / PUT / DELETE /api/v1/prompt-templates/{id}
// ===========================================================================

pub async fn prompt_template_item_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    dispatch(
        &state,
        CoreRequest::PromptTemplateGet { id },
        StatusCode::OK,
    )
    .await
}

pub async fn prompt_template_item_put(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let parsed = parse_body(&body);
    if let Some(refusal) = non_object_refusal(&parsed) {
        return refusal;
    }
    let req = CoreRequest::PromptTemplateUpdate {
        id,
        name: tri(&parsed, "name"),
        content: tri(&parsed, "content"),
        description: tri(&parsed, "description"),
        category: tri(&parsed, "category"),
        model_hint: tri(&parsed, "modelHint"),
    };
    dispatch(&state, req, StatusCode::OK).await
}

pub async fn prompt_template_item_delete(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AxumResponse {
    dispatch(
        &state,
        CoreRequest::PromptTemplateDelete { id },
        StatusCode::OK,
    )
    .await
}
