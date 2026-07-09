//! `POST /api/dispatch` — the one action route (D3): body = the `Request`
//! enum (internally tagged JSON), reply = the `Response` enum serialized
//! verbatim with the HTTP status mapped from `ErrorKind` (v4
//! `lib/api/responses.ts` semantics), and the `Locked` refusal carrying v4's
//! readiness body (`auth.ts:98–105`) merged alongside the typed envelope.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{ErrorKind, QuilltapCore, Request, Response};
use serde_json::{json, Value};

use crate::state::{SharedState, StartupStatus};

/// Map a dispatch `Response` to its HTTP status (v4 responses.ts: success 200,
/// bad-request 400, not-found 404, locked → the 503 setup surface, internal
/// 500).
fn status_for(resp: &Response) -> StatusCode {
    match resp {
        Response::Error(e) => match e.kind {
            ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Locked => StatusCode::SERVICE_UNAVAILABLE,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        },
        _ => StatusCode::OK,
    }
}

fn json_response(status: StatusCode, body: &Value) -> AxumResponse {
    (
        status,
        [("content-type", "application/json")],
        body.to_string(),
    )
        .into_response()
}

pub async fn dispatch(State(state): State<SharedState>, body: Bytes) -> AxumResponse {
    let Some(host) = state.host() else {
        // A failed boot: everything is 503 (health carries the details).
        let msg = match &state.startup {
            StartupStatus::LockConflict { message, .. } => message.clone(),
            StartupStatus::Failed { message } => message.clone(),
            StartupStatus::Running(_) => unreachable!(),
        };
        let resp = Response::error(ErrorKind::Internal, msg);
        return json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            &serde_json::to_value(&resp).unwrap_or(Value::Null),
        );
    };

    let req: Request = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::error(ErrorKind::BadRequest, format!("Invalid request: {e}"));
            return json_response(
                StatusCode::BAD_REQUEST,
                &serde_json::to_value(&resp).unwrap_or(Value::Null),
            );
        }
    };

    let resp = host.core().dispatch(req).await;
    let status = status_for(&resp);
    let mut body = serde_json::to_value(&resp).unwrap_or(Value::Null);

    // The Locked 503 carries v4's readiness body merged in (auth.ts:98–105:
    // `{error: 'Setup required', setupUrl: '/setup', pepperState}`) alongside
    // the typed envelope, so dispatch clients stay typed.
    if let Response::Error(e) = &resp {
        if e.kind == ErrorKind::Locked {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("error".into(), json!("Setup required"));
                obj.insert("setupUrl".into(), json!("/setup"));
                obj.insert(
                    "pepperState".into(),
                    serde_json::to_value(e.pepper_state).unwrap_or(Value::Null),
                );
            }
        }
    }
    json_response(status, &body)
}
