//! **`POST /api/v1/messages/{id}?action=swipe[&stream=1]`** — v4
//! `app/api/v1/messages/[id]/route.ts`'s `POST` (`f564b0de3`). v5 had no POST
//! edge on this path at all before P4.D207 (measured: `messages_routes.rs`
//! served `GET /api/v1/messages` and nothing else).
//!
//! ```text
//! POST …/{id}?action=swipe            body {"swipeIndex": N}  → the SWITCH, 200 {message}
//! POST …/{id}?action=swipe            body {} or absent       → GENERATE, 201 {message}
//! POST …/{id}?action=swipe&stream=1   body {} or absent       → GENERATE, text/event-stream
//! POST …/{id}?action=reattribute                              → a LOUD deferral (below)
//! POST …/{id} (no/unknown action)                             → v4's 400 sentence
//! ```
//!
//! ## What the `stream=1` leg is, and what it is not
//!
//! v4's streaming handler builds a `ReadableStream` fed by the service's
//! `onProgress`. v5's boundary streams only on the `Event` channel, so the
//! generation publishes [`EventPayload::SwipeProgress`] frames and this edge
//! re-frames them back into v4's exact bytes — `data: <frame JSON>\n\n`, no
//! `event:` line, no `id:`, no retry, no keep-alive comments. That is
//! [`crate::generator_sse`]'s job, shared rather than copied: P4.D207 widened
//! its pump to take the payload family as a parameter, so the swipe inherits
//! its subscribe-before-dispatch ordering AND its two recorded `RecvError`
//! divergences instead of growing a second, differently-wrong copy.
//!
//! **The SPA does not use this edge.** It dispatches
//! `messageSwipe { stream: true }` and listens for `swipeProgress` events with
//! its own `progressId`. This exists because v4 has it and a REST client may
//! want it (the order's Tier 2).
//!
//! ## v4's refusal rule, which is the whole reason the shapes differ
//!
//! `resolveSwipeTarget` runs BEFORE `new ReadableStream`, so all three refusals
//! — 404 `Message not found`, 400 `Only assistant messages can be swiped`,
//! 400 `Staff and system messages cannot be regenerated` — are ordinary JSON
//! errors on BOTH legs. Only a throw INSIDE `start()` becomes an `error` frame,
//! because by then the headers are long gone. **So a caller checks `res.ok`
//! first**, and the re-framer's outcome mapping is what preserves that: a
//! dispatch that fails with no frame emitted answers a status + JSON body
//! rather than a 200 stream.
//!
//! ## The `reattribute` deferral — LOUD, and named
//!
//! v4's POST serves two actions. `MessageReattribute` exists as a v5 verb but
//! has never had a REST edge, and wiring one is outside P4.D207's mandate. So
//! `?action=reattribute` answers a typed, explicit refusal naming itself
//! (the `data_dir::not_available` idiom) rather than v4's
//! `Action parameter required: swipe or reattribute` — that sentence would
//! claim the action is unrecognised when in fact it is recognised and not
//! served here. An unknown or absent action does get v4's sentence, because
//! there v4 and v5 agree.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{QuilltapCore as _, Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;

use crate::characters_routes::core_error_status_body;
use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// v4's own sentence for an absent or unrecognised action on this route
/// (`route.ts:244`).
const ACTION_REQUIRED: &str = "Action parameter required: swipe or reattribute";

/// v4's `swipeActionSchema` is `{ swipeIndex: z.int().min(0).optional() }` read
/// with `safeParse`, and the branch guard is `if (parsed.success && …)`. So a
/// body that fails the schema — a string `swipeIndex`, a negative one, a
/// fractional one — does NOT refuse: it falls through to the GENERATE branch
/// with the flag unread. That is v4's behaviour and the census row
/// (`MessageSwipe.swipe_index`, `V4::BodySafeParse`) already records it; this
/// reader reproduces it by taking ONLY a non-negative integer and treating
/// everything else as absent.
fn switch_index(body: &Value) -> Option<i64> {
    let v = body.get("swipeIndex")?;
    let n = v.as_i64()?;
    (n >= 0).then_some(n)
}

