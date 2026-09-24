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
//!   preference two owners, so these edges answer a loud named refusal for
//!   it rather than silently treating it as the default arm — and never v4's
//!   `Unknown action` envelope, which would list the action as available in
//!   the answer that refuses it.
//! - Every other shape is v4's `dispatchAction` (`route.ts` at `ad1c4c37f`):
//!   GET/PUT fall back to the profile read/update when the action is ABSENT,
//!   and refuse a bare or unknown one with the envelope; the PATCH has no
//!   fallback, so an absent action is `Action parameter required`. (The old
//!   hand-rolled `Unknown action: null. Available actions: set-avatar` retired
//!   with `ad1c4c37f`.)

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

/// v4's GET/PUT thunk map on this route — one key (`route.ts` at `ad1c4c37f`).
const PROFILE_GET_PUT_ACTIONS: &[&str] = &["theme-preference"];
/// v4's PATCH thunk map — one key, no fallback.
const PROFILE_PATCH_ACTIONS: &[&str] = &["set-avatar"];
const PROFILE_PATH: &str = "/api/v1/user/profile";

/// The one v4-KNOWN arm v5 does not serve on this surface: a loud refusal that
/// names the action and where it lives, so a client that asks for it is told
/// plainly rather than served the default.
fn unported_theme_preference() -> AxumResponse {
    error_json(
        StatusCode::BAD_REQUEST,
        "The 'theme-preference' action is not served on this route; the theme \
         preference is served by theme.service over chat settings",
    )
}

// ===========================================================================
// /api/v1/user/profile
// ===========================================================================

pub async fn user_profile_get(
    State(state): State<SharedState>,
    Query(params): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // The theme-preference arm belongs to `theme.service`, not here. Absent
    // falls through to the profile read; bare / unknown are v4's envelope.
    match crate::query::dispatch_action(&params, PROFILE_GET_PUT_ACTIONS, "GET", PROFILE_PATH) {
        Ok(None) => {}
        Ok(Some(_theme_preference)) => return unported_theme_preference(),
        Err(r) => return *r,
    }
    match dispatch_core(&state, CoreRequest::UserProfileGet).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn user_profile_put(
    State(state): State<SharedState>,
    Query(params): Query<crate::query::QueryPairs>,
    body: String,
) -> AxumResponse {
    // As on the GET: absent falls through to the update.
    match crate::query::dispatch_action(&params, PROFILE_GET_PUT_ACTIONS, "PUT", PROFILE_PATH) {
        Ok(None) => {}
        Ok(Some(_theme_preference)) => return unported_theme_preference(),
        Err(r) => return *r,
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
    Query(params): Query<crate::query::QueryPairs>,
    body: String,
) -> AxumResponse {
    // v4 `dispatchAction(req, { 'set-avatar': … })` — no fallback.
    if let Err(r) = crate::query::dispatch_required_action(
        &params,
        PROFILE_PATCH_ACTIONS,
        "PATCH",
        PROFILE_PATH,
    ) {
        return *r;
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
pub async fn system_data_dir_post(Query(params): Query<crate::query::QueryPairs>) -> AxumResponse {
    // v4 `withCollectionActionDispatch({ open: handleOpen })` passes NO
    // default handler, so the middleware's own envelopes answer: absent gets
    // `Action parameter required`, and — since `ad1c4c37f` — a bare `?action=`
    // is refused as unknown alongside any other name.
    match crate::query::dispatch_required_action(
        &params,
        &["open"],
        "POST",
        "/api/v1/system/data-dir",
    ) {
        // The refusal's wording lives in the core (one source of truth) — this
        // edge only carries it out to HTTP.
        Ok(_open) => match quilltap_core::api::data_dir::not_available("open") {
            CoreResponse::Error(e) => error_to_http(e),
            _ => error_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected core response",
            ),
        },
        Err(r) => *r,
    }
}
