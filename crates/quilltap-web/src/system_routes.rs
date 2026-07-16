//! P4.6au REST edge: the home-dashboard payload.
//!
//! - `GET /api/v1/system/home` → v4's `successResponse(data)` — the RAW
//!   `HomeData` body (`NextResponse.json(data)`, no envelope), status 200; on
//!   internal failure v4's `serverError('Failed to load the home dashboard')`
//!   → 500 `{error}` (the core arm carries that exact message).
//!
//! The verb also rides `POST /api/dispatch`; this edge gives v4-URL parity.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

pub async fn system_home_get(State(state): State<SharedState>) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::SystemHome).await {
        Ok(CoreResponse::SystemHome(v)) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        Ok(CoreResponse::Error(e)) => error_to_http(e),
        Ok(_) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
        Err(r) => r,
    }
}