pub async fn messages_post(
    State(state): State<SharedState>,
    Path(message_id): Path<String>,
    Query(query): Query<crate::query::QueryPairs>,
    body: Option<axum::Json<Value>>,
) -> AxumResponse {
    // v4 does `await req.json().catch(() => ({}))` — a missing or unparseable
    // body is `{}`, never a 400.
    let body = body.map(|axum::Json(v)| v).unwrap_or(Value::Null);

    match crate::query::action(&query) {
        Some("swipe") => {}
        Some("reattribute") => {
            return error_json(
                StatusCode::NOT_IMPLEMENTED,
                "The 'reattribute' message action is recognized but not yet served over REST; \
                 dispatch `messageReattribute` instead.",
            );
        }
        // An unknown action and an ABSENT one take the same leg, which is v4's
        // shape here: its POST has no `withActionDispatch` map, just two `if`s
        // and one fall-through sentence.
        _ => return error_json(StatusCode::BAD_REQUEST, ACTION_REQUIRED),
    }

    // The SWITCH branch. v4 reads `?stream=1` only AFTER this returns, so the
    // flag is deliberately not consulted here.
    if let Some(swipe_index) = switch_index(&body) {
        let req = CoreRequest::MessageSwipe {
            message_id,
            swipe_index: Some(swipe_index),
            stream: false,
        };
        return match dispatch_core(&state, req).await {
            Ok(CoreResponse::Message(v)) => (
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
        };
    }

    let wants_stream = crate::query::first(&query, "stream") == Some("1");
    if !wants_stream {
        // v4 `handleGenerateSwipe` → `created({ message })`, a 201.
        let req = CoreRequest::MessageSwipe {
            message_id,
            swipe_index: None,
            stream: false,
        };
        return match dispatch_core(&state, req).await {
            Ok(CoreResponse::Message(v)) => (
                StatusCode::CREATED,
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
        };
    }

    // The SSE leg.
    let Some(host) = state.host() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "The engine is not running");
    };
    let req = CoreRequest::MessageSwipe {
        message_id: message_id.clone(),
        swipe_index: None,
        stream: true,
    };
    // The dispatch future is handed in UN-POLLED: the re-framer subscribes
    // first, so a frame published synchronously by the run cannot predate the
    // subscription (`generator_sse`'s module header, step 1).
    let core = host.core().clone();
    let dispatch = async move { core.dispatch(req).await };
    crate::generator_sse::stream_frames(
        host.core().event_sender(),
        // §S.2: the scope tag is the TARGET message id.
        message_id,
        crate::generator_sse::swipe_frame,
        dispatch,
        |resp: CoreResponse| match resp {
            CoreResponse::Message(_) => Ok(()),
            // A refusal that produced no frame answers v4's ordinary JSON
            // error with its status — never a 200 stream.
            CoreResponse::Error(e) => Err(core_error_status_body(e)),
            _ => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({ "error": "Unexpected core response" }),
            )),
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// v4's `safeParse` + `if (parsed.success && …)` guard, arm by arm. The
    /// interesting ones are the THREE that fall through to GENERATE rather than
    /// refusing — a wrong type, a negative index, and a fractional one all fail
    /// `z.int().min(0)`, and v4 then ignores the key entirely.
    #[test]
    fn switch_index_reproduces_v4s_safeparse_fallthrough() {
        assert_eq!(switch_index(&json!({ "swipeIndex": 0 })), Some(0));
        assert_eq!(switch_index(&json!({ "swipeIndex": 3 })), Some(3));
        // Absent / null / empty body → GENERATE.
        assert_eq!(switch_index(&json!({})), None);
        assert_eq!(switch_index(&json!({ "swipeIndex": null })), None);
        assert_eq!(switch_index(&Value::Null), None);
        // Schema failures → GENERATE, not a 400.
        assert_eq!(switch_index(&json!({ "swipeIndex": "2" })), None);
        assert_eq!(switch_index(&json!({ "swipeIndex": -1 })), None);
        assert_eq!(switch_index(&json!({ "swipeIndex": 1.5 })), None);
        assert_eq!(switch_index(&json!({ "swipeIndex": true })), None);
    }
}
