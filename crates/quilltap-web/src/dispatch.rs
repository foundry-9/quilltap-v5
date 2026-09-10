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
            ErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
            ErrorKind::Forbidden => StatusCode::FORBIDDEN,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Conflict => StatusCode::CONFLICT,
            ErrorKind::Unprocessable => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorKind::Locked => StatusCode::SERVICE_UNAVAILABLE,
            // v4's deliberate contextful store-unavailable 503 (P4.23) — a
            // SEPARATE kind from Locked so "vault locked" stays
            // distinguishable from "store broken".
            ErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
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

/// The transport-agnostic dispatch core: raw request bytes in → (HTTP status,
/// envelope `Value`). All three arms live here — boot-failure (the 503 arm's
/// body), malformed body (the 400 arm's body), and the dispatched envelope
/// with the Locked merge — so the HTTP route and the Tauri IPC `dispatch`
/// command share them verbatim. IPC carries no HTTP status; there the
/// envelope alone is authoritative and the status is dropped.
pub async fn dispatch_body(state: &SharedState, body: &[u8]) -> (StatusCode, Value) {
    let Some(host) = state.host() else {
        // A failed boot: everything is 503 (health carries the details).
        let msg = match &state.startup {
            StartupStatus::LockConflict { message, .. } => message.clone(),
            StartupStatus::Failed { message } => message.clone(),
            StartupStatus::Running(_) => unreachable!(),
        };
        let resp = Response::error(ErrorKind::Internal, msg);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::to_value(&resp).unwrap_or(Value::Null),
        );
    };

    let req: Request = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::error(ErrorKind::BadRequest, format!("Invalid request: {e}"));
            return (
                StatusCode::BAD_REQUEST,
                serde_json::to_value(&resp).unwrap_or(Value::Null),
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
        // The store-unavailable 503 merges v4's contextful body keys
        // (`{error: "<…> unavailable", "<entity>Id": <id>}` —
        // context.ts:176-205) alongside the typed envelope, the same pattern
        // as the Locked readiness body above (P4.23).
        if e.kind == ErrorKind::Unavailable {
            if let (Some(obj), Some(Value::Object(wire))) =
                (body.as_object_mut(), e.unavailable_wire_body())
            {
                for (k, v) in wire {
                    obj.insert(k, v);
                }
            }
        }
        // v4's `validationError(err)` body (`{error: 'Validation error',
        // details: [...]}` — P4.D138's image-profile LoRA guard) merges the
        // same way, so a dispatch client reads v4's exact keys.
        if let (Some(obj), Some(Value::Object(wire))) =
            (body.as_object_mut(), e.validation_wire_body())
        {
            for (k, v) in wire {
                obj.insert(k, v);
            }
        }
        // v4's ALREADY_SAVED 409 (`actions/save-image.ts:118-125`) answers four
        // FLAT siblings — `{error, code, relativePath, keptAt}` — and the save
        // dialog reads `body.code` / `body.keptAt` off the top level, so the
        // riders merge beside the typed envelope the same way (P4.D174 §C.3;
        // the §3 unification review of the `78b381a96` round caught them
        // nested under `details`, unreachable by any client).
        merge_already_saved_riders(e, &mut body);
    }
    (status, body)
}

/// The one place the dispatch wire flattens [`CoreError::already_saved`]:
/// `CoreError::already_saved_wire_body`'s keys land beside the typed envelope.
/// A no-op for every other error.
fn merge_already_saved_riders(e: &quilltap_core::api::CoreError, body: &mut Value) {
    if let (Some(obj), Some(Value::Object(wire))) =
        (body.as_object_mut(), e.already_saved_wire_body())
    {
        for (k, v) in wire {
            obj.insert(k, v);
        }
    }
}

pub async fn dispatch(State(state): State<SharedState>, body: Bytes) -> AxumResponse {
    let (status, body) = dispatch_body(&state, &body).await;
    json_response(status, &body)
}

#[cfg(test)]
mod already_saved_wire_tests {
    use super::*;
    use quilltap_core::api::types::AlreadySavedRiders;

    /// The §3 unification catch of the `78b381a96` round: the riders MUST be
    /// flat siblings of `error`, never nested. Mutation: move them back onto
    /// `details` (or drop the merge) and both assertions redden.
    #[test]
    fn the_already_saved_riders_land_flat_beside_error_and_code() {
        let mut resp = Response::error(ErrorKind::Conflict, "already in this album");
        if let Response::Error(e) = &mut resp {
            e.code = Some("ALREADY_SAVED".into());
            e.already_saved = Some(Box::new(AlreadySavedRiders {
                relative_path: Some("Photos/marchpane.webp".into()),
                kept_at: Some("2026-09-01T00:00:00.000Z".into()),
            }));
        }
        let mut body = serde_json::to_value(&resp).unwrap();
        let Response::Error(e) = &resp else {
            unreachable!()
        };
        merge_already_saved_riders(e, &mut body);
        assert_eq!(body["error"], "already in this album");
        assert_eq!(body["code"], "ALREADY_SAVED");
        assert_eq!(body["relativePath"], "Photos/marchpane.webp");
        assert_eq!(body["keptAt"], "2026-09-01T00:00:00.000Z");
        assert!(
            body.get("details").is_none(),
            "v4's 409 has no `details` key"
        );
        // The typed envelope still carries the riders for the SPA's reader.
        assert_eq!(
            body["data"]["alreadySaved"]["keptAt"],
            "2026-09-01T00:00:00.000Z"
        );
    }

    #[test]
    fn a_plain_error_merges_nothing() {
        let resp = Response::error(ErrorKind::BadRequest, "no");
        let Response::Error(e) = &resp else {
            unreachable!()
        };
        let mut body = serde_json::to_value(&resp).unwrap();
        let before = body.clone();
        merge_already_saved_riders(e, &mut body);
        assert_eq!(body, before);
    }
}
