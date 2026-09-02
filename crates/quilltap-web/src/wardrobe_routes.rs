//! P4.9f1 REST edges (lane F1): the wardrobe server surface. Each edge
//! dispatches the corresponding `Request` and UNWRAPS the dispatch envelope to
//! v4's RAW route body (the P4.6ah lesson).
//!
//! - `GET|POST   /api/v1/wardrobe`                 → the GLOBAL archetype tier
//! - `GET|PUT|DELETE /api/v1/wardrobe/{itemId}`    → one archetype
//! - `GET|POST   /api/v1/wardrobe/transfers`       → the transfers pair
//! - `POST       /api/v1/wardrobe/preview-avatar`  → the one-shot render
//! - `POST       /api/v1/wardrobe/analyze-image`   → REFUSAL-ARMED (tier 3)
//! - `GET        /api/v1/chats/{id}?action=outfit` → the equipped-outfit read
//!   (the fan-out DELEGATES every other action to the landed
//!   [`crate::text_replacements_routes::chat_get_background`] — that file is
//!   lane-foreign this round, so the fan-out lives here and lib.rs re-points
//!   the route registration, which IS this lane's to edit)
//! - `POST       /api/v1/chats/{id}?action=equip|regenerate-avatar` — new
//!   registration (v4's chat POST actions had no v5 REST edge before this)
//!
//! ## Malformed-JSON bodies (v4's per-route `await req.json()` throw arms)
//!
//! Each POST/PUT edge maps an unparsable body to the exact place v4's throw
//! lands: the equip catch (`Failed to equip wardrobe slot`), the transfers
//! catch (`Failed to transfer wardrobe item`), the regenerate catch
//! (`Failed to queue avatar regeneration`), and the uncaught middleware 500
//! (`Internal server error`) for the archetype create/update and
//! preview-avatar routes.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use quilltap_core::content_disposition::{build_content_disposition, Disposition};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{chat_get_background, dispatch_core, error_to_http};

