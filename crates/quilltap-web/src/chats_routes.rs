//! P4.D143 §H: the chat-collection REST edge — v4 `app/api/v1/chats/route.ts`'s
//! `GET` dispatcher.
//!
//! - `GET /api/v1/chats?action=has-dangerous` → `{ hasDangerous: boolean }`
//!   (v4 `handleHasDangerous`, re-based by `c43d3b1b4` onto the uncensored
//!   route: the Quick-hide toggle hides Flagged AND Uncensored, not every chat
//!   carrying a preserved label). v5 never had this edge before.
//! - `GET /api/v1/chats?action=<anything else>` → v4's exact 400,
//!   `Unknown action: X. Available actions: has-dangerous`.
//! - `GET /api/v1/chats` with no action → `handleList`, delegated to the
//!   `ListChats` verb the SPA already dispatches. v4 serves the list here, so
//!   refusing it would be an invention; the query parsing below is v4's,
//!   parameter for parameter.
//!
//! The POST/PUT legs of v4's collection route are NOT registered here — they
//! were never part of that lane and have no v5 REST edge today; the SPA reaches
//! them through `/api/dispatch`.
//!
//! **P4.80 (dogfood finding #117)** adds the per-id DELETE edge below —
//! `DELETE /api/v1/chats/{id}`, v4's whole `handleDelete` dispatch
//! (`app/api/v1/chats/[id]/handlers/delete.ts`). Until it landed, v5 answered
//! **405** on every one of v4's three DELETE surfaces and a salon chat could
//! not be deleted from anywhere.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::{json, Value};

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// v4 `CHAT_GET_ACTIONS` — the whole list, and the source of the 400's tail.
const CHAT_GET_ACTIONS: &[&str] = &["has-dangerous"];

pub async fn chats_collection_get(
    State(state): State<SharedState>,
    Query(query): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // v4's gate is `if (!action) return handleList(...)` — JS truthiness, so a
    // present-but-empty `?action=` lists exactly like an absent one.
    match crate::query::action(&query) {
        Some("has-dangerous") => {
            match dispatch_core(&state, CoreRequest::ChatsHasDangerous).await {
                Ok(CoreResponse::ChatsHasDangerous(v)) => (
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
        Some(other) => error_json(
            StatusCode::BAD_REQUEST,
            &format!(
                "Unknown action: {other}. Available actions: {}",
                CHAT_GET_ACTIONS.join(", ")
            ),
        ),
        // v4 `handleList`. `excludeTagIds` splits on `,` and drops empties;
        // `limit` is v4's `limitParam ? parseInt(limitParam, 10) : undefined` —
        // a PREFIX parse (`"12abc"` → 12, `" 12"` → 12, `"12.9"` → 12), so it
        // goes through core's `js_parse_int_10` twin rather than Rust's
        // whole-string `parse`; an empty or digitless value is NaN there and
        // `limit && limit > 0` is then false — `None` here (unification
        // review, 2026-09-02); `includeAutonomous` is a strict `=== 'true'`.
        None => {
            let exclude_tag_ids = crate::query::first(&query, "excludeTagIds")
                .map(|s| {
                    s.split(',')
                        .filter(|p| !p.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let limit = crate::query::first(&query, "limit").and_then(|s| {
                let n = quilltap_core::api::llm_logs::js_parse_int_10(s);
                n.is_finite().then_some(n as i64)
            });
            let include_autonomous =
                crate::query::first(&query, "includeAutonomous") == Some("true");
            let req = CoreRequest::ListChats {
                exclude_tag_ids,
                limit,
                include_autonomous,
            };
            match dispatch_core(&state, req).await {
                Ok(CoreResponse::Chats(chats)) => (
                    StatusCode::OK,
                    [("content-type", "application/json")],
                    json!({ "chats": chats }).to_string(),
                )
                    .into_response(),
                // v4 `handleList`'s catch answers the FIXED sentence, never the
                // error's own text (`route.ts:888-890` `serverError('Failed to
                // fetch chats')`); the dispatch verb keeps its typed error for
                // the SPA's own path (unification review, 2026-09-02).
                Ok(CoreResponse::Error(_)) => {
                    error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch chats")
                }
                Ok(_) => error_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unexpected core response",
                ),
                Err(r) => r,
            }
        }
    }
}

// ===========================================================================
// P4.80 — DELETE /api/v1/chats/{id}
// ===========================================================================

/// v4 `DELETE /api/v1/chats/[id]` — the transport half of `handleDelete`.
///
/// The DISPATCH itself (the four legs, the guard order, the Zod parse, v4's
/// sentences) lives in `quilltap_core::api::chat_delete::chat_delete_dispatch`,
/// so both transports answer from one piece of code and the differential can
/// drive it. What is left here is genuinely transport: read the raw
/// `?action=` (UNFOLDED — the `if (action)` truthiness belongs to the ported
/// dispatch, which is why `query::first` is used rather than `query::action`),
/// turn the request bytes into the value `await req.json()` would yield, and
/// render the typed `Response`.
pub async fn chat_delete(
    State(state): State<SharedState>,
    Path(chat_id): Path<String>,
    Query(query): Query<crate::query::QueryPairs>,
    body: axum::body::Bytes,
) -> AxumResponse {
    // v4's `getActionParam` is `searchParams.get('action')` — FIRST wins, and
    // the empty string SURVIVES to the dispatch's own `if (action)`.
    let raw_action = crate::query::first(&query, "action");

    // `await req.json()` on an EMPTY body throws in Next just as it does here;
    // v4 only ever reaches it on the stop-impersonate leg (`participants.ts:89`
    // `await req.json()`). ANY body that is not JSON — including an EMPTY one:
    // `req.json()` on zero bytes throws the same `SyntaxError` — is what v4's
    // middleware turns into 500 `Internal server error` (not a ZodError, so
    // not a 400). The §3 unification review retired an `is_empty → {}`
    // special case here that answered 400 where v4 answers 500; the
    // `stop_impersonate_empty_body` corpus row now measures it.
    // Parsed HERE, judged THERE: the composite answers the 500 only on the
    // stop-impersonate leg and only after its chat gate (a missing chat with an
    // unreadable body is v4's 404, `stop_impersonate_missing_chat_empty_body`).
    let json_body: Option<Value> = serde_json::from_slice(&body).ok();

    // The dispatch reaches the ported composite DIRECTLY rather than through a
    // second `Request` variant: §B of the round's contract admits exactly one
    // new verb here (`chatDelete`, which the SPA uses), and the REST edge's
    // extra needs — the raw action, the un-parsed body — are transport shape,
    // not a wire contract. `files_routes::db_and_backend` is the precedent for
    // an edge holding the `Db` itself.
    let db = match crate::files_routes::db_and_backend(&state) {
        Ok((db, _)) => db,
        Err(r) => return *r,
    };
    let resp = quilltap_core::api::chat_delete::chat_delete_dispatch(
        &db, &chat_id, raw_action, json_body.as_ref(),
    )
    .await;
    match resp {
        // `{success: true}` (delete), `{success, previousState}` (reset-state),
        // the impersonation body (stop-impersonate) — v4 sends each raw.
        CoreResponse::ChatAdmin(v)
        | CoreResponse::State(v)
        | CoreResponse::ChatImpersonation(v) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        // v4's `validationError` body carries `details` beside `error`; the
        // shared mapper only knows the plain `{error}` and the store-unavailable
        // shapes, so the Zod envelope is rendered here.
        CoreResponse::Error(e) => match e.validation_wire_body() {
            Some(wire) => (
                StatusCode::BAD_REQUEST,
                [("content-type", "application/json")],
                wire.to_string(),
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
