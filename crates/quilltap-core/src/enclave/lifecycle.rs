//! The autonomous-room lifecycle service (U4.3) — v4
//! `lib/services/chat-message/autonomous-room.service.ts` (whole file) +
//! `lib/background-jobs/handlers/autonomous-run-start.ts` (whole file).
//!
//! Manual start / pause / resume / stop / update-settings + the startup and
//! failed-turn reconciliation, over the shared run-start core
//! (`beginAutonomousRun`): flip the row straight to `running` (counters zeroed
//! via [`crate::enclave::announce::run_start_patch`]), enqueue the first
//! `AUTONOMOUS_ROOM_TURN`, post the "run begun" banner; on an enqueue failure
//! roll the row back rather than leave it falsely `running`.
//!
//! ## The `runStateMessage` vocabulary written here (verbatim from v4)
//!
//! - `manual:paused` — [`pause_autonomous_room`]
//! - `manual:stopped` — [`stop_autonomous_room`]
//! - `restart:interrupted` — [`reconcile_autonomous_runs_at_startup`]
//! - `turn_failed:{reason}` (UTF-16-sliced to 500) — [`reconcile_failed_autonomous_turn`]
//! - `start:enqueue_failed` / `schedule:enqueue_failed` — the
//!   [`begin_autonomous_run`] rollback (manual vs scheduled entry)
//!
//! ## The cron seam
//!
//! Wherever v4 evaluates `new Cron(expr).nextRun(anchor)` (the manual-start
//! cron-slot consumption; the settings editor's recompute), the evaluation is an
//! **injected** [`LifecycleDeps::next_occurrence`] closure —
//! `(expr, anchor_ms) → Ok(Some(next_ms)) | Ok(None) | Err(invalid)` — so this
//! sub-unit does not depend on the concurrently-ported `enclave::cron`. The
//! integrator wires the real `enclave::cron::next_occurrence` here.
//!
//! ## The fork bridge does not port
//!
//! v4's `startScheduledAutonomousRun` exists only to hop the forked job child's
//! buffered writes back to the parent's RW connection (host-RPC) so the
//! `currentRunId` write commits **before** the turn job is claimable. In v5 the
//! single-writer runtime already guarantees that ordering (handlers run
//! in-process and `Db::write` serializes), so [`start_scheduled_autonomous_run`]
//! is the core directly — the W4.8 fork/IPC non-port decision.

use serde_json::Value;

use crate::clock::{iso_from_unix_ms, iso_to_ms};
use crate::db::chats::{ChatUpdate, ChatsRepository};
use crate::db::runtime::Db;
use crate::db::{chat_settings, chats_read, DbError};
use crate::enclave::announce::{post_run_start_announcement, run_start_patch, MintUuidFn, NowMsFn};
use crate::jsstr::{js_trim, utf16_truncate};
use crate::services::queue_service::enqueue_autonomous_room_turn;

/// v4 `DEFAULT_FRESHNESS_WINDOW_MS` — 12 hours.
pub const DEFAULT_FRESHNESS_WINDOW_MS: f64 = 12.0 * 60.0 * 60.0 * 1000.0;

/// The injected croner-semantics evaluation: `(expr, anchor_ms)` →
/// `Ok(Some(next_ms))` (the next occurrence strictly after the anchor),
/// `Ok(None)` (no future occurrence — v4 `nextRun()` returned `null`), or
/// `Err(message)` (v4 `new Cron(expr)` threw — invalid expression).
pub type NextOccurrenceFn<'a> =
    &'a (dyn Fn(&str, i64) -> Result<Option<i64>, String> + Send + Sync);

/// The lifecycle's injected impurities (the orchestrator `Deps` precedent):
/// clock, UUID mint, and the cron seam.
pub struct LifecycleDeps<'a> {
    /// v4 `Date.now()` / `new Date()` — the millisecond wall clock.
    pub now_ms: NowMsFn<'a>,
    /// v4 `randomUUID()`.
    pub mint_uuid: MintUuidFn<'a>,
    /// v4 `new Cron(expr).nextRun(anchor)` (see [`NextOccurrenceFn`]).
    pub next_occurrence: NextOccurrenceFn<'a>,
}

// ============================================================================
// beginAutonomousRun (v4 autonomous-run-start.ts)
// ============================================================================

/// Where to roll the row if the turn enqueue fails *after* the row already
/// flipped to `running`. Manual start uses `Error` (surfaces as a failed
/// request); the scheduled tick uses `Idle` so the next cron slot can retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnEnqueueFailure {
    Idle,
    Error,
}

impl OnEnqueueFailure {
    fn run_state(self) -> &'static str {
        match self {
            OnEnqueueFailure::Idle => "idle",
            OnEnqueueFailure::Error => "error",
        }
    }
    fn run_state_message(self) -> &'static str {
        match self {
            OnEnqueueFailure::Error => "start:enqueue_failed",
            OnEnqueueFailure::Idle => "schedule:enqueue_failed",
        }
    }
}

/// v4 `BeginAutonomousRunInput`. The two schedule fields are tri-state,
/// distinguished by *presence* (`None` = key omitted → leave the column
/// untouched; `Some(None)` = write NULL; `Some(Some(v))` = write the value) —
/// exactly v4's `'scheduleNextRunAt' in input` / `!== undefined` checks.
pub struct BeginAutonomousRunInput {
    pub chat_id: String,
    pub user_id: String,
    pub run_id: String,
    pub now_iso: String,
    /// Stamp the run-start time onto `scheduleLastRunAt`. `None` to leave untouched.
    pub schedule_last_run_at: Option<Option<String>>,
    /// `None` → leave untouched (manual start that didn't consume a cron slot);
    /// `Some(None)` → clear it; `Some(Some(v))` → write it (slot advanced).
    pub schedule_next_run_at: Option<Option<String>>,
    pub on_enqueue_failure: OnEnqueueFailure,
}

/// v4 `BeginAutonomousRunResult::reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeginFailReason {
    ChatNotFound,
    NotAutonomous,
    EnqueueFailed,
}

