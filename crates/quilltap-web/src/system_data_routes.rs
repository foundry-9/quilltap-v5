//! P4.9G1 REST edges — the Data & System server surface's v4-URL parity routes.
//! Each edge dispatches the matching `Request` and unwraps the dispatch envelope
//! to v4's RAW route body (the `Response::System(Value)` carrier). The JSON verbs
//! also ride `POST /api/dispatch`; these edges give v4-URL parity for the
//! Settings "Data & System" tab.
//!
//! - `GET  /api/v1/system/tools?action=tasks-queue|job-concurrency|delete-data-preview|memory-dedup-preview`
//! - `POST /api/v1/system/tools?action=tasks-queue|job-concurrency|delete-data|memory-dedup`
//! - `GET  /api/v1/system/jobs/{id}`
//! - `DELETE /api/v1/system/jobs/{id}`
//! - `POST /api/v1/system/jobs/{id}?action=pause|resume`
//! - `GET|POST /api/v1/system/jobs` — the COLLECTION (P4.9G3; web-edge-only)
//! - `POST /api/v1/system/unlock?action=change-passphrase` (P4.9G3; the alias)
//!
//! The export/import/backup/restore edges (streaming NDJSON, multipart, byte
//! legs) land in the sibling P4.9G4 / P4.9G5 lanes.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use serde_json::{json, Value};

use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// Unwrap a `Response::System(Value)` to the raw route body at `status`.
/// Re-exported for the P4.9G5 backup edges, which answer the same envelope.
pub fn system_body_public(resp: CoreResponse, status: StatusCode) -> AxumResponse {
    system_body(resp, status)
}

