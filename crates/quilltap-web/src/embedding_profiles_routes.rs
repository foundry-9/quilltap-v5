//! P4.9H2A REST edges: the embedding-profiles management surface. Each edge
//! dispatches the corresponding `Request` and UNWRAPS the dispatch envelope to
//! v4's RAW route body (the LOCKED SPA client reads the raw body, not the tagged
//! `Response`). The JSON verbs also ride `POST /api/dispatch`; these edges give
//! v4-URL parity for the settings client.
//!
//! - `GET /api/v1/embedding-profiles` → `{profiles, count}` (with `?action=`:
//!   `list-providers` → `{providers}`; `list-models[&provider=]` → `{provider,
//!   models}` / `Record<provider, models[]>`; `fetch-models` → the loud refusal).
//! - `POST /api/v1/embedding-profiles` → created + `apiKey` (201).
//! - `GET/PUT/DELETE /api/v1/embedding-profiles/{id}` → get / updated / `{message}`.
//! - `POST /api/v1/embedding-profiles/{id}?action=refit|reindex|reapply`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap an `EmbeddingProfile` body to the raw route shape (v4's routes answer a
/// bare JSON body, not the successResponse envelope).
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::EmbeddingProfile(v) => (
            success_status,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        CoreResponse::Error(e) => error_to_http(e),
        _ => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
    }
}

/// v4's `await req.json()` on the create/PUT routes: an EMPTY or malformed body
/// THROWS into the route's outer catch, which answers the route's FIXED 500
/// sentence (`serverError('Failed to …')`) — there is no tolerance on these two
/// routes. (§3 unify-review fix: v5 originally tolerated empty → `{}` and
/// answered a v5-invented 400 on malformed.) The boxed Err keeps the `Ok` path
/// small (`clippy::result_large_err`).
fn parse_body_strict(
    body: &axum::body::Bytes,
    fixed_sentence: &str,
) -> Result<Value, Box<AxumResponse>> {
    serde_json::from_slice(body).map_err(|e| {
        tracing::error!(
            target: "quilltap_web::embedding_profiles_routes",
            error = %e,
            "{fixed_sentence}"
        );
        Box::new(error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            fixed_sentence,
        ))
    })
}

/// v4's reindex route deliberately GUARDS its parse and defaults `scope='all'`
/// ([id]/route.ts:338-355 — "the legacy call sites POST with no body, which
/// Next.js's req.json() would reject — so guard the parse and default to
/// 'all'"): an empty OR malformed body is simply "no scope". The refit/reapply
/// routes never read the body at all.
fn parse_body_lenient(body: &axum::body::Bytes) -> Value {
    if body.is_empty() {
        return Value::Object(Default::default());
    }
    serde_json::from_slice(body).unwrap_or_else(|_| Value::Object(Default::default()))
}

// ===========================================================================
// GET / POST /api/v1/embedding-profiles
// ===========================================================================

pub async fn collection_get(
    State(state): State<SharedState>,
    Query(query): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // v4 is a plain `if (action === '…')` chain whose fallthrough is the
    // listing, so absent / `?action=` / unknown all list — `crate::query::action`
    // only has to keep FIRST-wins and fold the empty string.
    let req = match crate::query::action(&query) {
        Some("list-providers") => CoreRequest::EmbeddingProfileListProviders,
        Some("list-models") => CoreRequest::EmbeddingProfileListModels {
            provider: crate::query::first(&query, "provider").map(str::to_string),
        },
        Some("fetch-models") => CoreRequest::EmbeddingProfileFetchModels {
            // v4 400s on a missing provider before the refusal, but the arm is a
            // loud refusal either way; carry the provider through if present.
            provider: crate::query::first(&query, "provider")
                .unwrap_or_default()
                .to_string(),
            base_url: crate::query::first(&query, "baseUrl").map(str::to_string),
        },
        _ => CoreRequest::EmbeddingProfileList,
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn collection_post(
    State(state): State<SharedState>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let json_body = match parse_body_strict(&body, "Failed to create embedding profile") {
        Ok(v) => v,
        Err(r) => return *r,
    };
    // v4 `created(...)` → 201.
    match dispatch_core(
        &state,
        CoreRequest::EmbeddingProfileCreate { body: json_body },
    )
    .await
    {
        Ok(resp) => unwrap_to_http(resp, StatusCode::CREATED),
        Err(r) => r,
    }
}

// ===========================================================================
// GET / PUT / DELETE / POST /api/v1/embedding-profiles/{id}
// ===========================================================================

pub async fn item_get(State(state): State<SharedState>, Path(id): Path<String>) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::EmbeddingProfileGet { profile_id: id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn item_put(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let json_body = match parse_body_strict(&body, "Failed to update embedding profile") {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let req = CoreRequest::EmbeddingProfileUpdate {
        profile_id: id,
        body: json_body,
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn item_delete(State(state): State<SharedState>, Path(id): Path<String>) -> AxumResponse {
    match dispatch_core(
        &state,
        CoreRequest::EmbeddingProfileDelete { profile_id: id },
    )
    .await
    {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn item_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<crate::query::QueryPairs>,
    body: axum::body::Bytes,
) -> AxumResponse {
    const AVAILABLE: &[&str] = &["refit", "reindex", "reapply"];
    const PATH: &str = "/api/v1/embedding-profiles/[id]";
    let req = match crate::query::action(&query) {
        // v4's refit/reapply handlers never read the body; only reindex parses
        // it, leniently (see `parse_body_lenient`).
        Some("refit") => CoreRequest::EmbeddingProfileRefit { profile_id: id },
        Some("reindex") => {
            let json_body = parse_body_lenient(&body);
            CoreRequest::EmbeddingProfileReindex {
                profile_id: id,
                // RAW, absent-vs-null preserved: v4 tests `body.scope !==
                // undefined` and interpolates `String(scope)` into the refusal,
                // neither of which survives a coercion here (P4.60).
                scope: json_body.get("scope").cloned(),
            }
        }
        Some("reapply") => CoreRequest::EmbeddingProfileReapply { profile_id: id },
        // v4 `withActionDispatch`: unknown action / missing action → 400 with the
        // dispatcher's exact sentence + availableActions. `?action=` is JS-falsy,
        // so `crate::query::action` has already folded it onto `None` here.
        Some(other) => {
            return crate::query::unknown_action_response(other, AVAILABLE, "POST", PATH)
        }
        None => return crate::query::action_required_response(AVAILABLE, "POST", PATH),
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}