impl BeginFailReason {
    pub fn as_str(self) -> &'static str {
        match self {
            BeginFailReason::ChatNotFound => "chat_not_found",
            BeginFailReason::NotAutonomous => "not_autonomous",
            BeginFailReason::EnqueueFailed => "enqueue_failed",
        }
    }
}

/// v4 `BeginAutonomousRunResult`. `cause` carries the original enqueue error for
/// `enqueue_failed` so the manual-start caller can re-throw it verbatim.
#[derive(Debug)]
pub enum BeginAutonomousRunResult {
    Ok {
        run_id: String,
        job_id: String,
    },
    Failed {
        reason: BeginFailReason,
        message: String,
        cause: Option<DbError>,
    },
}

/// v4 `beginAutonomousRun` — the parent-executed run-start core. Flip the row
/// straight to `running` (counters zeroed, pause state cleared) plus any
/// schedule bookkeeping the caller asked for; the write commits before the
/// enqueue (the single-writer channel serializes), so the first turn job always
/// sees the live `currentRunId`. Then enqueue the first turn; on failure roll
/// the row back (`start:enqueue_failed` / `schedule:enqueue_failed` + `runEndedAt`)
/// so it is eligible again. Finally post the "run begun" banner (best-effort —
/// the run is already live).
pub async fn begin_autonomous_run(
    db: &Db,
    deps: &LifecycleDeps<'_>,
    input: BeginAutonomousRunInput,
) -> Result<BeginAutonomousRunResult, DbError> {
    let chat_id = input.chat_id.clone();
    let chat = db.read_main(|conn| chats_read::find_by_id(conn, &chat_id))?;
    let Some(chat) = chat else {
        return Ok(BeginAutonomousRunResult::Failed {
            reason: BeginFailReason::ChatNotFound,
            message: "Chat not found.".to_string(),
            cause: None,
        });
    };
    if chat.get("chatType").and_then(Value::as_str) != Some("autonomous") {
        return Ok(BeginAutonomousRunResult::Failed {
            reason: BeginFailReason::NotAutonomous,
            message: "This chat is not an autonomous room.".to_string(),
            cause: None,
        });
    }

    // The run-start patch + schedule bookkeeping, committed before the enqueue.
    let mut patch = run_start_patch(&input.now_iso, &input.run_id);
    if let Some(v) = &input.schedule_last_run_at {
        patch.schedule_last_run_at = Some(v.clone());
    }
    if let Some(v) = &input.schedule_next_run_at {
        patch.schedule_next_run_at = Some(v.clone());
    }
    {
        let chat_id = input.chat_id.clone();
        db.write(move |writers| {
            ChatsRepository::new(writers.main().connection()).update(&chat_id, &patch)
        })
        .await?;
    }

    let job_id =
        match enqueue_autonomous_room_turn(db, &input.user_id, &input.chat_id, &input.run_id).await
        {
            Ok(id) => id,
            Err(error) => {
                // The row already flipped to `running`; with no turn on the queue
                // it would wedge (the scheduler filter excludes `running`). Roll
                // it back so it's eligible again — `idle` for the scheduled path
                // (next cron slot retries), `error` for the manual path (the
                // request fails loudly).
                let run_state_message = input.on_enqueue_failure.run_state_message();
                let rollback = ChatUpdate {
                    run_state: Some(input.on_enqueue_failure.run_state().to_string()),
                    run_state_message: Some(Some(run_state_message.to_string())),
                    run_ended_at: Some(Some(iso_from_unix_ms((deps.now_ms)()))),
                    ..Default::default()
                };
                let chat_id = input.chat_id.clone();
                db.write(move |writers| {
                    ChatsRepository::new(writers.main().connection()).update(&chat_id, &rollback)
                })
                .await?;
                return Ok(BeginAutonomousRunResult::Failed {
                    reason: BeginFailReason::EnqueueFailed,
                    message: run_state_message.to_string(),
                    cause: Some(error),
                });
            }
        };

    // Run is live and a turn is queued — post the "run begun" banner.
    // Best-effort (the helper swallows its own write failures); `chat` is the
    // pre-update snapshot, whose participants and budget caps are unchanged.
    post_run_start_announcement(
        db,
        deps.now_ms,
        deps.mint_uuid,
        &input.chat_id,
        &input.run_id,
        &chat,
    )
    .await?;

    Ok(BeginAutonomousRunResult::Ok {
        run_id: input.run_id,
        job_id,
    })
}

/// v4 `startScheduledAutonomousRun` — in v4 a child-aware host-RPC bridge; in v5
/// simply the core (see the module docs: the fork bridge does not port).
pub async fn start_scheduled_autonomous_run(
    db: &Db,
    deps: &LifecycleDeps<'_>,
    input: BeginAutonomousRunInput,
) -> Result<BeginAutonomousRunResult, DbError> {
    begin_autonomous_run(db, deps, input).await
}

// ============================================================================
// The manual lifecycle (v4 autonomous-room.service.ts)
// ============================================================================

/// v4 `StartManualRunResult::reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartFailReason {
    NotAutonomous,
    AlreadyRunning,
    ChatNotFound,
}

impl StartFailReason {
    pub fn as_str(self) -> &'static str {
        match self {
            StartFailReason::NotAutonomous => "not_autonomous",
            StartFailReason::AlreadyRunning => "already_running",
            StartFailReason::ChatNotFound => "chat_not_found",
        }
    }
}

/// v4 `StartManualRunResult` (also the `resumeAutonomousRoom` result shape).
#[derive(Debug)]
pub enum StartManualRunResult {
    Ok {
        run_id: String,
        job_id: String,
    },
    Failed {
        reason: StartFailReason,
        message: String,
    },
}

fn chat_str<'a>(chat: &'a Value, key: &str) -> Option<&'a str> {
    chat.get(key).and_then(Value::as_str)
}

