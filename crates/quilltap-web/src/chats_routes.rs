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
//! The POST/PUT/DELETE legs of v4's collection route are NOT registered here —
//! they were never part of this lane and have no v5 REST edge today; the SPA
//! reaches them through `/api/dispatch`.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::json;

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
