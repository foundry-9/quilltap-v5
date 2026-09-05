//! The autonomous-rooms dispatch surface (P4.6ad) — thin marshaling over the
//! already-ported [`enclave::lifecycle`](crate::enclave::lifecycle) run-control
//! core. A differential port of v4's two route files:
//!
//!   - `app/api/v1/system/autonomous-rooms/route.ts` (the user-scoped listing),
//!   - `app/api/v1/chats/[id]/autonomous-room/route.ts` (the per-room status +
//!     the `start`/`pause`/`stop`/`resume`/`update-settings` verbs).
//!
//! The lifecycle engine (`step()` + `RunState` + the schedule tick + the manual
//! verbs) is frozen and separately differential-proven; this module only wires
//! the api boundary onto it. Production impurities (the wall clock, the UUID
//! mint, the cron seam) are the system defaults — the cron seam resolves the
//! host's local zone (v4's croner runs tz-less = system-local), matching the
//! host cadence loop.
//!
//! Envelope quirks the port preserves (see the order):
//!   1. `start`/`resume` distinguish `chat_not_found` (→ 404 `notFound('Chat')`);
//!      `pause`/`stop` carry no reason and answer every failure as
//!      `400 badRequest(message)` — so a missing chat is a 400 there, a 404 on
//!      start/resume. Do not collapse the two.
//!   2. `update-settings` guards the chat FIRST (`notFound('Chat')` /
//!      `badRequest('Chat is not an autonomous room')`), then routes the service
//!      result (`invalid_cron` → 400).
//!   3. Listing sort is two-key: the v4 `stateOrder` literal, then `updatedAt`
//!      desc (localeCompare). Status/listing default `budgetExcludeCacheHits`
//!      to 1 and `runDestructiveToolsAllowed` to 0.

use serde_json::{json, Map, Value};

use crate::db::chats_read;
use crate::db::document_store_overlay::OverlayError;
use crate::db::projects::ProjectsRepository;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::enclave::announce::{system_mint_uuid, system_now_ms};
use crate::enclave::lifecycle::{
    self, AutonomousRoomSettingsPatch, StartFailReason, StartManualRunResult,
    UpdateAutonomousRoomSettingsResult, UpdateSettingsFailReason,
};

use super::types::{ErrorKind, Response};

/// The host's local IANA zone (v4 croner is tz-less = system-local). A fixed
/// offset with no IANA name falls back to `UTC` (defensive; dev/CI machines
/// always resolve a zone).
fn system_tz() -> String {
    jiff::tz::TimeZone::system()
        .iana_name()
        .unwrap_or("UTC")
        .to_string()
}

/// A `v ?? default` read over the marshaled chat object (chats_read omits NULL
/// nullable columns, so an absent key is exactly v4's `undefined ?? default`).
fn or_default(chat: &Value, key: &str, default: Value) -> Value {
    match chat.get(key) {
        Some(v) if !v.is_null() => v.clone(),
        _ => default,
    }
}

/// A `v ?? null` read over the marshaled chat object.
fn or_null(chat: &Value, key: &str) -> Value {
    or_default(chat, key, Value::Null)
}

/// Map a `DbError` to v4's `serverError(...)` arm (500). The specific 500 copy
/// per handler is preserved (the differential never drives these; the message
/// mirrors v4 for parity).
fn server_error(message: &str) -> Response {
    Response::error(ErrorKind::Internal, message.to_string())
}

// ============================================================================
// GET /api/v1/system/autonomous-rooms — the user-scoped listing
// ============================================================================