/// v4 `startAutonomousRoomManually(chatId, userId)`. Rejects if the chat isn't
/// autonomous, or if there's already a non-terminal run in flight (`running`).
/// Idle/paused/budgetExhausted rooms are eligible — the new runId atomically
/// supersedes any prior state.
///
/// Consumes the upcoming cron slot when it falls inside the freshness window
/// (per-chat `scheduleFreshnessWindowMs`, else the user's
/// `autonomousRoomSettings.defaultFreshnessWindowMs`, else the 12 h default): a
/// manual start close to the scheduled slot replaces it, and `scheduleNextRunAt`
/// advances past the consumed slot (via the injected cron seam; an evaluation
/// failure or a no-future result keeps the slot untouched, matching v4's
/// try/catch + `advanced ? … : unchanged`).
///
/// An enqueue failure after the row flip is re-thrown (v4 re-throws the cause;
/// the core has already rolled the row to `error`).
pub async fn start_autonomous_room_manually(
    db: &Db,
    deps: &LifecycleDeps<'_>,
    chat_id: &str,
    user_id: &str,
) -> Result<StartManualRunResult, DbError> {
    let cid = chat_id.to_string();
    let chat = db.read_main(|conn| chats_read::find_by_id(conn, &cid))?;
    let Some(chat) = chat else {
        return Ok(StartManualRunResult::Failed {
            reason: StartFailReason::ChatNotFound,
            message: "Chat not found.".to_string(),
        });
    };
    if chat_str(&chat, "chatType") != Some("autonomous") {
        return Ok(StartManualRunResult::Failed {
            reason: StartFailReason::NotAutonomous,
            message: "This chat is not an autonomous room.".to_string(),
        });
    }
    if chat_str(&chat, "runState") == Some("running") {
        return Ok(StartManualRunResult::Failed {
            reason: StartFailReason::AlreadyRunning,
            message: "An autonomous run is already in progress for this room.".to_string(),
        });
    }

    let now = (deps.now_ms)();
    let now_iso = iso_from_unix_ms(now);
    let run_id = (deps.mint_uuid)();

    // Freshness window: per-chat override, else the user setting, else 12 h.
    // v4 `repos.chatSettings.findByUserId` is a `safeQuery` (errors → null), so
    // a read failure falls through to the default.
    let uid = user_id.to_string();
    let settings = db
        .read_main(|conn| chat_settings::find_by_user_id(conn, &uid))
        .unwrap_or(None);
    let default_freshness_ms = settings
        .as_ref()
        .and_then(|s| s.get("autonomousRoomSettings"))
        .and_then(|a| a.get("defaultFreshnessWindowMs"))
        .and_then(Value::as_f64)
        .unwrap_or(DEFAULT_FRESHNESS_WINDOW_MS);
    let freshness_window_ms = chat
        .get("scheduleFreshnessWindowMs")
        .and_then(Value::as_f64)
        .unwrap_or(default_freshness_ms);

    // Consume the upcoming cron slot if it falls inside the freshness window.
    let original_next: Option<String> = chat_str(&chat, "scheduleNextRunAt").map(String::from);
    let mut next_scheduled_run_at: Option<String> = original_next.clone();
    if let (Some(cron), Some(next_at)) = (chat_str(&chat, "scheduleCron"), &original_next) {
        // v4 `Date.parse` — an unparseable slot makes the distance NaN, which
        // fails the `<=` and skips consumption.
        if let Some(next_ms) = iso_to_ms(next_at) {
            let distance = (next_ms - now).abs() as f64;
            if distance <= freshness_window_ms {
                // `advanced` falsy (`Ok(None)`) keeps the slot; a throw (`Err`)
                // is warned + swallowed. Both leave it unchanged.
                if let Ok(Some(advanced_ms)) = (deps.next_occurrence)(cron, now) {
                    next_scheduled_run_at = Some(iso_from_unix_ms(advanced_ms));
                }
            }
        }
    }

    // v4 `advancedNextRunAt = nextScheduledRunAt !== chat.scheduleNextRunAt` —
    // strict inequality, where an absent column is `undefined` and the local
    // starts at `?? null`: an unchanged **string** slot compares equal (no
    // write), but a chat with no slot at all compares `null !== undefined` →
    // the input carries an explicit `scheduleNextRunAt: null`.
    let advanced_next_run_at = !matches!(
        (&next_scheduled_run_at, &original_next),
        (Some(a), Some(b)) if a == b
    );

    let result = begin_autonomous_run(
        db,
        deps,
        BeginAutonomousRunInput {
            chat_id: chat_id.to_string(),
            user_id: user_id.to_string(),
            run_id,
            now_iso: now_iso.clone(),
            schedule_last_run_at: Some(Some(now_iso)),
            schedule_next_run_at: if advanced_next_run_at {
                Some(next_scheduled_run_at)
            } else {
                None
            },
            on_enqueue_failure: OnEnqueueFailure::Error,
        },
    )
    .await?;

    match result {
        BeginAutonomousRunResult::Ok { run_id, job_id } => {
            Ok(StartManualRunResult::Ok { run_id, job_id })
        }
        // chat existence/type were validated above, so the only reachable
        // failure is an enqueue error — the core already rolled the row to
        // `error`. Re-throw the original cause (v4 maps the throw to a 500).
        BeginAutonomousRunResult::Failed { message, cause, .. } => Err(cause
            .unwrap_or_else(|| DbError::Key(format!("Autonomous-room start failed: {message}")))),
    }
}

/// The `{ ok, message? }` result of pause/stop (v4 returns a plain object).
#[derive(Debug)]
pub struct SimpleOpResult {
    pub ok: bool,
    pub message: Option<String>,
}

impl SimpleOpResult {
    fn ok() -> Self {
        SimpleOpResult {
            ok: true,
            message: None,
        }
    }
    fn fail(message: &str) -> Self {
        SimpleOpResult {
            ok: false,
            message: Some(message.to_string()),
        }
    }
}

