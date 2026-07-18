//! P4.9c REST edges (lane C): the user-profile + data-directory surface. Each
//! edge dispatches the corresponding `Request` and UNWRAPS the dispatch
//! envelope to v4's RAW route body (the P4.6ah lesson — v4-shaped clients read
//! the raw body, not the tagged `Response`).
//!
//! - `GET    /api/v1/user/profile`                     → `{profile}`
//! - `PUT    /api/v1/user/profile`                     → `{profile}`
//! - `PATCH  /api/v1/user/profile?action=set-avatar`   → `{profile}`
//! - `GET    /api/v1/system/data-dir`                  → the `DataDirInfo` payload
//! - `POST   /api/v1/system/data-dir?action=open`      → the loud refusal
//!
//! ## The action multiplexing this edge owns
//!
//! v4's profile route is one file serving several `?action=` values. Two of
//! those arms are NOT this lane's:
//!
//! - `?action=theme-preference` (GET and PUT) is already live in v5 through
//!   `theme.service` over chatSettings. Reaching it here would give one
//!   preference two owners, so these edges answer v4's own unknown-action
//!   shape for it rather than silently treating it as the default arm.
//! - The PATCH's action gate is v4's (`route.ts:234-236`): any action other
//!   than `set-avatar` — including an ABSENT one, which interpolates as the
//!   literal `null` — is a 400 naming what is available.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap a profile / data-dir body to the raw route shape.
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::UserProfile(v) | CoreResponse::SystemDataDir(v) => (
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

/// v4 `validationError` at 400 — the body v4's middleware emits for any Zod
/// failure. v5 carries the message only (no `details` array).
fn validation_error() -> AxumResponse {
    error_json(StatusCode::BAD_REQUEST, "Validation error")
}

/// The one arm v5 refuses on this surface, worded as v4's unknown-action 400 so
/// a client that asks for it is told plainly rather than served the default.
fn unported_action(action: &str, available: &str) -> AxumResponse {
    error_json(
        StatusCode::BAD_REQUEST,
        &format!("Unknown action: {action}. Available actions: {available}"),
    )
}

/// v4 reads `?action=` with `searchParams.get('action')`, which is `null` when
/// absent — and v4 interpolates that `null` straight into its message.
fn action_of(params: &HashMap<String, String>) -> Option<&str> {
    params.get("action").map(String::as_str)
}

// ===========================================================================
// /api/v1/user/profile
// ===========================================================================

pub async fn user_profile_get(
    State(state): State<SharedState>,
    Query(params): Query<HashMap<String, String>>,
) -> AxumResponse {
    // The theme-preference arm belongs to `theme.service`, not here.
    if let Some(action) = action_of(&params) {
        return unported_action(action, "(none)");
    }
    match dispatch_core(&state, CoreRequest::UserProfileGet).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn user_profile_put(
    State(state): State<SharedState>,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> AxumResponse {
    if let Some(action) = action_of(&params) {
        return unported_action(action, "(none)");
    }

    // Decode through the Request itself so the absent / explicit-null / value
    // tri-state is resolved by exactly ONE piece of code (the `double_option`
    // fields), not re-implemented at the edge.
    let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(&body) else {
        return validation_error();
    };
    map.retain(|k, _| matches!(k.as_str(), "name" | "email" | "image"));
    map.insert("type".into(), Value::String("userProfileUpdate".into()));
    let Ok(req) = serde_json::from_value::<CoreRequest>(Value::Object(map)) else {
        return validation_error();
    };

    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn user_profile_patch(
    State(state): State<SharedState>,
    Query(params): Query<HashMap<String, String>>,
    body: String,
) -> AxumResponse {
    // v4 `route.ts:234-236`, verbatim — including the literal "null" an absent
    // action interpolates to.
    match action_of(&params) {
        Some("set-avatar") => {}
        other => {
            return error_json(
                StatusCode::BAD_REQUEST,
                &format!(
                    "Unknown action: {}. Available actions: set-avatar",
                    other.unwrap_or("null")
                ),
            );
        }
    }

    let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(&body) else {
        return validation_error();
    };
    map.retain(|k, _| k == "imageId");
    map.insert("type".into(), Value::String("userProfileSetAvatar".into()));
    let Ok(req) = serde_json::from_value::<CoreRequest>(Value::Object(map)) else {
        return validation_error();
    };

    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

// ===========================================================================
// /api/v1/system/data-dir
// ===========================================================================

pub async fn system_data_dir_get(State(state): State<SharedState>) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::SystemDataDir).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

/// v4 `POST /api/v1/system/data-dir?action=open` shells out to
/// `open`/`explorer`/`xdg-open`. v5 REFUSES, loudly and by name — a Tauri
/// shell-open is a named future native nicety, and the HTTP deployment has no
/// business opening a file browser on the server's desktop.
pub async fn system_data_dir_post(Query(params): Query<HashMap<String, String>>) -> AxumResponse {
    match action_of(&params) {
        // The refusal's wording lives in the core (one source of truth) — this
        // edge only carries it out to HTTP.
        Some("open") => match quilltap_core::api::data_dir::not_available("open") {
            CoreResponse::Error(e) => error_to_http(e),
            _ => error_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected core response",
            ),
        },
        other => unported_action(other.unwrap_or("null"), "open"),
    }
}
