//! **`GET /api/v1/messages`** — v4 `app/api/v1/messages/route.ts`'s `GET`
//! (`5029075bb`). v5 had no messages edge at all before this.
//!
//! ```text
//! GET /api/v1/messages?chatId=X&action=transcript[&knownVersion=N]  → handleTranscript
//! GET /api/v1/messages?chatId=X                                     → handleListMessages
//! GET /api/v1/messages?chatId=X&action=<unknown>                    → 400, v4's envelope
//! ```
//!
//! **The dispatcher's shape was MEASURED, and it is not what it looks like.**
//! v4 wires this as `withCollectionActionDispatch({transcript}, handleList
//! Messages)` — a handler map WITH a default — so the natural reading is that
//! an unrecognised action falls through to the listing. It does not:
//! `withActionDispatch` tests `if (action)` FIRST and answers the 400
//! `{error: "Unknown action: X", availableActions: [...]}` envelope for any
//! truthy action it does not know. The default runs only when the parameter is
//! absent — or present-and-empty, which is JS-falsy
//! ([`crate::query::action`] folds those two together).
//!
//! **`chatId` is checked before anything else**, inside each handler rather
//! than at the dispatcher, so a missing `chatId` answers 400 `Query parameter
//! required: chatId` on BOTH legs — and before the 404 an unknown chat would
//! otherwise get.
//!
//! **`knownVersion` goes through JS `Number()` semantics, not Rust's parse.**
//! v4 does `param === null ? null : Number(param)` and then gates on
//! `Number.isInteger`. So `"12abc"` is NaN — *not* a prefix parse to 12 — and
//! `""` is 0 (v4's `Number('')`, which is a real integer and will match a chat
//! whose counter is still 0). Both are v4's, both are pinned.
//!
//! `POST /api/v1/messages` (v4's SSE send) is NOT registered: NO-COUNTERPART
//! by the locked boundary — sends are `chatSend` over dispatch and tokens
//! arrive on the `Event` channel. No 405 is invented as a "port" of it.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// v4's handler-map keys, in insertion order — `Object.keys(actions)`, which
/// is what the 400's `availableActions` tail carries.
const MESSAGES_GET_ACTIONS: &[&str] = &["transcript"];

/// v4 `Number(param)` — the whole ECMAScript conversion, not a Rust parse.
/// Returned as a JSON value so the core handler applies v4's
/// `Number.isInteger` gate on exactly the number v4 would have produced; NaN
/// has no JSON spelling, so it becomes `None` ("no known version"), which is
/// the same answer `Number.isInteger(NaN)` gives.
fn known_version_param(raw: Option<&str>) -> Option<serde_json::Value> {
    let n = quilltap_core::jsnum::number_from_str(raw?);
    if n.is_nan() {
        return None;
    }
    serde_json::Number::from_f64(n).map(serde_json::Value::Number)
}

pub async fn messages_get(
    State(state): State<SharedState>,
    Query(query): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    let chat_id = crate::query::first(&query, "chatId");

    match crate::query::action(&query) {
        Some("transcript") => {
            // v4 checks `chatId` inside the handler, so it precedes the 404.
            let Some(chat_id) = chat_id.filter(|s| !s.is_empty()) else {
                return error_json(StatusCode::BAD_REQUEST, "Query parameter required: chatId");
            };
            let req = CoreRequest::ChatTranscript {
                chat_id: chat_id.to_string(),
                known_version: known_version_param(crate::query::first(&query, "knownVersion")),
            };
            match dispatch_core(&state, req).await {
                Ok(CoreResponse::ChatTranscript(v)) => (
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
        Some(other) => crate::query::unknown_action_response(
            other,
            MESSAGES_GET_ACTIONS,
            "GET",
            "/api/v1/messages",
        ),
        // v4's `defaultHandler` — `handleListMessages`. An absent `?action=`
        // and a present-but-empty one take this same leg.
        None => {
            let Some(chat_id) = chat_id.filter(|s| !s.is_empty()) else {
                return error_json(StatusCode::BAD_REQUEST, "Query parameter required: chatId");
            };
            let req = CoreRequest::ChatMessageEvents {
                chat_id: chat_id.to_string(),
            };
            match dispatch_core(&state, req).await {
                Ok(CoreResponse::ChatMessageEvents(v)) => (
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's `Number()` on the raw query value, arm by arm. The interesting
    /// ones are the two that a Rust `parse::<i64>()` would get wrong in
    /// OPPOSITE directions: `"12abc"` (Rust refuses, v4 says NaN → absent —
    /// same answer by luck) and `"12.0"`/`" 12 "` (Rust refuses, v4 accepts as
    /// 12 — a real difference), plus `""`, which v4 turns into a genuine 0.
    #[test]
    fn known_version_follows_js_number_semantics() {
        let n = |s: &str| known_version_param(Some(s)).and_then(|v| v.as_f64());
        assert_eq!(n("3"), Some(3.0));
        assert_eq!(n("0"), Some(0.0));
        // `Number('')` is 0, NOT NaN — so a bare `?knownVersion=` really can
        // answer `unchanged` on a chat whose counter has never moved.
        assert_eq!(n(""), Some(0.0));
        // Whitespace is trimmed; a decimal point survives as a float and the
        // core's `Number.isInteger` gate decides what to do with it.
        assert_eq!(n(" 12 "), Some(12.0));
        assert_eq!(n("12.0"), Some(12.0));
        assert_eq!(n("1.5"), Some(1.5));
        // NOT a prefix parse.
        assert_eq!(n("12abc"), None);
        assert_eq!(n("abc"), None);
        assert_eq!(known_version_param(None), None);
    }
}