/// v4 `pauseAutonomousRoom(chatId)`. Idempotent; transitions to `paused`
/// regardless of starting state when the chat is autonomous, stamping
/// `runPausedAt` so a later resume can fold the interval into
/// `runPausedAccumMs` (keeping it out of the wall-clock budget).
pub async fn pause_autonomous_room(
    db: &Db,
    deps: &LifecycleDeps<'_>,
    chat_id: &str,
) -> Result<SimpleOpResult, DbError> {
    let cid = chat_id.to_string();
    let chat = db.read_main(|conn| chats_read::find_by_id(conn, &cid))?;
    let Some(chat) = chat else {
        return Ok(SimpleOpResult::fail("Chat not found."));
    };
    if chat_str(&chat, "chatType") != Some("autonomous") {
        return Ok(SimpleOpResult::fail("This chat is not an autonomous room."));
    }
    let patch = ChatUpdate {
        run_state: Some("paused".to_string()),
        run_state_message: Some(Some("manual:paused".to_string())),
        run_paused_at: Some(Some(iso_from_unix_ms((deps.now_ms)()))),
        ..Default::default()
    };
    let cid = chat_id.to_string();
    db.write(move |writers| ChatsRepository::new(writers.main().connection()).update(&cid, &patch))
        .await?;
    Ok(SimpleOpResult::ok())
}

/// v4 `stopAutonomousRoom(chatId)`. Transitions to `stopped` and bumps
/// `currentRunId` to a fresh UUID so any in-flight turn job exits cleanly via
/// the stale-run guard on its next tick.
pub async fn stop_autonomous_room(
    db: &Db,
    deps: &LifecycleDeps<'_>,
    chat_id: &str,
) -> Result<SimpleOpResult, DbError> {
    let cid = chat_id.to_string();
    let chat = db.read_main(|conn| chats_read::find_by_id(conn, &cid))?;
    let Some(chat) = chat else {
        return Ok(SimpleOpResult::fail("Chat not found."));
    };
    if chat_str(&chat, "chatType") != Some("autonomous") {
        return Ok(SimpleOpResult::fail("This chat is not an autonomous room."));
    }
    // v4 object-literal order: `currentRunId: randomUUID()` before
    // `runEndedAt: new Date().toISOString()`.
    let fresh_run_id = (deps.mint_uuid)();
    let ended_at = iso_from_unix_ms((deps.now_ms)());
    let patch = ChatUpdate {
        run_state: Some("stopped".to_string()),
        run_state_message: Some(Some("manual:stopped".to_string())),
        current_run_id: Some(fresh_run_id),
        run_ended_at: Some(Some(ended_at)),
        ..Default::default()
    };
    let cid = chat_id.to_string();
    db.write(move |writers| ChatsRepository::new(writers.main().connection()).update(&cid, &patch))
        .await?;
    Ok(SimpleOpResult::ok())
}

/// v4 `resumeAutonomousRoom(chatId, userId)`.
///
/// A genuinely *paused* room that still owns a live run continues the SAME run:
/// counters, `currentRunId`, and `runStartedAt` are preserved (moving
/// `runStartedAt` would drop pre-pause tokens from the token-accounting window),
/// no "run begun" banner is posted, and the paused interval is folded into
/// `runPausedAccumMs` (which the wall-clock budget subtracts). Any other state
/// (idle / stopped / budgetExhausted / error, or a paused row with no runId)
/// falls back to a fresh run via [`start_autonomous_room_manually`].
///
/// The re-enqueue is **not** guarded (v4 has no try/catch here — an enqueue
/// failure throws with the row already flipped back to `running`).
pub async fn resume_autonomous_room(
    db: &Db,
    deps: &LifecycleDeps<'_>,
    chat_id: &str,
    user_id: &str,
) -> Result<StartManualRunResult, DbError> {
    let cid = chat_id.to_string();
    let chat = db.read_main(|conn| chats_read::find_by_id(conn, &cid))?;
    let Some(chat) = chat else {
        return Ok(StartManualRunResult::Failed {
            reason: StartFailReason::ChatNotFound,
            message: "Chat not found.".to_string(),
        });
    };
    if chat_str(&chat, "chatType") != Some("autonomous") {
        return Ok(StartManualRunResult::Failed {
            reason: StartFailReason::NotAutonomous,
            message: "This chat is not an autonomous room.".to_string(),
        });
    }
    if chat_str(&chat, "runState") == Some("running") {
        return Ok(StartManualRunResult::Failed {
            reason: StartFailReason::AlreadyRunning,
            message: "An autonomous run is already in progress for this room.".to_string(),
        });
    }

    // Anything that isn't a live paused run starts fresh (v4 `!chat.currentRunId`
    // is a truthiness test — an absent or empty id both fall through).
    let current_run_id = chat_str(&chat, "currentRunId").filter(|s| !s.is_empty());
    if chat_str(&chat, "runState") != Some("paused") || current_run_id.is_none() {
        return start_autonomous_room_manually(db, deps, chat_id, user_id).await;
    }
    let run_id = current_run_id.unwrap_or_default().to_string();

    let now = (deps.now_ms)();

    // Exclude the paused interval from the wall-clock budget by accumulating it
    // into runPausedAccumMs — NOT by shifting runStartedAt (which also anchors
    // the token-accounting window).
    let mut run_paused_accum_ms = chat
        .get("runPausedAccumMs")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if let Some(paused_at) = chat_str(&chat, "runPausedAt") {
        // v4 `Number.isFinite(pausedForMs) && pausedForMs > 0` — an unparseable
        // stamp (NaN) or a non-positive interval is skipped.
        if let Some(paused_at_ms) = iso_to_ms(paused_at) {
            let paused_for_ms = now - paused_at_ms;
            if paused_for_ms > 0 {
                run_paused_accum_ms += paused_for_ms as f64;
            }
        }
    }

    let patch = ChatUpdate {
        run_state: Some("running".to_string()),
        run_state_message: Some(None),
        run_ended_at: Some(None),
        run_paused_at: Some(None),
        run_paused_accum_ms: Some(run_paused_accum_ms),
        // currentRunId / runStartedAt / runTurnsConsumed / runTokensConsumed are
        // deliberately left untouched — this continues the existing run.
        ..Default::default()
    };
    let cid = chat_id.to_string();
    db.write(move |writers| ChatsRepository::new(writers.main().connection()).update(&cid, &patch))
        .await?;

    let job_id = enqueue_autonomous_room_turn(db, user_id, chat_id, &run_id).await?;

    Ok(StartManualRunResult::Ok { run_id, job_id })
}

// ============================================================================
// updateAutonomousRoomSettings
// ============================================================================