/// v4 `systemAutonomousRooms` — every `chatType==='autonomous'` chat owned by
/// the user, projected to the management DTO and sorted running-first.
pub fn system_autonomous_rooms(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    let result = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let user_chats = chats_read::find_by_user_id(main, &uid)?;
            let autonomous: Vec<Value> = user_chats
                .into_iter()
                .filter(|c| c.get("chatType").and_then(Value::as_str) == Some("autonomous"))
                .collect();

            // Unique, non-empty projectIds → the projectName map (v4 builds the
            // Set then findById each). Projects are store-backed (overlay repo).
            let repo = ProjectsRepository::new(main, mount);
            let mut project_name_by_id: Map<String, Value> = Map::new();
            for c in &autonomous {
                let Some(pid) = c.get("projectId").and_then(Value::as_str) else {
                    continue;
                };
                if pid.is_empty() || project_name_by_id.contains_key(pid) {
                    continue;
                }
                if let Some(project) = repo.find_by_id(pid).map_err(overlay_to_db)? {
                    let name = project.get("name").cloned().unwrap_or(Value::Null);
                    project_name_by_id.insert(pid.to_string(), name);
                }
            }

            let mut rooms: Vec<Value> = autonomous
                .iter()
                .map(|c| room_dto(c, &project_name_by_id))
                .collect();
            sort_rooms(&mut rooms);
            Ok(Value::Array(rooms))
        })
    });
    match result {
        Ok(rooms) => Response::AutonomousRoom(json!({ "rooms": rooms })),
        Err(_) => server_error("Failed to list autonomous rooms"),
    }
}

