//! P4.9f1 REST edges (lane F1): the wardrobe server surface. Each edge
//! dispatches the corresponding `Request` and UNWRAPS the dispatch envelope to
//! v4's RAW route body (the P4.6ah lesson).
//!
//! - `GET|POST   /api/v1/wardrobe`                 → the GLOBAL archetype tier
//!   (the route factory's collection: `?action=instructions | folders` on GET,
//!   `?action=instructions | create-folder | rename-folder | delete-folder |
//!   move-items` on POST — P4.D281, v4 `4a38222e6`)
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
use quilltap_core::api::types::WardrobeContainerScope;
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use quilltap_core::content_disposition::{build_content_disposition, Disposition};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{chat_get_background, dispatch_core, error_to_http};

/// Unwrap a wardrobe-family body to the raw route shape.
pub(crate) fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::Wardrobe(v)
        // P4.D281: the character collection's POST (create / instructions)
        // answers the character envelope.
        | CoreResponse::Character(v)
        | CoreResponse::ChatOutfit(v)
        | CoreResponse::ChatDialog(v)
        // === P4.D205 (v4 `e7d77bb60`) ===
        // The three Inform bodies leave through this fan-out too. Without them
        // every SUCCESS fell to the `_` arm and answered 500 `Unexpected core
        // response` — the P4.56 `BrahmaConsole` failure shape exactly, and
        // invisible to the route differential, which drives the HANDLER rather
        // than the HTTP edge. `success_status` is what the caller passes: 201
        // for `?action=inform` (v4's `created(...)`), 200 for `?action=informs`
        // and `?action=cancel-inform` (v4's plain `NextResponse.json`).
        | CoreResponse::ChatInform(v)
        | CoreResponse::ChatInforms(v)
        | CoreResponse::ChatInformCancelled(v)
        // P4.D256 (v4 `3ee3b1342`): the item GET's `?action=wear-history`.
        | CoreResponse::WardrobeWearHistory(v) => (
            success_status,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        // v4's `validationError(zodError)` puts BOTH keys on the wire —
        // `{error: 'Validation error', details: [issues]}` at 400
        // (`lib/api/responses.ts:108-118`, reached from the route middleware's
        // `handleRouteError`). The shared `error_to_http` renders only
        // `{error}`, so a details-bearing refusal — the Inform verbs' Zod arm —
        // is rendered here, the same way `images_routes`/`subprompts_routes`/
        // `prompt_templates_routes`/`chats_routes` already render theirs.
        CoreResponse::Error(e) => match e.validation_wire_body() {
            Some(body) => (
                StatusCode::BAD_REQUEST,
                [("content-type", "application/json")],
                body.to_string(),
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

/// v4's collection GET action map (the route factory, `4a38222e6`), in its
/// literal order: `?action=instructions` and `?action=folders`; no action is
/// the listing.
pub(crate) const COLLECTION_GET_ACTIONS: [&str; 2] = ["instructions", "folders"];

/// v4's collection POST action map, in its literal order; no action is the
/// create.
pub(crate) const COLLECTION_POST_ACTIONS: [&str; 5] = [
    "instructions",
    "create-folder",
    "rename-folder",
    "delete-folder",
    "move-items",
];

/// The folder write verb for a collection POST action (C2 — the body rides
/// RAW so the handler answers v4's flat `Validation error`).
fn folder_write_request(
    action: &str,
    scope: WardrobeContainerScope,
    container_id: Option<String>,
    body: Value,
) -> CoreRequest {
    match action {
        "create-folder" => CoreRequest::WardrobeFolderCreate {
            scope,
            container_id,
            body,
        },
        "rename-folder" => CoreRequest::WardrobeFolderRename {
            scope,
            container_id,
            body,
        },
        "delete-folder" => CoreRequest::WardrobeFolderDelete {
            scope,
            container_id,
            body,
        },
        _ => CoreRequest::WardrobeItemsMove {
            scope,
            container_id,
            body,
        },
    }
}

/// v4 `GET /api/v1/wardrobe` — the factory's collection GET for Quilltap
/// General: `?action=instructions`, `?action=folders` (`4a38222e6`), else the
/// listing. An unknown or bare `?action` is the `Unknown action` envelope.
pub async fn wardrobe_get(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // Every query key this route reads is a v4 `searchParams.get` — FIRST wins,
    // so the pair list collapses to the map the rest of the handler expects.
    let query = crate::query::first_map(&pairs);
    let action = match crate::query::dispatch_action(
        &pairs,
        &COLLECTION_GET_ACTIONS,
        "GET",
        "/api/v1/wardrobe",
    ) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let req = match action {
        Some("instructions") => CoreRequest::WardrobeInstructionsGet,
        Some(_folders) => CoreRequest::WardrobeFoldersList {
            scope: WardrobeContainerScope::General,
            container_id: None,
        },
        None => CoreRequest::WardrobeList {
            include_archived: read_include_archived(&query),
        },
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

/// v4 `POST /api/v1/wardrobe` — the factory's collection POST for Quilltap
/// General.
pub async fn wardrobe_post(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
    body: String,
) -> AxumResponse {
    collection_post(
        &state,
        &pairs,
        "/api/v1/wardrobe",
        WardrobeContainerScope::General,
        None,
        &body,
    )
    .await
}

/// The factory's collection POST behind a REST edge (Quilltap General and a
/// character's vault — R-F: project / group stay dispatch-only): the action
/// dispatch FIRST (an unknown action never reaches the body), then
/// `?action=instructions` (decoded through the Request enum so the
/// required-but-nullable key keeps its tri-state), the four folder writes
/// (`4a38222e6`), or the create (201). An unparsable body is v4's uncaught
/// `req.json()` throw — the middleware's 500.
pub(crate) async fn collection_post(
    state: &SharedState,
    pairs: &[(String, String)],
    path: &str,
    scope: WardrobeContainerScope,
    container_id: Option<String>,
    body: &str,
) -> AxumResponse {
    let action = match crate::query::dispatch_action(pairs, &COLLECTION_POST_ACTIONS, "POST", path)
    {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let body = match parse_body(body, "Internal server error") {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let (req, success) = match action {
        Some("instructions") => {
            let (tag, key) = match scope {
                WardrobeContainerScope::Character => {
                    ("characterWardrobeInstructionsSet", Some("characterId"))
                }
                _ => ("wardrobeInstructionsSet", None),
            };
            let Some(req) = quilltap_core::api::types::wardrobe_instructions_set_request(
                &body,
                tag,
                key.zip(container_id.as_deref()),
            ) else {
                return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
            };
            (req, StatusCode::OK)
        }
        Some(folder_action) => (
            folder_write_request(folder_action, scope, container_id, body),
            StatusCode::OK,
        ),
        None => (
            match scope {
                WardrobeContainerScope::Character => CoreRequest::CharacterWardrobeCreate {
                    character_id: container_id.unwrap_or_default(),
                    item: body,
                },
                _ => CoreRequest::WardrobeCreate { item: body },
            },
            // v4 `created({ wardrobeItem })` — 201, body raw.
            StatusCode::CREATED,
        ),
    };
    match dispatch_core(state, req).await {
        Ok(resp) => unwrap_to_http(resp, success),
        Err(r) => r,
    }
}

// ===========================================================================
// /api/v1/wardrobe/{itemId}
// ===========================================================================

/// v4 `GET /api/v1/wardrobe/[itemId]` — the route factory's item GET
/// (`9ecebf986`): `withActionDispatch({ 'wear-history' }, get)` runs BEFORE
/// the handler loads anything, so an unknown or bare `?action` is the
/// `Unknown action` 400 even for a MISSING item (the pre-factory route ran the
/// item's 404 first); `?action=wear-history` → the `wardrobeItemWearHistory`
/// verb at General scope (its own 404 first); no action → the item.
pub async fn wardrobe_item_get(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    const PATH: &str = "/api/v1/wardrobe/[itemId]";
    let action = match crate::query::dispatch_action(&pairs, &["wear-history"], "GET", PATH) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let request = match action {
        None => CoreRequest::WardrobeItemGet { item_id },
        Some(_wear_history) => CoreRequest::WardrobeItemWearHistory {
            scope: WardrobeContainerScope::General,
            container_id: None,
            item_id,
        },
    };
    match dispatch_core(&state, request).await {
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

// === P4.D283: preview-avatar (owned) ===
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
        Ok(resp) => match preview_avatar_status_and_body(resp) {
            Some((status, body)) => (
                status,
                [("content-type", "application/json")],
                body.to_string(),
            )
                .into_response(),
            None => unwrap_to_http(
                CoreResponse::Error(quilltap_core::api::types::CoreError {
                    kind: quilltap_core::api::types::ErrorKind::Internal,
                    message: "Unexpected core response".to_string(),
                    pepper_state: None,
                    code: None,
                    associations: None,
                    character_id: None,
                    entity: None,
                    details: None,
                    already_saved: None,
                }),
                StatusCode::OK,
            ),
        },
        Err(r) => r,
    }
}

/// v4 `preview-avatar/route.ts` (`9ecebf986`): the HTTP status and body for
/// the preview's dispatch answer. A generation failure is `errorResponse(
/// message, refused ? 422 : 502, { trail, refused })` — the core carries the
/// refusal as `Unprocessable` and the provider failure as `Internal` with
/// `details.refused === false` (the images route's idiom for its own 502),
/// both with the `details` bag on the body; every other error is the ordinary
/// status map with `{ error }`. `None` for a response variant the preview
/// never answers. `pub` so the routes differential pins the edge's mapping.
pub fn preview_avatar_status_and_body(resp: CoreResponse) -> Option<(StatusCode, Value)> {
    use quilltap_core::api::types::ErrorKind;
    match resp {
        CoreResponse::Wardrobe(v) => Some((StatusCode::OK, v)),
        CoreResponse::Error(e) => {
            let provider_failed = e.kind == ErrorKind::Internal
                && e.details
                    .as_deref()
                    .and_then(|d| d.get("refused"))
                    .and_then(Value::as_bool)
                    == Some(false);
            let status = match e.kind {
                _ if provider_failed => StatusCode::BAD_GATEWAY,
                ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
                ErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
                ErrorKind::Forbidden => StatusCode::FORBIDDEN,
                ErrorKind::NotFound => StatusCode::NOT_FOUND,
                ErrorKind::Conflict => StatusCode::CONFLICT,
                ErrorKind::Unprocessable => StatusCode::UNPROCESSABLE_ENTITY,
                ErrorKind::Locked | ErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
                ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let mut body = serde_json::json!({ "error": e.message });
            if let Some(d) = e.details {
                body["details"] = *d;
            }
            Some((status, body))
        }
        _ => None,
    }
}
// === end P4.D283 ===

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
    // v4 `68da64d9b` put the chat GET's old `if (action === '…')` ladder (whose
    // fallthrough served the full chat payload for absent / `?action=` /
    // unknown alike) behind `dispatchAction` with the FOURTEEN-key thunk map:
    // a bare or unknown action is now the `Unknown action` envelope, answered
    // BEFORE any chat read (v4's test asserts `findById` is not called — so an
    // unknown action on a MISSING chat is 400, never 404). The absent action is
    // v4's full chat payload, which v5 serves over `/api/dispatch` only: it
    // keeps the loud pointer in `chat_get_background` (a recorded divergence,
    // pinned both ways in `query_param_semantics_equivalence`). (NB `68da64d9b`'s
    // message says it "adds `dispatchAction`" — its diff never touches the
    // middleware; the primitive is `ad1c4c37f`'s.)
    let action = match crate::query::dispatch_action(
        &query.0,
        CHAT_GET_ACTIONS,
        "GET",
        "/api/v1/chats/[id]",
    ) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    match action {
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
        // === P4.D205 (v4 `e7d77bb60`) ===
        // `GET ?action=informs` — the pending batches for the composer's chip.
        Some("informs") => {
            let req = CoreRequest::ChatInformsList { chat_id: path.0 };
            match dispatch_core(&state.0, req).await {
                Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
                Err(r) => r,
            }
        }
        // === end P4.D205 ===
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
        // `get-background`, `cost`, the absent action and the v4-known actions
        // v5 serves only over dispatch — the pointer lives there.
        _ => chat_get_background(state, path, query).await,
    }
}

/// v4's chat-GET thunk map, in literal order (`chats/[id]/handlers/get.ts:60-82`
/// at `68da64d9b` — the old if-ladder's order): the `availableActions` of the
/// unknown/bare refusal.
const CHAT_GET_ACTIONS: &[&str] = &[
    "export",
    "export-markdown",
    "get-avatars",
    "get-state",
    "outfit",
    "outfit-summary",
    "photo-albums",
    "informs",
    "group-stores",
    "mailbox",
    "accessible-stores",
    "get-background",
    "gallery",
    "cost",
];

/// v4's chat-POST thunk map, in literal order (`chats/[id]/handlers/post.ts` at
/// `ad1c4c37f` — the retired `CHAT_POST_ACTIONS` constant's order, which the
/// map kept): the `availableActions` of both refusals. v4 has NO fallback here.
const CHAT_POST_ACTIONS: &[&str] = &[
    "regenerate-title",
    "rebuild-summary",
    "add-tag",
    "remove-tag",
    "impersonate",
    "set-active-speaker",
    "turn",
    "add-participant",
    "update-participant",
    "remove-participant",
    "rebuild-system-prompt",
    "bulk-reattribute",
    "set-avatar",
    "remove-avatar",
    "add-tool-result",
    "queue-memories",
    "extract-memories-dry-run",
    "recall-replay",
    "update-tool-settings",
    "rng",
    "run-tool",
    "toggle-agent-mode",
    "regenerate-background",
    // P4.D228 (v4 `ce2f1dabf`, #77): the 24th key, between
    // `regenerate-background` and `reclassify-danger` — served over dispatch
    // (`chatRetryImageUncensored`); the REST arm answers the pointer.
    "retry-image-uncensored",
    "reclassify-danger",
    "equip",
    "toggle-avatar-generation",
    "regenerate-avatar",
    "render-conversation",
    "active-document",
    "open-documents",
    "recent-documents",
    "open-document",
    "close-document",
    "read-document",
    "resolve-document",
    "write-document",
    "rename-document",
    "delete-document",
    "announcement",
    "inform",
    "cancel-inform",
    "announcement-preview",
    "impersonation-voice-preview",
    "send-mail",
    "merge-conversation",
    "scenario",
    "save-image",
];

// ===========================================================================
// POST /api/v1/chats/{id} — equip | regenerate-avatar
// ===========================================================================

pub async fn chat_action_post(
    State(state): State<SharedState>,
    Path(chat_id): Path<String>,
    Query(pairs): Query<crate::query::QueryPairs>,
    body: String,
) -> AxumResponse {
    // v4 `dispatchAction(req, { …forty-eight… })` with NO fallback
    // (`ad1c4c37f`): absent → `Action parameter required`, bare / unknown →
    // `Unknown action`, answered here byte-for-byte. v4 looks the chat up
    // FIRST (`notFound('Chat')`); this edge gates without a lookup — the
    // recorded gate-ORDER divergence (`query_param_semantics_equivalence`).
    let action = match crate::query::dispatch_required_action(
        &pairs,
        CHAT_POST_ACTIONS,
        "POST",
        "/api/v1/chats/[id]",
    ) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let req = match action {
        "equip" => {
            let body = match parse_body(&body, "Failed to equip wardrobe slot") {
                Ok(v) => v,
                Err(r) => return *r,
            };
            CoreRequest::ChatEquip { chat_id, body }
        }
        "regenerate-avatar" => {
            let body = match parse_body(&body, "Failed to queue avatar regeneration") {
                Ok(v) => v,
                Err(r) => return *r,
            };
            CoreRequest::ChatRegenerateAvatar { chat_id, body }
        }
        // === P4.D205 (v4 `e7d77bb60`) ===
        // The two Inform POST actions. Both bodies carry tri-state keys, so
        // they go through the SHARED envelope decoder rather than a hand-built
        // variant — the `tri_state_edges_share_the_decoder` census forbids the
        // second spelling, and P4.98's measured drift (an explicit `null`
        // answering two different things on the two transports) is why.
        "inform" => {
            let parsed: serde_json::Value =
                serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            match crate::request_envelope::request_envelope(
                "chatInform",
                &parsed,
                // P4.D249 (v4 `52d6e7ecd`): `permanent` rides the same
                // tri-state as its siblings, so `null` is v4's Zod 400 here too.
                &["contentMarkdown", "targetParticipantIds", "permanent"],
                &[("chatId", serde_json::Value::String(chat_id))],
            ) {
                Some(r) => {
                    // v4 answers 201 for a posted inform.
                    return match dispatch_core(&state, r).await {
                        Ok(resp) => unwrap_to_http(resp, StatusCode::CREATED),
                        Err(r) => r,
                    };
                }
                None => {
                    return error_json(StatusCode::BAD_REQUEST, "Validation error");
                }
            }
        }
        "cancel-inform" => {
            let parsed: serde_json::Value =
                serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            match crate::request_envelope::request_envelope(
                "chatInformCancel",
                &parsed,
                &["batchId"],
                &[("chatId", serde_json::Value::String(chat_id))],
            ) {
                Some(r) => r,
                None => {
                    return error_json(StatusCode::BAD_REQUEST, "Validation error");
                }
            }
        }
        // === end P4.D205 ===
        // Only these POST actions are served on this REST edge; the other
        // v4-known chat actions ride POST /api/dispatch (a loud pointer, not a
        // silent 404 — the mount-point-action precedent — and never the
        // `Unknown action` envelope, which would list the action it refuses).
        _ => {
            return error_json(
                StatusCode::BAD_REQUEST,
                "Only the equip, regenerate-avatar, inform and cancel-inform actions are \
                 served on this route; the other chat actions ride POST /api/dispatch",
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