/// v4 `AutonomousRoomSettingsPatch` — the Edit Enclave modal's partial patch.
/// Tri-state semantics field by field: `None` → leave untouched; `Some(None)` →
/// clear to NULL (the nullable columns); `Some(Some(v))` → write it. `title`
/// only ever *sets* (a non-empty trim also pins `isManuallyRenamed`); the two
/// booleans are plain optional.
#[derive(Default)]
pub struct AutonomousRoomSettingsPatch {
    pub title: Option<String>,
    pub schedule_cron: Option<Option<String>>,
    pub schedule_freshness_window_ms: Option<Option<f64>>,
    pub budget_max_turns: Option<Option<f64>>,
    pub budget_max_tokens: Option<Option<f64>>,
    pub budget_max_wall_clock_ms: Option<Option<f64>>,
    pub budget_estimated_spend_cap_usd: Option<Option<f64>>,
    pub run_visibility: Option<Option<String>>,
    pub run_destructive_tools_allowed: Option<bool>,
    pub budget_exclude_cache_hits: Option<bool>,
}

/// v4 `UpdateAutonomousRoomSettingsResult::reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateSettingsFailReason {
    NotAutonomous,
    ChatNotFound,
    InvalidCron,
}

impl UpdateSettingsFailReason {
    pub fn as_str(self) -> &'static str {
        match self {
            UpdateSettingsFailReason::NotAutonomous => "not_autonomous",
            UpdateSettingsFailReason::ChatNotFound => "chat_not_found",
            UpdateSettingsFailReason::InvalidCron => "invalid_cron",
        }
    }
}

/// v4 `UpdateAutonomousRoomSettingsResult`.
#[derive(Debug)]
pub enum UpdateAutonomousRoomSettingsResult {
    Ok {
        clamped_destructive: bool,
    },
    Failed {
        reason: UpdateSettingsFailReason,
        message: String,
    },
}

/// v4 `updateAutonomousRoomSettings(chatId, userId, patch)`.
///
/// Edits land on the live chat row and take effect on the *next* turn of any
/// in-flight run; run-state and counters are deliberately untouched, so editing
/// never restarts or resets a run. Cron is validated here — a bad expression
/// rejects the WHOLE edit (nothing written); setting one recomputes
/// `scheduleNextRunAt` (via the injected cron seam, anchored at `now`); clearing
/// drops the room to manual-only. The user-level `destructiveToolPolicy` ceiling
/// is enforced at write time (`always_refuse` forces the per-room flag off).
/// An all-empty effective patch returns `Ok` without touching the row.
pub async fn update_autonomous_room_settings(
    db: &Db,
    deps: &LifecycleDeps<'_>,
    chat_id: &str,
    user_id: &str,
    patch: &AutonomousRoomSettingsPatch,
) -> Result<UpdateAutonomousRoomSettingsResult, DbError> {
    let cid = chat_id.to_string();
    let chat = db.read_main(|conn| chats_read::find_by_id(conn, &cid))?;
    let Some(chat) = chat else {
        return Ok(UpdateAutonomousRoomSettingsResult::Failed {
            reason: UpdateSettingsFailReason::ChatNotFound,
            message: "Chat not found.".to_string(),
        });
    };
    if chat_str(&chat, "chatType") != Some("autonomous") {
        return Ok(UpdateAutonomousRoomSettingsResult::Failed {
            reason: UpdateSettingsFailReason::NotAutonomous,
            message: "This chat is not an autonomous room.".to_string(),
        });
    }

    let mut update = ChatUpdate::default();
    let mut fields_set = 0usize;

    // --- Title (also pins isManuallyRenamed so the auto-titler backs off) ---
    if let Some(title) = &patch.title {
        let trimmed = js_trim(title);
        if !trimmed.is_empty() {
            update.title = Some(trimmed.to_string());
            update.is_manually_renamed = Some(true);
            fields_set += 2;
        }
    }

    // --- Schedule (three-way: set+recompute / clear / reject-on-invalid) ---
    if let Some(cron_patch) = &patch.schedule_cron {
        // v4 `patch.scheduleCron?.trim() ?? ''`.
        let cron = cron_patch.as_deref().map(js_trim).unwrap_or("");
        if cron.is_empty() {
            update.schedule_cron = Some(None);
            update.schedule_next_run_at = Some(None);
            fields_set += 2;
        } else {
            // v4 `new Cron(cron).nextRun(new Date())?.toISOString() ?? null` in
            // a try/catch that rejects the whole edit on an invalid expression.
            let anchor = (deps.now_ms)();
            match (deps.next_occurrence)(cron, anchor) {
                Err(_) => {
                    return Ok(UpdateAutonomousRoomSettingsResult::Failed {
                        reason: UpdateSettingsFailReason::InvalidCron,
                        message: format!("Invalid cron expression: {cron}"),
                    });
                }
                Ok(next) => {
                    update.schedule_cron = Some(Some(cron.to_string()));
                    update.schedule_next_run_at = Some(next.map(iso_from_unix_ms));
                    fields_set += 2;
                }
            }
        }
    }

    // --- Catch-up freshness window ---
    if let Some(v) = &patch.schedule_freshness_window_ms {
        update.schedule_freshness_window_ms = Some(*v);
        fields_set += 1;
    }

    // --- Budget caps (each nullable: null clears the cap) ---
    if let Some(v) = &patch.budget_max_turns {
        update.budget_max_turns = Some(*v);
        fields_set += 1;
    }
    if let Some(v) = &patch.budget_max_tokens {
        update.budget_max_tokens = Some(*v);
        fields_set += 1;
    }
    if let Some(v) = &patch.budget_max_wall_clock_ms {
        update.budget_max_wall_clock_ms = Some(*v);
        fields_set += 1;
    }
    if let Some(v) = &patch.budget_estimated_spend_cap_usd {
        update.budget_estimated_spend_cap_usd = Some(*v);
        fields_set += 1;
    }

    // --- Token-budget counting mode (default: exclude cache hits) ---
    if let Some(v) = patch.budget_exclude_cache_hits {
        // v4 `=== false ? 0 : 1` (the NUMBER, not a bool).
        update.budget_exclude_cache_hits = Some(if v { 1.0 } else { 0.0 });
        fields_set += 1;
    }

    // --- Visibility (null = inherit the user default) ---
    if let Some(v) = &patch.run_visibility {
        update.run_visibility = Some(v.clone());
        fields_set += 1;
    }

    // --- Destructive-tool authorization, clamped by the user-level ceiling ---
    let mut clamped_destructive = false;
    if let Some(requested) = patch.run_destructive_tools_allowed {
        // v4 `repos.chatSettings.findByUserId` — safeQuery (errors → null).
        let uid = user_id.to_string();
        let settings = db
            .read_main(|conn| chat_settings::find_by_user_id(conn, &uid))
            .unwrap_or(None);
        let policy_refuse = settings
            .as_ref()
            .and_then(|s| s.get("autonomousRoomSettings"))
            .and_then(|a| a.get("destructiveToolPolicy"))
            .and_then(Value::as_str)
            == Some("always_refuse");
        let allowed = if policy_refuse { false } else { requested };
        clamped_destructive = policy_refuse && requested;
        update.run_destructive_tools_allowed = Some(if allowed { 1.0 } else { 0.0 });
        fields_set += 1;
    }

    if fields_set == 0 {
        return Ok(UpdateAutonomousRoomSettingsResult::Ok {
            clamped_destructive,
        });
    }

    let cid = chat_id.to_string();
    db.write(move |writers| {
        ChatsRepository::new(writers.main().connection()).update(&cid, &update)
    })
    .await?;

    Ok(UpdateAutonomousRoomSettingsResult::Ok {
        clamped_destructive,
    })
}