/// The per-room listing DTO (v4's `.map((c) => …)` object literal).
fn room_dto(c: &Value, project_name_by_id: &Map<String, Value>) -> Value {
    let project_id = c
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let project_name = match project_id {
        Some(pid) => project_name_by_id.get(pid).cloned().unwrap_or(Value::Null),
        None => Value::Null,
    };
    let participants: Vec<Value> = c
        .get("participants")
        .and_then(Value::as_array)
        .map(|ps| {
            ps.iter()
                .map(|p| {
                    json!({
                        "id": p.get("id").cloned().unwrap_or(Value::Null),
                        "type": p.get("type").cloned().unwrap_or(Value::Null),
                        "characterId": or_null(p, "characterId"),
                        "status": p.get("status").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    json!({
        "id": c.get("id").cloned().unwrap_or(Value::Null),
        "title": c.get("title").cloned().unwrap_or(Value::Null),
        "projectId": project_id.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
        "projectName": project_name,
        "participants": participants,
        "runState": or_null(c, "runState"),
        "runStateMessage": or_null(c, "runStateMessage"),
        "currentRunId": or_null(c, "currentRunId"),
        "runStartedAt": or_null(c, "runStartedAt"),
        "runEndedAt": or_null(c, "runEndedAt"),
        "runPausedAccumMs": or_default(c, "runPausedAccumMs", json!(0)),
        "runTurnsConsumed": or_default(c, "runTurnsConsumed", json!(0)),
        "runTokensConsumed": or_default(c, "runTokensConsumed", json!(0)),
        "scheduleCron": or_null(c, "scheduleCron"),
        "scheduleNextRunAt": or_null(c, "scheduleNextRunAt"),
        "scheduleLastRunAt": or_null(c, "scheduleLastRunAt"),
        "scheduleFreshnessWindowMs": or_null(c, "scheduleFreshnessWindowMs"),
        "budgetMaxTurns": or_null(c, "budgetMaxTurns"),
        "budgetMaxTokens": or_null(c, "budgetMaxTokens"),
        "budgetMaxWallClockMs": or_null(c, "budgetMaxWallClockMs"),
        "budgetEstimatedSpendCapUSD": or_null(c, "budgetEstimatedSpendCapUSD"),
        "runDestructiveToolsAllowed": or_default(c, "runDestructiveToolsAllowed", json!(0)),
        "runVisibility": or_null(c, "runVisibility"),
        "createdAt": c.get("createdAt").cloned().unwrap_or(Value::Null),
        "updatedAt": c.get("updatedAt").cloned().unwrap_or(Value::Null),
    })
}

/// v4's stable two-key sort: the `stateOrder` literal, unknown → 99; then
/// `updatedAt` desc (localeCompare over the ISO strings).
fn sort_rooms(rooms: &mut [Value]) {
    fn state_order(v: &Value) -> u32 {
        match v.get("runState").and_then(Value::as_str) {
            Some("running") => 0,
            Some("idle") => 1,
            Some("paused") => 2,
            Some("budgetExhausted") => 3,
            Some("stopped") => 4,
            Some("error") => 5,
            _ => 99,
        }
    }
    rooms.sort_by(|a, b| {
        let ao = state_order(a);
        let bo = state_order(b);
        if ao != bo {
            return ao.cmp(&bo);
        }
        // v4 `(b.updatedAt ?? '').localeCompare(a.updatedAt ?? '')`.
        let au = a.get("updatedAt").and_then(Value::as_str).unwrap_or("");
        let bu = b.get("updatedAt").and_then(Value::as_str).unwrap_or("");
        crate::collation::locale_compare(bu, au)
    });
}

// ============================================================================
// GET /api/v1/chats/[id]/autonomous-room — the status snapshot
// ============================================================================

/// The `ensureAutonomousChat` guard shared by status + update-settings: reads
/// the chat (global `findById`, not user-scoped — v4 parity), refusing a missing
/// chat as `notFound('Chat')` and a non-autonomous chat as
/// `badRequest('Chat is not an autonomous room')`.
fn ensure_autonomous_chat(db: &Db, chat_id: &str) -> Result<Value, Response> {
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |conn| chats_read::find_by_id(conn, &cid)) {
        Ok(c) => c,
        Err(_) => return Err(server_error("Failed to read chat")),
    };
    let Some(chat) = chat else {
        return Err(Response::error(ErrorKind::NotFound, "Chat not found"));
    };
    if chat.get("chatType").and_then(Value::as_str) != Some("autonomous") {
        return Err(Response::error(
            ErrorKind::BadRequest,
            "Chat is not an autonomous room",
        ));
    }
    Ok(chat)
}

/// v4 `handleStatus` — the run-status snapshot for the management UI.
pub fn autonomous_room_status(db: &Db, chat_id: &str) -> Response {
    let chat = match ensure_autonomous_chat(db, chat_id) {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    Response::AutonomousRoom(json!({
        "chatId": chat_id,
        "chatType": "autonomous",
        "runState": or_null(&chat, "runState"),
        "currentRunId": or_null(&chat, "currentRunId"),
        "runStateMessage": or_null(&chat, "runStateMessage"),
        "runStartedAt": or_null(&chat, "runStartedAt"),
        "runEndedAt": or_null(&chat, "runEndedAt"),
        "runPausedAccumMs": or_default(&chat, "runPausedAccumMs", json!(0)),
        "runTurnsConsumed": or_default(&chat, "runTurnsConsumed", json!(0)),
        "runTokensConsumed": or_default(&chat, "runTokensConsumed", json!(0)),
        "scheduleCron": or_null(&chat, "scheduleCron"),
        "scheduleNextRunAt": or_null(&chat, "scheduleNextRunAt"),
        "scheduleLastRunAt": or_null(&chat, "scheduleLastRunAt"),
        "scheduleFreshnessWindowMs": or_null(&chat, "scheduleFreshnessWindowMs"),
        "budgetMaxTurns": or_null(&chat, "budgetMaxTurns"),
        "budgetMaxTokens": or_null(&chat, "budgetMaxTokens"),
        "budgetMaxWallClockMs": or_null(&chat, "budgetMaxWallClockMs"),
        "budgetEstimatedSpendCapUSD": or_null(&chat, "budgetEstimatedSpendCapUSD"),
        "budgetExcludeCacheHits": or_default(&chat, "budgetExcludeCacheHits", json!(1)),
        "runDestructiveToolsAllowed": or_default(&chat, "runDestructiveToolsAllowed", json!(0)),
        "runVisibility": or_null(&chat, "runVisibility"),
    }))
}

// ============================================================================
// POST …?action=start|pause|stop|resume|update-settings
// ============================================================================

/// v4 `handleStart` — manual run start. `chat_not_found` → 404; other failures
/// → 400; success → `{runId, jobId}`.
pub async fn autonomous_room_start(db: &Db, user_id: &str, chat_id: &str) -> Response {
    let tz = system_tz();
    let now_ms = system_now_ms;
    let mint = system_mint_uuid;
    let cron_seam =
        |expr: &str, anchor: i64| crate::enclave::cron::try_next_occurrence(expr, anchor, &tz);
    let deps = lifecycle::LifecycleDeps {
        now_ms: &now_ms,
        mint_uuid: &mint,
        next_occurrence: &cron_seam,
    };
    match lifecycle::start_autonomous_room_manually(db, &deps, chat_id, user_id).await {
        Ok(result) => start_result_response(result),
        Err(_) => server_error("Failed to start autonomous run"),
    }
}

/// v4 `handleResume` — resume (or fresh-start) a paused/idle room. Same envelope
/// as start.
pub async fn autonomous_room_resume(db: &Db, user_id: &str, chat_id: &str) -> Response {
    let tz = system_tz();
    let now_ms = system_now_ms;
    let mint = system_mint_uuid;
    let cron_seam =
        |expr: &str, anchor: i64| crate::enclave::cron::try_next_occurrence(expr, anchor, &tz);
    let deps = lifecycle::LifecycleDeps {
        now_ms: &now_ms,
        mint_uuid: &mint,
        next_occurrence: &cron_seam,
    };
    match lifecycle::resume_autonomous_room(db, &deps, chat_id, user_id).await {
        Ok(result) => start_result_response(result),
        Err(_) => server_error("Failed to resume autonomous run"),
    }
}

/// Shared start/resume envelope routing.
fn start_result_response(result: StartManualRunResult) -> Response {
    match result {
        StartManualRunResult::Ok { run_id, job_id } => {
            Response::AutonomousRoom(json!({ "runId": run_id, "jobId": job_id }))
        }
        StartManualRunResult::Failed { reason, message } => {
            if reason == StartFailReason::ChatNotFound {
                Response::error(ErrorKind::NotFound, "Chat not found")
            } else {
                Response::error(ErrorKind::BadRequest, message)
            }
        }
    }
}

/// v4 `handlePause` — idempotent pause. Every failure (incl. a missing chat) is
/// `400 badRequest(message)` (the `SimpleOpResult` carries no reason).
pub async fn autonomous_room_pause(db: &Db, chat_id: &str) -> Response {
    let tz = system_tz();
    let now_ms = system_now_ms;
    let mint = system_mint_uuid;
    let cron_seam =
        |expr: &str, anchor: i64| crate::enclave::cron::try_next_occurrence(expr, anchor, &tz);
    let deps = lifecycle::LifecycleDeps {
        now_ms: &now_ms,
        mint_uuid: &mint,
        next_occurrence: &cron_seam,
    };
    match lifecycle::pause_autonomous_room(db, &deps, chat_id).await {
        Ok(result) => {
            if result.ok {
                Response::AutonomousRoom(json!({ "paused": true }))
            } else {
                Response::error(
                    ErrorKind::BadRequest,
                    result.message.unwrap_or_else(|| "Pause failed".to_string()),
                )
            }
        }
        Err(_) => server_error("Failed to pause autonomous run"),
    }
}

/// v4 `handleStop` — stop + bump `currentRunId`. Failure → `400 badRequest`.
pub async fn autonomous_room_stop(db: &Db, chat_id: &str) -> Response {
    let tz = system_tz();
    let now_ms = system_now_ms;
    let mint = system_mint_uuid;
    let cron_seam =
        |expr: &str, anchor: i64| crate::enclave::cron::try_next_occurrence(expr, anchor, &tz);
    let deps = lifecycle::LifecycleDeps {
        now_ms: &now_ms,
        mint_uuid: &mint,
        next_occurrence: &cron_seam,
    };
    match lifecycle::stop_autonomous_room(db, &deps, chat_id).await {
        Ok(result) => {
            if result.ok {
                Response::AutonomousRoom(json!({ "stopped": true }))
            } else {
                Response::error(
                    ErrorKind::BadRequest,
                    result.message.unwrap_or_else(|| "Stop failed".to_string()),
                )
            }
        }
        Err(_) => server_error("Failed to stop autonomous run"),
    }
}

/// v4 `handleUpdateSettings` — the Edit Enclave modal's partial patch. Guards the
/// chat first, then routes the service result. `settings` is the raw
/// `updateSettingsSchema` bag (every cap nullish so the modal can CLEAR a value;
/// ms units). `invalid_cron` → 400; success → `{updated: true, clampedDestructive}`.
pub async fn autonomous_room_update_settings(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    settings: &Value,
) -> Response {
    // The chat guard runs BEFORE the body parse (v4 order).
    if let Err(resp) = ensure_autonomous_chat(db, chat_id) {
        return resp;
    }
    let patch = match parse_settings_patch(settings) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    let tz = system_tz();
    let now_ms = system_now_ms;
    let mint = system_mint_uuid;
    let cron_seam =
        |expr: &str, anchor: i64| crate::enclave::cron::try_next_occurrence(expr, anchor, &tz);
    let deps = lifecycle::LifecycleDeps {
        now_ms: &now_ms,
        mint_uuid: &mint,
        next_occurrence: &cron_seam,
    };
    match lifecycle::update_autonomous_room_settings(db, &deps, chat_id, user_id, &patch).await {
        Ok(UpdateAutonomousRoomSettingsResult::Ok {
            clamped_destructive,
        }) => Response::AutonomousRoom(json!({
            "updated": true,
            "clampedDestructive": clamped_destructive,
        })),
        Ok(UpdateAutonomousRoomSettingsResult::Failed { reason, message }) => {
            if reason == UpdateSettingsFailReason::ChatNotFound {
                Response::error(ErrorKind::NotFound, "Chat not found")
            } else {
                Response::error(ErrorKind::BadRequest, message)
            }
        }
        Err(_) => server_error("Failed to update autonomous-room settings"),
    }
}

/// Map v4 `updateSettingsSchema` fields (all `.nullish()` on the caps) onto the
/// lifecycle patch's tri-state `Option<Option<_>>` columns. Presence-of-key ⇒
/// the outer `Some`; an explicit `null` ⇒ the inner `None` (clear); a value ⇒
/// `Some(v)`. The two plain booleans + `title` (set-only) are non-tri-state.
///
/// P4.55 (the merge-verb silent-keep sweep) made this FALLIBLE. v4 runs
/// `updateSettingsSchema.parse` (`chats/[id]/autonomous-room/route.ts:178-184`,
/// schema :60-71) BEFORE the service call and answers `validationError` — 400
/// `"Validation error"` — for any present-but-invalid field. v5 used to DROP a
/// wrong-typed or out-of-range field silently and answer 200, so a bogus
/// `runVisibility`, a negative cap or a 400-character `title` vanished without
/// a word. Only `.parse` failures land here; the one service-layer guard
/// (invalid cron → 400 with its own sentence) still belongs to the lifecycle.
///
/// v5 answers the message only, no `details` issues array (the recorded
/// no-details divergence class).
///
/// A non-object body is Zod's own root-level `invalid_type` — the same 400.
/// (v4 reaches `badRequest('Invalid request body')` only when `req.json()`
/// itself throws, which is upstream of this boundary: the dispatch layer hands
/// us an already-parsed value.) Unknown keys are STRIPPED by `z.object`, never
/// refused.
fn parse_settings_patch(settings: &Value) -> Result<AutonomousRoomSettingsPatch, Response> {
    let mut patch = AutonomousRoomSettingsPatch::default();
    let Some(obj) = settings.as_object() else {
        return Err(validation_error());
    };

    // title — `z.string().max(300).optional()`. `.optional()`, NOT `.nullish()`:
    // a present `null` is an invalid_type failure, not a clear. The max is
    // UTF-16 code units, as Zod measures (`String.length`) — a chars() count
    // here under-counts astral characters and silently widens the limit (the
    // §3 unification review's catch; pinned by `update_invalid_title_astral`).
    if let Some(v) = obj.get("title") {
        // Zod ≥ 4.5.4 (v4 `6e1a64ea6`): code points once the UTF-16 count overflows.
        let Some(t) = v.as_str().filter(|t| crate::jsstr::zod_len_max_ok(t, 300)) else {
            return Err(validation_error());
        };
        patch.title = Some(t.to_string());
    }
    // scheduleCron — `z.string().max(120).nullish()`.
    patch.schedule_cron = tri_state_string(obj, "scheduleCron", Some(120))?;
    // The nullish numeric caps: `z.number().int().positive().nullish()` …
    patch.schedule_freshness_window_ms = tri_state_number(obj, "scheduleFreshnessWindowMs", true)?;
    patch.budget_max_turns = tri_state_number(obj, "budgetMaxTurns", true)?;
    patch.budget_max_tokens = tri_state_number(obj, "budgetMaxTokens", true)?;
    patch.budget_max_wall_clock_ms = tri_state_number(obj, "budgetMaxWallClockMs", true)?;
    // … except the spend cap, which is `.positive()` WITHOUT `.int()`.
    patch.budget_estimated_spend_cap_usd =
        tri_state_number(obj, "budgetEstimatedSpendCapUSD", false)?;
    // runVisibility — `z.enum(['owner_only','household','open']).nullish()`.
    patch.run_visibility = tri_state_string(obj, "runVisibility", None)?;
    if let Some(Some(vis)) = patch.run_visibility.as_ref() {
        if !matches!(vis.as_str(), "owner_only" | "household" | "open") {
            return Err(validation_error());
        }
    }
    // The two plain `z.boolean().optional()` fields.
    for (key, slot) in [
        (
            "runDestructiveToolsAllowed",
            &mut patch.run_destructive_tools_allowed,
        ),
        (
            "budgetExcludeCacheHits",
            &mut patch.budget_exclude_cache_hits,
        ),
    ] {
        if let Some(v) = obj.get(key) {
            let Some(b) = v.as_bool() else {
                return Err(validation_error());
            };
            *slot = Some(b);
        }
    }
    Ok(patch)
}

/// v4 `validationError(...)` — 400 `{"error":"Validation error"}` (the `details`
/// issues array is the recorded no-details divergence class).
fn validation_error() -> Response {
    Response::error(ErrorKind::BadRequest, "Validation error")
}

/// Tri-state read for a `.nullish()` string column: absent → `None`; explicit
/// null → `Some(None)` (clear); string → `Some(Some(v))`. A present non-string
/// is a Zod `invalid_type` refusal; `max_len` (in UTF-16 code units, as Zod
/// measures) is the optional `.max()` check.
fn tri_state_string(
    obj: &Map<String, Value>,
    key: &str,
    max_len: Option<usize>,
) -> Result<Option<Option<String>>, Response> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) => {
            if max_len.is_some_and(|m| !crate::jsstr::zod_len_max_ok(s, m)) {
                return Err(validation_error());
            }
            Ok(Some(Some(s.clone())))
        }
        Some(_) => Err(validation_error()),
    }
}

/// Tri-state read for a `.nullish()` numeric column: absent → `None`; explicit
/// null → `Some(None)` (clear); number → `Some(Some(v))`. Every one of v4's
/// numeric caps is `.positive()`; `int` additionally demands
/// `Number.isSafeInteger`.
fn tri_state_number(
    obj: &Map<String, Value>,
    key: &str,
    int: bool,
) -> Result<Option<Option<f64>>, Response> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::Number(n)) => {
            let Some(f) = n.as_f64().filter(|f| !f.is_nan()) else {
                return Err(validation_error());
            };
            if f <= 0.0 {
                return Err(validation_error());
            }
            if int && !(f.fract() == 0.0 && f.abs() <= 9_007_199_254_740_991.0) {
                return Err(validation_error());
            }
            Ok(Some(Some(f)))
        }
        Some(_) => Err(validation_error()),
    }
}

/// Map an overlay error to a `DbError` (the projects repo returns its own error
/// type; the listing surfaces it as a 500 like every other read failure).
fn overlay_to_db(e: OverlayError) -> DbError {
    // Structure-preserving (P4.23): the `Unavailable` refusal survives as
    // `DbError::StoreUnavailable` so the terminal arm can answer v4's
    // contextful 503 instead of a 500 + leaked detail.
    e.into_db()
}