/// Unwrap a `Response::System(Value)` to the raw route body at `status`.
fn system_body(resp: CoreResponse, status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::System(v) => (
            status,
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

async fn dispatch_system(
    state: &SharedState,
    req: CoreRequest,
    status: StatusCode,
) -> AxumResponse {
    match dispatch_core(state, req).await {
        Ok(resp) => system_body(resp, status),
        Err(r) => r,
    }
}

/// v4 `TOOLS_GET_ACTIONS` / `TOOLS_POST_ACTIONS` (`app/api/v1/system/tools/
/// route.ts`) — the whole lists, and the source of the 400's tail. v5 does not
/// serve `capabilities-report-progress` (GET) or `ai-import-stream` (POST);
/// they stay in the tail because the tail is v4's `join(', ')` over the
/// CONSTANT, not over what this edge dispatches.
const TOOLS_GET_ACTIONS: &[&str] = &[
    "tasks-queue",
    "job-concurrency",
    "delete-data-preview",
    "export-entities",
    "export-preview",
    "capabilities-report-list",
    "capabilities-report-progress",
    "capabilities-report-get",
    "memory-dedup-preview",
];

const TOOLS_POST_ACTIONS: &[&str] = &[
    "delete-data",
    "tasks-queue",
    "job-concurrency",
    "export",
    "import-preview",
    "import-execute",
    "capabilities-report-generate",
    "capabilities-report-delete",
    "memory-dedup",
    "ai-import-stream",
];

/// v4's `system/tools` refusal: `isValidAction` with NO `!action` carve-out, so
/// an ABSENT action reaches the same sentence and interpolates as the literal
/// `null` — while `?action=` interpolates as the empty string. The two must
/// stay distinguishable, which is why this takes `Option`.
fn tools_unknown_action(action: Option<&str>, verb: &str, available: &[&str]) -> AxumResponse {
    // A v4-KNOWN action this edge does not serve (`capabilities-report-progress`
    // on GET, `ai-import-stream` on POST — both ride `/api/dispatch` in v5) must
    // not be refused as "unknown" by a sentence that lists it as available. The
    // §3 unification review put the loud, honest refusal back; the divergence is
    // RECORDED in `query_param_semantics_equivalence` (`UNSERVED_KNOWN_ACTIONS`).
    if let Some(a) = action.filter(|a| available.contains(a)) {
        return error_json(
            StatusCode::BAD_REQUEST,
            &format!("The '{a}' action is not served on this route; it rides POST /api/dispatch"),
        );
    }
    error_json(
        StatusCode::BAD_REQUEST,
        &format!(
            "Unknown action: {}. Available {verb} actions: {}",
            action.unwrap_or("null"),
            available.join(", ")
        ),
    )
}

fn unknown_action(action: &str) -> AxumResponse {
    error_json(
        StatusCode::BAD_REQUEST,
        &format!("Unknown action: {action}"),
    )
}

/// v4's `validationError(zodError)` — `{error:'Validation error', details:[…]}`
/// at 400, with the RAW Zod issue objects as wire payload
/// (`lib/api/responses.ts:108`). `error_json` cannot express the second key.
fn validation_error(issues: Value) -> AxumResponse {
    (
        StatusCode::BAD_REQUEST,
        [("content-type", "application/json")],
        json!({ "error": "Validation error", "details": issues }).to_string(),
    )
        .into_response()
}

/// Zod 4's `received` label for a value that failed an `invalid_type` check.
pub(crate) fn zod_received(v: Option<&Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

/// v4's `jobConcurrencySchema = z.object({ concurrency: z.number().int().min(1).max(32) })`,
/// transcribed issue-for-issue from Zod 4's own output (the
/// `system-body-guards` oracle's `concurrency_*` arms are the transcription's
/// proof). Zod stops at the first failing check on the field, so at most one
/// issue is ever produced.
// `Box`ed like `files_routes::db_and_backend`'s refusal — an `AxumResponse` is
// a 128-byte `Err` variant clippy rightly refuses to carry by value.
fn parse_job_concurrency(body: &Value) -> Result<i64, Box<AxumResponse>> {
    let raw = body.get("concurrency");
    let Some(n) = raw.and_then(Value::as_f64) else {
        return Err(Box::new(validation_error(json!([{
            "expected": "number",
            "code": "invalid_type",
            "path": ["concurrency"],
            "message": format!("Invalid input: expected number, received {}", zod_received(raw)),
        }]))));
    };
    // `.int()` is Zod 4's `safeint` format check, reported as its own
    // `invalid_type` with `expected: "int"`.
    if n.fract() != 0.0 || !n.is_finite() || n.abs() > 9_007_199_254_740_991.0 {
        return Err(Box::new(validation_error(json!([{
            "expected": "int",
            "format": "safeint",
            "code": "invalid_type",
            "path": ["concurrency"],
            "message": "Invalid input: expected int, received number",
        }]))));
    }
    if n < 1.0 {
        return Err(Box::new(validation_error(json!([{
            "origin": "number",
            "code": "too_small",
            "minimum": 1,
            "inclusive": true,
            "path": ["concurrency"],
            "message": "Too small: expected number to be >=1",
        }]))));
    }
    if n > 32.0 {
        return Err(Box::new(validation_error(json!([{
            "origin": "number",
            "code": "too_big",
            "maximum": 32,
            "inclusive": true,
            "path": ["concurrency"],
            "message": "Too big: expected number to be <=32",
        }]))));
    }
    Ok(n as i64)
}

/// Zod 4's `z.uuid()` regex, transcribed byte-for-byte from
/// `zod/v4/core/regexes.cjs`:
///
/// ```text
/// ^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}
///  |00000000-0000-0000-0000-000000000000
///  |ffffffff-ffff-ffff-ffff-ffffffffffff)$
/// ```
///
/// It is RFC 9562-strict about the version nibble (`[1-8]`) and the variant
/// nibble (`[89abAB]`) where `Uuid::parse_str` is not, and its nil/max escape
/// hatches are LOWERCASE literals — so an uppercase max UUID is refused while
/// the lowercase one is accepted. The `progressid_gate_*` arms measure the
/// whole accept-set against Zod itself.
pub fn zod_uuid(s: &str) -> bool {
    if s == "00000000-0000-0000-0000-000000000000" || s == "ffffffff-ffff-ffff-ffff-ffffffffffff" {
        return true;
    }
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, ch) in b.iter().enumerate() {
        let ok = match i {
            8 | 13 | 18 | 23 => *ch == b'-',
            14 => (b'1'..=b'8').contains(ch),
            19 => matches!(ch, b'8' | b'9' | b'a' | b'b' | b'A' | b'B'),
            _ => ch.is_ascii_hexdigit(),
        };
        if !ok {
            return false;
        }
    }
    true
}

/// Unwrap a `Response::MemoryMaintenance(Value)` (the memory-dedup carrier) to
/// v4's raw route body at 200 (P4.43).
async fn dispatch_memory_maintenance(state: &SharedState, req: CoreRequest) -> AxumResponse {
    match dispatch_core(state, req).await {
        Ok(CoreResponse::MemoryMaintenance(v)) => (
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

// ── GET /api/v1/system/tools ─────────────────────────────────────────────────

pub async fn system_tools_get(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // Every query key this route reads is a v4 `searchParams.get` — FIRST wins.
    let q = crate::query::first_map(&pairs);
    let action = crate::query::first(&pairs, "action");
    match action.unwrap_or("") {
        "tasks-queue" => {
            dispatch_system(&state, CoreRequest::SystemTasksQueue, StatusCode::OK).await
        }
        "job-concurrency" => {
            dispatch_system(&state, CoreRequest::SystemJobConcurrencyGet, StatusCode::OK).await
        }
        "delete-data-preview" => {
            dispatch_system(&state, CoreRequest::SystemDeleteDataPreview, StatusCode::OK).await
        }
        "export-entities" => {
            let Some(entity_type) = q.get("type").filter(|s| !s.is_empty()).cloned() else {
                return error_json(StatusCode::BAD_REQUEST, "Missing type parameter");
            };
            dispatch_system(
                &state,
                CoreRequest::SystemExportEntities { entity_type },
                StatusCode::OK,
            )
            .await
        }
        // === P4.37: The Almanack — v4's frozen action names ===
        "capabilities-report-list" => {
            dispatch_system(&state, CoreRequest::SystemAlmanackList, StatusCode::OK).await
        }
        "capabilities-report-get" => {
            let Some(report_id) = q.get("reportId").filter(|s| !s.is_empty()).cloned() else {
                return error_json(StatusCode::BAD_REQUEST, "Missing reportId parameter");
            };
            let download = q.get("download").map(String::as_str) == Some("true");
            let resp = match dispatch_core(
                &state,
                CoreRequest::SystemAlmanackGet {
                    report_id: report_id.clone(),
                },
            )
            .await
            {
                Ok(r) => r,
                Err(r) => return r,
            };
            if !download {
                return system_body(resp, StatusCode::OK);
            }
            // v4's download leg is the SAME action with `download=true`: the
            // markdown bytes as an attachment rather than the JSON envelope.
            let CoreResponse::System(body) = resp else {
                return system_body(resp, StatusCode::OK);
            };
            let content = body
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let filename = body
                .get("filename")
                .and_then(Value::as_str)
                .unwrap_or("almanack.md")
                .to_string();
            let bytes = content.into_bytes();
            let len = bytes.len();
            (
                StatusCode::OK,
                [
                    ("content-type", "text/markdown".to_string()),
                    (
                        "content-disposition",
                        // v4 writes a bare `attachment; filename="…"` here.
                        // The generated name is always ASCII, so the shared
                        // builder's RFC 5987 arm is never reached.
                        quilltap_core::content_disposition::build_content_disposition(
                            &filename,
                            quilltap_core::content_disposition::Disposition::Attachment,
                        ),
                    ),
                    ("content-length", len.to_string()),
                ],
                bytes,
            )
                .into_response()
        }
        // === end P4.37 ===
        "export-preview" => {
            let Some(entity_type) = q.get("type").filter(|s| !s.is_empty()).cloned() else {
                return error_json(StatusCode::BAD_REQUEST, "Missing required parameter: type");
            };
            let scope = q.get("scope").cloned();
            let selected_ids = q
                .get("selectedIds")
                .map(|s| {
                    s.split(',')
                        .filter(|p| !p.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let include_memories = q.get("includeMemories").map(String::as_str) == Some("true");
            dispatch_system(
                &state,
                CoreRequest::SystemExportPreview {
                    entity_type,
                    scope,
                    selected_ids,
                    include_memories,
                },
                StatusCode::OK,
            )
            .await
        }
        // === P4.43: memory-dedup preview (v4's `?action=memory-dedup-preview`) ===
        "memory-dedup-preview" => {
            // v4: `thresholdParam ? parseFloat(thresholdParam) : 0.80`.
            // Absent/empty → default (None). Present-and-numeric → `Some`;
            // present-non-numeric → `Some(NaN)` so the handler answers v4's exact
            // 400. (Rust's `parse` is stricter than JS `parseFloat` on trailing
            // garbage like "0.8x" — a documented minor edge; the SPA rides the
            // dispatch verb, not this parity edge.)
            let threshold = q
                .get("threshold")
                .filter(|s| !s.is_empty())
                .map(|s| s.parse::<f64>().unwrap_or(f64::NAN));
            dispatch_memory_maintenance(&state, CoreRequest::MemoryDedupPreview { threshold }).await
        }
        _ => tools_unknown_action(action, "GET", TOOLS_GET_ACTIONS),
    }
}

// ── POST /api/v1/system/tools ────────────────────────────────────────────────

pub async fn system_tools_post(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
    // P4.9G4: the import legs need the raw headers so they can re-drive the
    // multipart parser over the buffered body (v4 branches on `content-type`).
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> AxumResponse {
    // `action` is the only key this verb reads off the URL (every other value
    // comes out of the body); v4 reads it with `searchParams.get` — FIRST wins.
    let action = crate::query::first(&pairs, "action");
    // v4 reads the body with `await req.json()` INSIDE each handler's own try,
    // so a malformed body is not a 400 — it escapes to that handler's catch as
    // a 500 carrying the leg's own sentence. `job-concurrency` alone survives
    // it, because its read is `req.json().catch(() => ({}))`. Keeping the parse
    // failure distinct from a `null` body is what lets each arm answer its own
    // way; `Value::Null` would have collapsed the two.
    let parsed: Option<Value> = serde_json::from_slice(&body).ok();
    let body_or_null = parsed.clone().unwrap_or(Value::Null);
    match action.unwrap_or("") {
        "tasks-queue" => {
            let Some(parsed) = parsed.as_ref() else {
                return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to control queue");
            };
            let Some(control) = parsed.get("action").and_then(Value::as_str) else {
                return error_json(
                    StatusCode::BAD_REQUEST,
                    "Invalid action. Must be \"start\" or \"stop\"",
                );
            };
            dispatch_system(
                &state,
                CoreRequest::SystemTasksQueueControl {
                    action: control.to_string(),
                },
                StatusCode::OK,
            )
            .await
        }
        // === P4.37: The Almanack ===
        "capabilities-report-generate" => {
            // v4 zod-parses `{progressId?: uuid}` and treats a missing/invalid
            // body as "untracked" rather than an error (the card only sends one
            // when it is watching) — so a non-UUID progressId must fall back to
            // untracked too, not ride through (§3 review, 2026-08-06). The
            // malformed-body arm lands here as well: v4's `req.json()` for this
            // action sits in its OWN inner try whose catch leaves progressId null.
            //
            // [P4.62] The gate used to be `len() == 36 && Uuid::parse_str(..).is_ok()`,
            // which accepts UUIDs whose version/variant nibbles Zod 4 refuses —
            // measured, not reasoned about (`progressid_gate_bad_version` /
            // `_bad_variant`). It is Zod's own regex now.
            let progress_id = body_or_null
                .get("progressId")
                .and_then(Value::as_str)
                .filter(|s| zod_uuid(s))
                .map(str::to_string);
            dispatch_system(
                &state,
                CoreRequest::SystemAlmanackGenerate { progress_id },
                StatusCode::OK,
            )
            .await
        }
        "capabilities-report-delete" => {
            let Some(parsed) = parsed.as_ref() else {
                return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete report");
            };
            // [P4.62] v4's gate is `if (!reportId)` — JS FALSINESS, not
            // "is it a string". So `0`, `''` and `false` are missing exactly as
            // `null` and an absent key are, while a TRUTHY non-string (`true`,
            // `123`, `{}`, `[]`) passes it and then fails `f.id === reportId`,
            // which no string id can ever satisfy — v4 answers **404 Report not
            // found**, not 400. Collapsing on `as_str` answered 400 for all of
            // them.
            //
            // A truthy non-string is forwarded as `""`, which reaches the same
            // `find_report_entry` read (so a DB failure still surfaces as v4's
            // 500) and cannot match a minted id — v4's `===` outcome exactly.
            let raw = parsed.get("reportId");
            if !quilltap_core::api::system_qtap::js_truthy(raw) {
                return error_json(StatusCode::BAD_REQUEST, "Missing reportId");
            }
            let report_id = raw.and_then(Value::as_str).unwrap_or("").to_string();
            dispatch_system(
                &state,
                CoreRequest::SystemAlmanackDelete { report_id },
                StatusCode::OK,
            )
            .await
        }
        // === end P4.37 ===
        "job-concurrency" => {
            // v4 body `{concurrency}`; the verb field is `maxConcurrentJobs`.
            //
            // [P4.62] This is the one leg whose body read is
            // `req.json().catch(() => ({}))`, so a malformed body is an empty
            // object and answers the SAME 400 an absent key does. And the 400 is
            // `validationError(parsed.error)` — v4's two-key envelope carrying
            // the raw Zod issues — not the flat invented sentence this used to
            // answer. Validating here (rather than leaning on the core's
            // `validate_concurrency`, which the dispatch verb still uses) also
            // puts the refusal ahead of the pump-readiness check, where v4 has it.
            let value = match parse_job_concurrency(&body_or_null) {
                Ok(v) => v,
                Err(resp) => return *resp,
            };
            dispatch_system(
                &state,
                CoreRequest::SystemJobConcurrencySet {
                    max_concurrent_jobs: value,
                },
                StatusCode::OK,
            )
            .await
        }
        // ── P4.9G4 ──
        "export" => crate::qtap_routes::export_download(&state, &body).await,
        "import-preview" => {
            crate::qtap_routes::import_preview(&state, &headers, body.clone()).await
        }
        "import-execute" => {
            crate::qtap_routes::import_execute(&state, &headers, body.clone()).await
        }
        // ── end P4.9G4 ──
        "delete-data" => {
            let Some(parsed) = parsed.as_ref() else {
                return error_json(StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete data");
            };
            let confirm = parsed
                .get("confirm")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            dispatch_system(
                &state,
                CoreRequest::SystemDeleteData {
                    confirm,
                    // v4 `d553f72a`: absent keeps the archive bundles; only an
                    // explicit `false` wipes them with the rest.
                    keep_archived_character_bundles: parsed
                        .get("keepArchivedCharacterBundles")
                        .and_then(Value::as_bool),
                },
                StatusCode::OK,
            )
            .await
        }
        // === P4.43: memory-dedup apply (v4's `?action=memory-dedup`) ===
        "memory-dedup" => {
            let Some(parsed) = parsed.as_ref() else {
                return error_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to deduplicate memories",
                );
            };
            // v4: `typeof thresholdParam === 'number' ? thresholdParam : 0.80`
            // — v4's OWN coercion, so the collapse is faithful: absent, null and
            // every wrong-typed value become the default, and only a real number
            // reaches the range gate.
            let threshold = parsed.get("threshold").and_then(Value::as_f64);
            dispatch_memory_maintenance(&state, CoreRequest::MemoryDedupRun { threshold }).await
        }
        _ => tools_unknown_action(action, "POST", TOOLS_POST_ACTIONS),
    }
}

// ── /api/v1/system/conversation-summaries (P4.43) ────────────────────────────
//
// v4 `app/api/v1/system/conversation-summaries/route.ts` — only `?action=regenerate`
// is recognized; anything else answers v4's `badRequest('Unknown or missing action.')`.

pub async fn system_conversation_summaries_get(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // Every query key this route reads is a v4 `searchParams.get` — FIRST wins,
    // so the pair list collapses to the map the rest of the handler expects.
    let q = crate::query::first_map(&pairs);
    if q.get("action").map(String::as_str) != Some("regenerate") {
        return error_json(StatusCode::BAD_REQUEST, "Unknown or missing action.");
    }
    dispatch_memory_maintenance(&state, CoreRequest::ConversationSummariesRegenerateStatus).await
}

pub async fn system_conversation_summaries_post(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // Every query key this route reads is a v4 `searchParams.get` — FIRST wins,
    // so the pair list collapses to the map the rest of the handler expects.
    let q = crate::query::first_map(&pairs);
    if q.get("action").map(String::as_str) != Some("regenerate") {
        return error_json(StatusCode::BAD_REQUEST, "Unknown or missing action.");
    }
    dispatch_memory_maintenance(&state, CoreRequest::ConversationSummariesRegenerate).await
}

// ── /api/v1/system/jobs/{id} ─────────────────────────────────────────────────

pub async fn system_job_get(
    State(state): State<SharedState>,
    Path(job_id): Path<String>,
) -> AxumResponse {
    dispatch_system(&state, CoreRequest::SystemJobGet { job_id }, StatusCode::OK).await
}

pub async fn system_job_delete(
    State(state): State<SharedState>,
    Path(job_id): Path<String>,
) -> AxumResponse {
    dispatch_system(
        &state,
        CoreRequest::SystemJobDelete { job_id },
        StatusCode::OK,
    )
    .await
}

pub async fn system_job_post(
    State(state): State<SharedState>,
    Path(job_id): Path<String>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // Every query key this route reads is a v4 `searchParams.get` — FIRST wins,
    // so the pair list collapses to the map the rest of the handler expects.
    let q = crate::query::first_map(&pairs);
    let action = q.get("action").cloned().unwrap_or_default();
    dispatch_system(
        &state,
        CoreRequest::SystemJobControl { job_id, action },
        StatusCode::OK,
    )
    .await
}

// ── P4.9G3 ──────────────────────────────────────────────────────────────────
//
// `/api/v1/system/jobs` (the COLLECTION) and the change-passphrase alias are
// **web-edge-only** legs: the §1 wire surface (FROZEN this round) has no verb
// for the jobs collection, and the passphrase change already has one
// (`Request::ChangePassphrase`) that this alias simply re-exposes at v4's URL.
// The collection edge therefore reaches `api::system_data`'s free functions
// directly over `host.core().db()` — the established raw-edge pattern
// (`files_routes` / `terminal_routes`) — plus the host job-pump control.

use quilltap_core::api::system_data::{self, JobPumpControl, ProcessorStatus};
use quilltap_core::api::SINGLE_USER_ID;
use quilltap_core::db::runtime::Db;
use std::sync::Arc;

/// The Db + job pump, or the loud typed refusal (a locked engine / a read-only
/// embedder with no cadence assembled — the same refusal the dispatch
/// tasks-queue arms answer).
/// (The `Err` is boxed: an `AxumResponse` dwarfs the `Ok` tuple — clippy
/// `result_large_err`.)
fn db_and_pump(state: &SharedState) -> Result<(Db, Arc<dyn JobPumpControl>), Box<AxumResponse>> {
    let Some(host) = state.host() else {
        return Err(Box::new(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "Server is not running",
        )));
    };
    let Some(db) = host.core().db() else {
        return Err(Box::new(error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "Database is locked",
        )));
    };
    let Some(pump) = host.core().job_pump_control() else {
        return Err(Box::new(error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "job pump not assembled (job-pump-control seam deferral)",
        )));
    };
    Ok((db, pump))
}

/// v4 `GET /api/v1/system/jobs` (`system/jobs/route.ts:23`) — queue stats +
/// the by-kind activity snapshot + the processor snapshot, with the optional
/// `includeByType=true` (per-type active counts), `includeJobs=true` (50 newest)
/// and `chatId` (pending-for-chat) legs. v4 calls
/// `ensureProcessorRunning()` first; here that is the pump's idempotent `start`.
pub async fn system_jobs_collection_get(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // Every query key this route reads is a v4 `searchParams.get` — FIRST wins,
    // so the pair list collapses to the map the rest of the handler expects.
    let q = crate::query::first_map(&pairs);
    let (db, pump) = match db_and_pump(&state) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    pump.start();
    let include_jobs = q.get("includeJobs").map(String::as_str) == Some("true");
    // The raw flag; v4's `|| includeJobs` widening lives in the core fn.
    let include_by_type = q.get("includeByType").map(String::as_str) == Some("true");
    let chat_id = q.get("chatId").filter(|s| !s.is_empty()).cloned();
    let status: ProcessorStatus = pump.status();
    system_body(
        system_data::jobs_list(
            &db,
            SINGLE_USER_ID,
            include_jobs,
            include_by_type,
            chat_id.as_deref(),
            &status,
        ),
        StatusCode::OK,
    )
}

/// v4 `POST /api/v1/system/jobs` (`route.ts:71`) — enqueue a job; **201** on
/// success. The type gate and the payload-must-be-an-object gate live in the
/// core fn (v4's `BackgroundJobTypeEnum.safeParse` + the explicit check).
pub async fn system_jobs_collection_post(
    State(state): State<SharedState>,
    body: axum::body::Bytes,
) -> AxumResponse {
    let (db, pump) = match db_and_pump(&state) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let parsed: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let job_type = parsed
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let payload = parsed.get("payload").cloned().unwrap_or(Value::Null);
    // v4: `typeof priority === 'number' ? priority : undefined`.
    let priority = parsed.get("priority").and_then(Value::as_f64);
    let max_attempts = parsed.get("maxAttempts").and_then(Value::as_f64);

    let resp = system_data::jobs_enqueue_now(
        &db,
        SINGLE_USER_ID,
        &job_type,
        &payload,
        priority,
        max_attempts,
    )
    .await;
    // v4 nudges the processor before enqueueing; either order is equivalent for
    // an idempotent start, and doing it after means a rejected body never wakes
    // the pump.
    if matches!(resp, CoreResponse::System(_)) {
        pump.start();
    }
    system_body(resp, StatusCode::CREATED)
}

/// v4 `POST /api/v1/system/unlock?action=change-passphrase`
/// (`system/unlock/route.ts:318`) — the REST alias over the existing
/// `Request::ChangePassphrase` verb (which already reproduces v4's messages and
/// status kinds). Body `{oldPassphrase, newPassphrase}` → `{success:true}`.
///
/// **Scope, named:** only this one action is aliased. v4's four sibling actions
/// (`setup` / `unlock` / `store` / `lock`) all have dispatch verbs the SPA uses;
/// they get no REST alias in this lane and answer `unknown_action` here.
pub async fn system_unlock_post(
    State(state): State<SharedState>,
    Query(pairs): Query<crate::query::QueryPairs>,
    body: axum::body::Bytes,
) -> AxumResponse {
    // [P4.62] v4's POST gates in three steps, in this order
    // (`system/unlock/route.ts:75-119`): an ABSENT action gets its own long
    // sentence naming all five, an unrecognized one gets `Unknown action: X`,
    // and only then is the body read — by `parseRequestBody`, which refuses a
    // malformed body and any JSON that is not a plain object. v5 had neither the
    // absent-action sentence (it answered `Unknown action: ` with an empty name)
    // nor the body gate at all, so a body of `42` or `[]` rode straight through
    // to a passphrase change with two empty strings.
    let Some(action) = crate::query::action(&pairs) else {
        return error_json(
            StatusCode::BAD_REQUEST,
            "Missing action parameter. Use ?action=setup, ?action=unlock, \
             ?action=store, ?action=change-passphrase, or ?action=lock",
        );
    };
    // Scope, unchanged from P4.9G3: only `change-passphrase` is aliased here, so
    // v4's four siblings answer `unknown_action` — a RECORDED narrowing (they
    // all have dispatch verbs the SPA uses), not this lane's doing.
    if action != "change-passphrase" {
        return unknown_action(action);
    }
    let Ok(parsed) = serde_json::from_slice::<Value>(&body) else {
        return error_json(StatusCode::BAD_REQUEST, "Invalid JSON body");
    };
    // v4: `!body || typeof body !== 'object' || Array.isArray(body)`.
    if !parsed.is_object() {
        return error_json(
            StatusCode::BAD_REQUEST,
            "Request body must be a JSON object",
        );
    }
    // v4 `typeof body.oldPassphrase === 'string' ? body.oldPassphrase : ''` — the
    // collapse IS v4's coercion, so absent / null / wrong-typed all mean "".
    let str_field = |key: &str| {
        parsed
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let req = CoreRequest::ChangePassphrase {
        old_passphrase: str_field("oldPassphrase"),
        new_passphrase: str_field("newPassphrase"),
    };
    match dispatch_core(&state, req).await {
        // [P4.D65] v4 answers `{success: true, archives}` — the archive
        // re-encryption sweep's summary, which the settings card reads to name
        // the bundles left holding the old passphrase. It was a bare
        // `{"success":true}` here while the sweep had no caller.
        Ok(CoreResponse::ChangePassphrase(dto)) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            serde_json::to_string(&dto).unwrap_or_else(|_| "{\"success\":true}".to_string()),
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
// ── end P4.9G3 ──