// ============================================================================
// Reconciliation (startup + failed-turn)
// ============================================================================

/// v4 `buildResumablePausePatch(chat, runStateMessage, nowIso)` — pause a run
/// *resumably in place*: a fresh `currentRunId` (so any zombie/retry turn job
/// exits via the stale-run guard), `runEndedAt` cleared, `runPausedAt` anchored
/// to the last activity (`lastMessageAt ?? runStartedAt ?? nowIso` — v4's
/// nullish-coalescing chain, NOT a max) so a later resume excludes the outage
/// from the wall-clock budget. Counters are deliberately untouched. Single
/// source for both the startup reconcile and the terminal-failure reconcile.
fn build_resumable_pause_patch(
    chat: &Value,
    run_state_message: String,
    now_iso: &str,
    mint_uuid: MintUuidFn<'_>,
) -> ChatUpdate {
    let run_paused_at = chat_str(chat, "lastMessageAt")
        .or_else(|| chat_str(chat, "runStartedAt"))
        .unwrap_or(now_iso)
        .to_string();
    ChatUpdate {
        run_state: Some("paused".to_string()),
        run_state_message: Some(Some(run_state_message)),
        current_run_id: Some(mint_uuid()),
        run_ended_at: Some(None),
        run_paused_at: Some(Some(run_paused_at)),
        ..Default::default()
    }
}

/// v4 `reconcileAutonomousRunsAtStartup()` — the startup sweep for runs
/// interrupted by a crash/restart: every `chatType='autonomous'` chat stuck at
/// `runState='running'` is transitioned to `paused` (resumable in place —
/// counters and transcript preserved; `resumeAutonomousRoom` continues it),
/// with `restart:interrupted` recorded and `currentRunId` bumped so any zombie
/// turn job re-claimed by the orphan reset exits via the stale-run guard.
/// Idempotent — when nothing is stuck this is a findAll + filter with no writes
/// (and no clock read). Returns the reconciled count.
pub async fn reconcile_autonomous_runs_at_startup(
    db: &Db,
    deps: &LifecycleDeps<'_>,
) -> Result<usize, DbError> {
    let all_chats = db.read_main(chats_read::find_all)?;
    let stuck: Vec<&Value> = all_chats
        .iter()
        .filter(|c| {
            chat_str(c, "chatType") == Some("autonomous")
                && chat_str(c, "runState") == Some("running")
        })
        .collect();

    if stuck.is_empty() {
        return Ok(0);
    }

    let now_iso = iso_from_unix_ms((deps.now_ms)());
    for chat in &stuck {
        let Some(id) = chat_str(chat, "id") else {
            continue;
        };
        let patch = build_resumable_pause_patch(
            chat,
            "restart:interrupted".to_string(),
            &now_iso,
            deps.mint_uuid,
        );
        let cid = id.to_string();
        db.write(move |writers| {
            ChatsRepository::new(writers.main().connection()).update(&cid, &patch)
        })
        .await?;
    }
    Ok(stuck.len())
}