/// Unwrap a wardrobe-family body to the raw route shape.
fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::Wardrobe(v) | CoreResponse::ChatOutfit(v) | CoreResponse::ChatDialog(v) => (
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

/// v4 `readIncludeArchived` (`lib/api/query-params.ts`, `d25dacc1`): the
/// `?includeArchived` opt-in, accepted ONLY as the literal `true` or the bare
/// valueless spelling. `1`, `TRUE`, `yes` all mean "no" — one reader so the
/// accepted spelling cannot drift between the routes that honour it, and it
/// falls CLOSED on anything else (hiding archived entries is always the safe
/// answer to "were we asked?").
pub(crate) fn read_include_archived(query: &HashMap<String, String>) -> bool {
    matches!(
        query.get("includeArchived").map(String::as_str),
        Some("true") | Some("")
    )
}

/// Parse a JSON body, mapping a failure to the route-specific v4 arm.
fn parse_body(body: &str, on_bad_json: &str) -> Result<Value, Box<AxumResponse>> {
    serde_json::from_str::<Value>(body)
        .map_err(|_| Box::new(error_json(StatusCode::INTERNAL_SERVER_ERROR, on_bad_json)))
}

// ===========================================================================
// /api/v1/wardrobe
// ===========================================================================

/// v4 `b86bb1a5` puts the General dressing-instructions read on this collection
/// route behind `?action=instructions` (`withCollectionActionDispatch`);
/// everything else is the archetype listing.
pub async fn wardrobe_get(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // Every query key this route reads is a v4 `searchParams.get` — FIRST wins,
    // so the pair list collapses to the map the rest of the handler expects.
    let query = crate::query::first_map(&pairs);
    let req = match query.get("action").map(String::as_str) {
        Some("instructions") => CoreRequest::WardrobeInstructionsGet,
        Some(other) if !other.is_empty() => {
            return crate::query::unknown_action_response(
                other,
                &["instructions"],
                "GET",
                "/api/v1/wardrobe",
            )
        }
        _ => CoreRequest::WardrobeList {
            include_archived: read_include_archived(&query),
        },
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn wardrobe_post(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
    body: String,
) -> AxumResponse {
    // Every query key this route reads is a v4 `searchParams.get` — FIRST wins,
    // so the pair list collapses to the map the rest of the handler expects.
    let query = crate::query::first_map(&pairs);
    if let Some(other) = query.get("action").map(String::as_str) {
        if other != "instructions" && !other.is_empty() {
            return crate::query::unknown_action_response(
                other,
                &["instructions"],
                "POST",
                "/api/v1/wardrobe",
            );
        }
    }
    if query.get("action").map(String::as_str) == Some("instructions") {
        let body = match parse_body(&body, "Internal server error") {
            Ok(v) => v,
            Err(r) => return *r,
        };
        // Decoded THROUGH the Request enum so the required-but-nullable
        // `instructions` key keeps its absent / null / value tri-state.
        let Some(req) = quilltap_core::api::types::wardrobe_instructions_set_request(
            &body,
            "wardrobeInstructionsSet",
            None,
        ) else {
            return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
        };
        return match dispatch_core(&state, req).await {
            Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
            Err(r) => r,
        };
    }
    let item = match parse_body(&body, "Internal server error") {
        Ok(v) => v,
        Err(r) => return *r,
    };
    match dispatch_core(&state, CoreRequest::WardrobeCreate { item }).await {
        // v4 `created({wardrobeItem})` — 201, body raw.
        Ok(resp) => unwrap_to_http(resp, StatusCode::CREATED),
        Err(r) => r,
    }
}

// ===========================================================================
// /api/v1/wardrobe/{itemId}
// ===========================================================================

pub async fn wardrobe_item_get(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::WardrobeItemGet { item_id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn wardrobe_item_put(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
    body: String,
) -> AxumResponse {
    let item = match parse_body(&body, "Internal server error") {
        Ok(v) => v,
        Err(r) => return *r,
    };
    match dispatch_core(&state, CoreRequest::WardrobeUpdate { item_id, item }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn wardrobe_item_delete(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::WardrobeDelete { item_id }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

// ===========================================================================
// /api/v1/wardrobe/transfers
// ===========================================================================

pub async fn wardrobe_transfers_get(State(state): State<SharedState>) -> AxumResponse {
    match dispatch_core(&state, CoreRequest::WardrobeTransferDestinations).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn wardrobe_transfers_post(
    State(state): State<SharedState>,
    body: String,
) -> AxumResponse {
    let body = match parse_body(&body, "Failed to transfer wardrobe item") {
        Ok(v) => v,
        Err(r) => return *r,
    };
    match dispatch_core(&state, CoreRequest::WardrobeTransferApply { body }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

// ===========================================================================
// /api/v1/wardrobe/preview-avatar | analyze-image
// ===========================================================================

pub async fn wardrobe_preview_avatar_post(
    State(state): State<SharedState>,
    body: String,
) -> AxumResponse {
    // v4's parse block rethrows a non-Zod `req.json()` throw → middleware 500.
    let body = match parse_body(&body, "Internal server error") {
        Ok(v) => v,
        Err(r) => return *r,
    };
    match dispatch_core(&state, CoreRequest::WardrobePreviewAvatar { body }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

pub async fn wardrobe_analyze_image_post(
    State(state): State<SharedState>,
    body: String,
) -> AxumResponse {
    let body = match parse_body(&body, "Internal server error") {
        Ok(v) => v,
        Err(r) => return *r,
    };
    match dispatch_core(&state, CoreRequest::WardrobeAnalyzeImage { body }).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

// ===========================================================================
// GET /api/v1/chats/{id} — the action fan-out (outfit + the landed delegates)
// ===========================================================================

/// The chat-GET action fan-out this lane re-points the registration at:
/// `?action=outfit` is served here; `?action=outfit-summary` is the NAMED loud
/// deferral (v4's third handler in the same route file — unscoped by the
/// order's §1, recorded in the lane record); everything else delegates to the
/// landed P4.6ak/P4.6ao handler untouched.
pub async fn chat_action_get(
    state: State<SharedState>,
    path: Path<String>,
    query: Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // v4's chat GET is a plain `if (action === '…')` chain whose fallthrough is
    // the full chat payload, so absent / `?action=` / unknown all delegate the
    // same way here; only FIRST-wins had to change.
    match crate::query::first(&query.0, "action") {
        Some("outfit") => {
            let req = CoreRequest::ChatOutfitGet { chat_id: path.0 };
            match dispatch_core(&state.0, req).await {
                Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
                Err(r) => r,
            }
        }
        Some("outfit-summary") => {
            let req = CoreRequest::ChatOutfitSummary { chat_id: path.0 };
            match dispatch_core(&state.0, req).await {
                Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
                Err(r) => r,
            }
        }
        // The SillyTavern-JSONL byte download (P4.9E3B — v4 get.ts:46–127).
        // Headers live at this edge: `application/x-ndjson` + v4's exact
        // attachment filename (the characters_routes.rs byte-leg precedent).
        Some("export") => {
            let req = CoreRequest::ChatExport { chat_id: path.0 };
            match dispatch_core(&state.0, req).await {
                Ok(CoreResponse::ChatExportPayload { filename, jsonl }) => (
                    StatusCode::OK,
                    [
                        ("content-type", "application/x-ndjson".to_string()),
                        (
                            "content-disposition",
                            format!("attachment; filename=\"{filename}\""),
                        ),
                    ],
                    jsonl,
                )
                    .into_response(),
                Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
                Err(r) => r,
            }
        }
        // The readable Markdown transcript (P4.d28 — v4's `export-markdown.ts`,
        // registered in v4's if-chain right after `export`). Its headers differ
        // from the JSONL leg's in two ways that are the point: the RFC 5987
        // disposition (a chat title reaches the filename with its non-ASCII
        // intact) and `Cache-Control: no-store`, which v4 sends here and
        // nowhere else in this fan-out.
        Some("export-markdown") => {
            let req = CoreRequest::ChatExportMarkdown { chat_id: path.0 };
            match dispatch_core(&state.0, req).await {
                Ok(CoreResponse::ChatMarkdownTranscriptPayload { filename, markdown }) => (
                    StatusCode::OK,
                    [
                        ("content-type", "text/markdown; charset=utf-8".to_string()),
                        (
                            "content-disposition",
                            build_content_disposition(&filename, Disposition::Attachment),
                        ),
                        ("cache-control", "no-store".to_string()),
                    ],
                    markdown,
                )
                    .into_response(),
                Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
                Err(r) => r,
            }
        }
        _ => chat_get_background(state, path, query).await,
    }
}

// ===========================================================================
// POST /api/v1/chats/{id} — equip | regenerate-avatar
// ===========================================================================

pub async fn chat_action_post(
    State(state): State<SharedState>,
    Path(chat_id): Path<String>,
    Query(pairs): Query<crate::query::QueryPairs>,
    body: String,
) -> AxumResponse {
    // Every query key this route reads is a v4 `searchParams.get` — FIRST wins,
    // so the pair list collapses to the map the rest of the handler expects.
    let query = crate::query::first_map(&pairs);
    let req = match query.get("action").map(String::as_str) {
        Some("equip") => {
            let body = match parse_body(&body, "Failed to equip wardrobe slot") {
                Ok(v) => v,
                Err(r) => return *r,
            };
            CoreRequest::ChatEquip { chat_id, body }
        }
        Some("regenerate-avatar") => {
            let body = match parse_body(&body, "Failed to queue avatar regeneration") {
                Ok(v) => v,
                Err(r) => return *r,
            };
            CoreRequest::ChatRegenerateAvatar { chat_id, body }
        }
        // Only these two POST actions are served on this REST edge; the other
        // chat actions ride POST /api/dispatch (a loud pointer, not a silent
        // 404 — the mount-point-action precedent).
        _ => {
            return error_json(
                StatusCode::BAD_REQUEST,
                "Only the equip and regenerate-avatar actions are served on this route; \
                 the other chat actions ride POST /api/dispatch",
            )
        }
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

#[cfg(test)]
mod include_archived_tests {
    use super::read_include_archived;
    use std::collections::HashMap;

    fn q(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// v4 `readIncludeArchived`: `raw === 'true' || raw === ''`. The bare
    /// valueless spelling counts; everything else — `1`, `TRUE`, `yes`, absent —
    /// does not. These are the SPELLINGS; the resolved boolean is what the
    /// dispatch-only surfaces are driven with in the differentials.
    #[test]
    fn only_literal_true_and_the_bare_spelling_opt_in() {
        assert!(read_include_archived(&q(&[("includeArchived", "true")])));
        assert!(read_include_archived(&q(&[("includeArchived", "")])));
        assert!(!read_include_archived(&q(&[("includeArchived", "1")])));
        assert!(!read_include_archived(&q(&[("includeArchived", "TRUE")])));
        assert!(!read_include_archived(&q(&[("includeArchived", "True")])));
        assert!(!read_include_archived(&q(&[("includeArchived", "yes")])));
        assert!(!read_include_archived(&q(&[("includeArchived", "false")])));
        assert!(!read_include_archived(&q(&[])));
        assert!(!read_include_archived(&q(&[("includearchived", "true")])));
    }
}