/// v4 `reconcileFailedAutonomousTurn(payload, failureReason)` — recover a room
/// whose turn job failed *terminally* (the single-attempt job went DEAD with the
/// run-state write rolled back, leaving the row falsely `running`). Mirrors the
/// startup reconcile: `paused` (resumable), `currentRunId` bumped, counters
/// preserved, the cause recorded as `turn_failed:{reason}` (UTF-16-sliced to
/// 500). Acts only on the live run (`currentRunId` matches) while it still looks
/// active (`running` or `idle`).
///
/// Fully guarded — a failure here never propagates (v4 wraps the whole body in
/// try/catch so the dispatcher's result handling can't be poisoned).
pub async fn reconcile_failed_autonomous_turn(
    db: &Db,
    deps: &LifecycleDeps<'_>,
    payload_chat_id: Option<&str>,
    payload_run_id: Option<&str>,
    failure_reason: &str,
) {
    // JS truthiness: a missing OR empty id returns early.
    let Some(chat_id) = payload_chat_id.filter(|s| !s.is_empty()) else {
        return;
    };
    let Some(run_id) = payload_run_id.filter(|s| !s.is_empty()) else {
        return;
    };

    let cid = chat_id.to_string();
    let chat = match db.read_main(|conn| chats_read::find_by_id(conn, &cid)) {
        Ok(c) => c,
        Err(_) => return, // v4 catch → logged + swallowed
    };
    let Some(chat) = chat else { return };
    if chat_str(&chat, "chatType") != Some("autonomous") {
        return;
    }

    // Only act on the live run, and only while it still looks active. A newer
    // run (currentRunId moved on) or an already-terminal state means something
    // else has already taken over; leave it alone.
    if chat_str(&chat, "currentRunId") != Some(run_id) {
        return;
    }
    let run_state = chat_str(&chat, "runState");
    if run_state != Some("running") && run_state != Some("idle") {
        return;
    }

    let now_iso = iso_from_unix_ms((deps.now_ms)());
    // v4 `` `turn_failed:${failureReason}`.slice(0, 500) `` — UTF-16 units.
    let message = utf16_truncate(&format!("turn_failed:{failure_reason}"), 500);
    let patch = build_resumable_pause_patch(&chat, message, &now_iso, deps.mint_uuid);
    let cid = chat_id.to_string();
    let _ = db
        .write(move |writers| {
            ChatsRepository::new(writers.main().connection()).update(&cid, &patch)
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::runtime::Db;
    use crate::db::Writer;
    use serde_json::json;
    use std::sync::atomic::{AtomicI64, Ordering};

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// The full 97-column `chats` DDL in schema order (types loose — SQLite is
    /// dynamically typed and these unit tests only exercise the lifecycle's own
    /// reads/writes; the tier-2 differential runs on the real generated schema).
    fn chats_ddl() -> String {
        let cols = [
            "id TEXT PRIMARY KEY",
            "userId TEXT",
            "participants TEXT",
            "title TEXT",
            "contextSummary TEXT",
            "sillyTavernMetadata TEXT",
            "tags TEXT",
            "roleplayTemplateId TEXT",
            "timestampConfig TEXT",
            "lastTurnParticipantId TEXT",
            "messageCount REAL",
            "lastMessageAt TEXT",
            "lastRenameCheckInterchange REAL",
            "compactionGeneration REAL",
            "lastSummaryTurn REAL",
            "lastSummaryTokens REAL",
            "lastFullRebuildTurn REAL",
            "summaryAnchorMessageIds TEXT",
            "isPaused INTEGER",
            "isManuallyRenamed INTEGER",
            "impersonatingParticipantIds TEXT",
            "activeTypingParticipantId TEXT",
            "allLLMPauseTurnCount REAL",
            "turnQueue TEXT",
            "spokenThisCycleParticipantIds TEXT",
            "documentEditingMode INTEGER",
            "documentMode TEXT",
            "dividerPosition REAL",
            "terminalMode TEXT",
            "activeTerminalSessionId TEXT",
            "rightPaneVerticalSplit REAL",
            "projectId TEXT",
            "scenarioText TEXT",
            "totalPromptTokens REAL",
            "totalCompletionTokens REAL",
            "estimatedCostUSD REAL",
            "priceSource TEXT",
            "showSystemEventsOverride INTEGER",
            "requestFullContextOnNextMessage INTEGER",
            "disabledTools TEXT",
            "disabledToolGroups TEXT",
            "forceToolsOnNextMessage INTEGER",
            "allowCrossCharacterVaultReads INTEGER",
            "pendingOutfitNotifications TEXT",
            "state TEXT",
            "compressionCache TEXT",
            "agentModeEnabled INTEGER",
            "agentTurnCount REAL",
            "storyBackgroundImageId TEXT",
            "lastBackgroundGeneratedAt TEXT",
            "imageProfileId TEXT",
            "alertCharactersOfLanternImages INTEGER",
            "isDangerousChat INTEGER",
            "dangerScore REAL",
            "dangerCategories TEXT",
            "dangerClassifiedAt TEXT",
            "dangerClassifiedAtMessageCount REAL",
            "conciergeOverride TEXT",
            "sceneState TEXT",
            "renderedMarkdown TEXT",
            "equippedOutfit TEXT",
            "characterAvatars TEXT",
            "avatarGenerationEnabled INTEGER",
            "chatType TEXT",
            "helpPageUrl TEXT",
            "consoleConnectionProfileId TEXT",
            "compiledIdentityStacks TEXT",
            "courierCheckpoints TEXT",
            "commonplaceSceneCache TEXT",
            "commonplaceRecallHistory TEXT",
            "budgetMaxTurns REAL",
            "budgetMaxTokens REAL",
            "budgetMaxWallClockMs REAL",
            "budgetEstimatedSpendCapUSD REAL",
            "scheduleCron TEXT",
            "scheduleFreshnessWindowMs REAL",
            "scheduleNextRunAt TEXT",
            "scheduleLastRunAt TEXT",
            "runState TEXT",
            "currentRunId TEXT",
            "runStateMessage TEXT",
            "runStartedAt TEXT",
            "runEndedAt TEXT",
            "runPausedAt TEXT",
            "runPausedAccumMs REAL",
            "runTurnsConsumed REAL",
            "runTokensConsumed REAL",
            "runMilestonesAnnounced REAL",
            "runDestructiveToolsAllowed REAL",
            "budgetExcludeCacheHits REAL",
            "runVisibility TEXT",
            "coreWhisperEnabled INTEGER",
            "coreWhisperInterval REAL",
            "showThinking INTEGER",
            "createdAt TEXT",
            "updatedAt TEXT",
            "answerConfirmationOverride TEXT",
        ];
        format!("CREATE TABLE chats ({});", cols.join(", "))
    }

    /// A main-only Db with a `chats` table and one seeded autonomous chat — and
    /// deliberately NO `background_jobs` table, so any enqueue fails (the
    /// rollback branch the real-DB oracle cannot force).
    async fn make_db_without_jobs_table(seed_run_state: &str) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(&chats_ddl()).unwrap();
            w.connection()
                .execute(
                    "INSERT INTO chats (id, userId, participants, title, chatType, runState, \
                     turnQueue, spokenThisCycleParticipantIds, createdAt, updatedAt) \
                     VALUES ('chat-1', 'user-1', ?1, 'Room', 'autonomous', ?2, '[]', '[]', \
                     '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
                    rusqlite::params![
                        json!([{
                            "id": "p-1", "type": "CHARACTER", "characterId": "c-1",
                            "controlledBy": "user", "status": "active",
                            "createdAt": "2020-01-01T00:00:00.000Z",
                            "updatedAt": "2020-01-01T00:00:00.000Z",
                        }])
                        .to_string(),
                        seed_run_state,
                    ],
                )
                .unwrap();
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn stepped_clock(base: i64) -> (std::sync::Arc<AtomicI64>, impl Fn() -> i64 + Send + Sync) {
        let counter = std::sync::Arc::new(AtomicI64::new(base));
        let c = counter.clone();
        (counter, move || c.fetch_add(1, Ordering::SeqCst))
    }

    fn seq_uuids(prefix: &'static str) -> impl Fn() -> String + Send + Sync {
        let n = AtomicI64::new(0);
        move || {
            let i = n.fetch_add(1, Ordering::SeqCst);
            format!("{prefix}-{i}")
        }
    }

    fn chat_row(db: &Db) -> Value {
        db.read_main(|conn| chats_read::find_by_id(conn, "chat-1"))
            .unwrap()
            .unwrap()
    }

    /// The enqueue-failure ROLLBACK branch (v4 lines 104–124): the row flips to
    /// `running` first, the turn enqueue fails (no `background_jobs` table),
    /// and the row rolls back to `error` + `start:enqueue_failed` + `runEndedAt`
    /// for the manual path — which re-throws the cause.
    #[tokio::test]
    async fn begin_rollback_on_enqueue_failure_manual_path() {
        let (_dir, db) = make_db_without_jobs_table("idle").await;
        let (_c, now_ms) = stepped_clock(1_925_000_000_000);
        let mint = seq_uuids("uuid");
        let cron: NextOccurrenceFn = &|_, _| Ok(None);
        let deps = LifecycleDeps {
            now_ms: &now_ms,
            mint_uuid: &mint,
            next_occurrence: cron,
        };

        let err = start_autonomous_room_manually(&db, &deps, "chat-1", "user-1")
            .await
            .expect_err("manual start must re-throw the enqueue cause");
        // The cause is the underlying SQLite error (missing table).
        assert!(matches!(err, DbError::Sqlite(_)), "cause: {err:?}");

        let chat = chat_row(&db);
        assert_eq!(chat["runState"], "error");
        assert_eq!(chat["runStateMessage"], "start:enqueue_failed");
        assert!(chat.get("runEndedAt").is_some(), "runEndedAt stamped");
        // The run-start patch landed before the rollback: counters zeroed and
        // the minted runId still present (v4 leaves currentRunId in place).
        assert_eq!(chat["currentRunId"], "uuid-0");
        assert_eq!(chat["runTurnsConsumed"], 0.0);
        // updatedAt never minted.
        assert_eq!(chat["updatedAt"], "2020-01-01T00:00:00.000Z");
    }

    /// The scheduled entry rolls back to `idle` + `schedule:enqueue_failed` and
    /// returns the failure (no throw) — the tick's next slot retries.
    #[tokio::test]
    async fn begin_rollback_on_enqueue_failure_scheduled_path() {
        let (_dir, db) = make_db_without_jobs_table("idle").await;
        let (_c, now_ms) = stepped_clock(1_925_000_000_000);
        let mint = seq_uuids("uuid");
        let cron: NextOccurrenceFn = &|_, _| Ok(None);
        let deps = LifecycleDeps {
            now_ms: &now_ms,
            mint_uuid: &mint,
            next_occurrence: cron,
        };

        let result = start_scheduled_autonomous_run(
            &db,
            &deps,
            BeginAutonomousRunInput {
                chat_id: "chat-1".to_string(),
                user_id: "user-1".to_string(),
                run_id: "run-sched".to_string(),
                now_iso: "2031-01-01T00:00:00.000Z".to_string(),
                schedule_last_run_at: Some(Some("2031-01-01T00:00:00.000Z".to_string())),
                schedule_next_run_at: Some(None),
                on_enqueue_failure: OnEnqueueFailure::Idle,
            },
        )
        .await
        .expect("scheduled path returns the failure instead of throwing");

        match result {
            BeginAutonomousRunResult::Failed {
                reason,
                message,
                cause,
            } => {
                assert_eq!(reason, BeginFailReason::EnqueueFailed);
                assert_eq!(message, "schedule:enqueue_failed");
                assert!(cause.is_some());
            }
            _ => panic!("expected enqueue failure"),
        }

        let chat = chat_row(&db);
        assert_eq!(chat["runState"], "idle");
        assert_eq!(chat["runStateMessage"], "schedule:enqueue_failed");
        assert_eq!(chat["scheduleLastRunAt"], "2031-01-01T00:00:00.000Z");
        // The tri-state `Some(None)` cleared the slot (column NULL → key absent).
        assert!(chat.get("scheduleNextRunAt").is_none());
    }

    /// `reconcile_failed_autonomous_turn` never propagates an error (here the
    /// chats table read works but the stale-run guard rejects).
    #[tokio::test]
    async fn reconcile_failed_guards() {
        let (_dir, db) = make_db_without_jobs_table("running").await;
        let (_c, now_ms) = stepped_clock(1_925_000_000_000);
        let mint = seq_uuids("uuid");
        let cron: NextOccurrenceFn = &|_, _| Ok(None);
        let deps = LifecycleDeps {
            now_ms: &now_ms,
            mint_uuid: &mint,
            next_occurrence: cron,
        };

        // Missing payload halves → no-op; wrong run id → no-op (seed has no
        // currentRunId at all, so any id mismatches).
        reconcile_failed_autonomous_turn(&db, &deps, None, Some("r"), "boom").await;
        reconcile_failed_autonomous_turn(&db, &deps, Some("chat-1"), None, "boom").await;
        reconcile_failed_autonomous_turn(&db, &deps, Some("chat-1"), Some("stale"), "boom").await;
        let chat = chat_row(&db);
        assert_eq!(chat["runState"], "running", "untouched by the guards");
    }

    #[test]
    fn turn_failed_message_slices_to_500_utf16() {
        let long = "x".repeat(600);
        let s = utf16_truncate(&format!("turn_failed:{long}"), 500);
        assert_eq!(s.encode_utf16().count(), 500);
        assert!(s.starts_with("turn_failed:xxxx"));
    }
}
