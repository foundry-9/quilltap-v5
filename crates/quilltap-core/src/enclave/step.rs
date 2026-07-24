//! `step()` + the schedule tick (U4.4, the enclave capstone) — v4
//! `lib/background-jobs/handlers/autonomous-room-turn.ts`
//! (`handleAutonomousRoomTurn`, whole file) +
//! `autonomous-room-schedule-tick.ts` (`handleAutonomousRoomScheduleTick` +
//! `healWedgedRuns`, whole file).
//!
//! One persisted transition per claimed job (api-boundary.md Part 3): [`step`]
//! runs ONE autonomous-room turn — guards → pre-turn budget → speaker selection
//! → the ordinary send spine ([`crate::services::orchestrator::process_message`]
//! with the autonomous flags) → post-turn accounting → budget re-check →
//! milestones → the summary fold → self-re-enqueue — and returns. **No loop, no
//! timer**: the continuation is the `AUTONOMOUS_ROOM_TURN` job the handler
//! itself enqueues (v4's shape), and cadence belongs to the host driver.
//!
//! ## Writes go through `Db` directly (the encoded design decision)
//!
//! The spec doc's decision #3 originally routed the turn through
//! [`crate::write_apply`] (v4's main-primary batch apply). That batch model
//! exists to make the forked child's buffered writes atomic; v5's handlers run
//! in-process over the single-writer [`Db`] (the W4.8 non-port), and
//! `process_message` — which the turn calls — is itself a direct-writing
//! service. Batching the turn would re-introduce the buffered-proxy model the
//! port deliberately rejected. Decisive: the U4.4 differential's ORACLE side
//! runs v4 UNFORKED (no `QUILLTAP_JOB_CHILD`), so v4's oracle behavior is
//! direct-write too, and the differential pins exactly that. v4's
//! local-snapshot pinning (`chat.runTurnsConsumed` over `post.*` etc.) is still
//! REPRODUCED FAITHFULLY — see the step-8/9 comments, which carry v4's
//! buffered-read rationale adapted to note its origin.
//!
//! ## The error-swallowing shell (why `turn_error:` is dead code)
//!
//! v4 wraps the whole turn in `handleSendMessage`'s `ReadableStream` shell,
//! whose `start()` catch (`handleStreamError`) SWALLOWS every error — it emits
//! an SSE `error` frame to the (drained, unobserved) stream and closes. The
//! turn handler's own `turn_error:` catch therefore fires only when
//! `handleSendMessage` ITSELF rejects, which the v4 source at `6bf88959`
//! cannot do (its body is a synchronous `ReadableStream` construction). So in
//! v4 a mid-turn model/DB failure does NOT transition the run to `error` — the
//! post-turn bookkeeping proceeds (the turn is counted, the run re-enqueues).
//! Reproduced faithfully: [`step`] swallows a `process_message` error the same
//! way, and the `turn_error:` transition is documented dead code (NOT ported —
//! inventing a v5-only trigger would fire where v4 would not). A failure in
//! the turn handler's OWN reads/writes (v4's steps 1–5 / 7.5–10, which sit
//! OUTSIDE the shell) propagates out of `step` → the job fails → the runner's
//! `reconcile_failed_autonomous_turn` hook (v4 `job-dispatcher.ts:230`).
//!
//! ## The clock / UUID / timezone seams
//!
//! Injected per the [`crate::enclave::lifecycle`] precedent (`now_ms` /
//! `mint_uuid`; the differential steps the clock +1 ms per read to mirror the
//! oracle's Date mock). Cron evaluation + the instance-local midnight use the
//! injected IANA `tz` over [`crate::enclave::cron`] / jiff — v4 runs on the
//! host process's local zone.

use serde_json::Value;

use crate::clock::{iso_from_unix_ms, iso_to_ms};
use crate::db::background_jobs::BackgroundJobsRepository;
use crate::db::chats::{ChatUpdate, ChatsRepository};
use crate::db::llm_logs::LLMLogsRepository;
use crate::db::runtime::Db;
use crate::db::{chat_settings, chats_messages_read, chats_read, connection_profiles, DbError};
use crate::enclave::announce::{
    post_autonomous_room_announcement, post_run_start_announcement, AutonomousRoomKind, MintUuidFn,
    NowMsFn,
};
use crate::enclave::lifecycle::{
    start_scheduled_autonomous_run, BeginAutonomousRunInput, BeginAutonomousRunResult,
    LifecycleDeps, OnEnqueueFailure, DEFAULT_FRESHNESS_WINDOW_MS,
};
use crate::enclave::milestones::{
    build_milestone_message, decide_milestone, grant_grace_mask, post_turn_exhausted_action,
    pre_turn_exhausted_action, ExhaustedAction, GRACE_CONTENT, GRACE_OPAQUE,
};
use crate::enclave_budget::{
    check_budget, compute_autonomous_context_cap, compute_budget_progress, BudgetCheck,
    BudgetState, NextRunState,
};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::model::stream::StreamingCompletionProvider;
use crate::participant_filters::get_active_character_participants;
use crate::select_speaker::select_next_speaker;
use crate::services::build_context::BuildContextSeams;
use crate::services::carina_runner::{PostProsperoCarinaError, RunCarinaQuery};
use crate::services::chat_events::EventSink;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::llm_logging::LogContext;
use crate::services::message_finalizer::{
    AnswerConfirmationRunner, AsyncCompressionTrigger, CostTracker,
};
use crate::services::orchestrator::{
    process_message, OrchestratorDeps, OrchestratorSeams, ProcessClock, ProcessMessageInput,
    SendMessageOptions,
};
use crate::services::pricing_fetcher::PricingFetch;
use crate::services::provider_failover::DangerousContentRouter;
use crate::services::queue_service::enqueue_autonomous_room_turn;
use crate::services::turn_orchestrator::{
    load_talkativeness_map, participants_array, to_filter_participant, to_message_views,
    to_speaker_participant,
};
use crate::turn_state::calculate_turn_state_from_history;

// v4 `WEDGE_GRACE_MS` (autonomous-room-schedule-tick.ts:30) — a `running` room
// is only treated as wedged once untouched this long (a freshly-started run has
// a just-bumped `updatedAt` and a turn job that may not be visible yet).
pub const WEDGE_GRACE_MS: i64 = 60 * 1000;

// ---------------------------------------------------------------------------
// Payload + injected deps
// ---------------------------------------------------------------------------

/// v4 `AutonomousRoomTurnPayload` (`queue-service.ts`). Fields are optional at
/// decode (v4 does no validation — an absent key reads `undefined`; the corpus
/// always carries both).
#[derive(Clone, Debug)]
pub struct AutonomousRoomTurnPayload {
    pub chat_id: Option<String>,
    pub run_id: Option<String>,
}

impl AutonomousRoomTurnPayload {
    /// Decode from the stored payload JSON (missing/non-string → `None`).
    pub fn from_json(payload: &Value) -> Self {
        let get = |k: &str| payload.get(k).and_then(Value::as_str).map(str::to_string);
        Self {
            chat_id: get("chatId"),
            run_id: get("runId"),
        }
    }
}

/// The claimed job's own metadata the turn handler reads (the concurrency
/// tie-break compares `(createdAt, id)` against PROCESSING siblings).
pub struct TurnJobMeta<'a> {
    pub job_id: &'a str,
    pub job_created_at: &'a str,
    pub user_id: &'a str,
}

/// The step-level injected impurities + the `ProcessMessageInput` scaffolding
/// (the pieces the spine resolves above its seams — the orchestrator-harness
/// precedent).
pub struct StepDeps<'a> {
    /// v4 `Date.now()` / `new Date()` — the millisecond wall clock (stepped +1
    /// per read in the differential, mirroring the oracle's Date mock).
    pub now_ms: NowMsFn<'a>,
    /// v4 `randomUUID()`.
    pub mint_uuid: MintUuidFn<'a>,
    /// The instance's IANA timezone: cron evaluation + the daily-cap
    /// instance-local midnight (v4 uses the host process's local zone).
    pub tz: &'a str,
    /// `Math.random()` for the weighted next-speaker pick.
    pub random01: f64,
    /// The UNTAGGED cheap-LLM executor for the 9c summary fold. v4 runs the
    /// fold OUTSIDE the `runWithAutonomousRunId` scope — `getAutonomousRunId()`
    /// is null there, so the fold's cheap-LLM tokens are housekeeping, not turn
    /// spend. The TURN's executor (inside [`OrchestratorDeps`]) carries the
    /// run's [`LogContext`]; this one carries none.
    pub fold_executor: &'a CheapLlmTaskExecutor,
    // --- ProcessMessageInput scaffolding (resolved above the spine's seams) ---
    pub model_context_limit: i64,
    pub timestamp_config: Option<crate::chat_timestamp::TimestampConfig>,
    /// buildContext's resolved timezone (distinct from `tz`, which is the
    /// cron/midnight zone — both are `UTC` in the differential).
    pub timezone: Option<String>,
    pub local_offset_minutes: i64,
    pub provider_supports_web_search: bool,
}

// ---------------------------------------------------------------------------
// Outcomes
// ---------------------------------------------------------------------------

/// What one [`step`] did. The DB writes are the source of truth; the outcome
/// exists for the runner + tests (v4's handler returns void and logs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    /// Guard 1: chat missing (warn + exit in v4).
    ChatNotFound,
    /// Guard 1: `chatType !== 'autonomous'`.
    NotAutonomous,
    /// Guard 2: `chat.currentRunId !== payload.runId` (superseded by a newer run).
    StaleRun,
    /// Guard 2a: an elder PROCESSING sibling turn job exists — yield to it.
    YieldedToSibling,
    /// Guard 3: `paused | stopped | budgetExhausted | error`.
    RunNotActive,
    /// Pre-turn budget end (`budget:<reason>`; the daily cap pauses, others end).
    EndedPreTurn { reason: String, next_state: String },
    /// Pre-turn grace turn granted (re-enqueued).
    GraceGrantedPreTurn,
    /// Speaker selection found nobody (`no_eligible_speaker:<reason>` → `error`).
    NoEligibleSpeaker { reason: String },
    /// The step-7.5 / step-9 re-read found `currentRunId` moved on mid-turn.
    Superseded,
    /// Post-turn budget end.
    EndedPostTurn { reason: String, next_state: String },
    /// Post-turn grace turn granted (re-enqueued).
    GraceGrantedPostTurn,
    /// The run continues — the next turn is enqueued.
    ReEnqueued,
}

/// What one [`schedule_tick`] did (v4's tick-complete log counters).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickOutcome {
    pub autonomous_chats: usize,
    pub due_count: usize,
    pub stale_count: usize,
    pub enqueued_count: usize,
    pub healed_count: usize,
}

// ---------------------------------------------------------------------------
// Small shared helpers
// ---------------------------------------------------------------------------

fn chat_str<'a>(chat: &'a Value, key: &str) -> Option<&'a str> {
    chat.get(key).and_then(Value::as_str)
}

fn chat_f64(chat: &Value, key: &str) -> Option<f64> {
    chat.get(key).and_then(Value::as_f64)
}

/// Render a JS number for template-literal interpolation (`${n}`): an
/// integer-valued double renders bare (`3.0` → `"3"`).
fn js_number_string(n: f64) -> String {
    crate::db::js_number_to_json(n).to_string()
}

/// v4 `n.toLocaleString()` over a token count (en-US integer grouping; a
/// schema-impossible fractional value falls back to the bare JS rendering —
/// the [`crate::enclave::announce`] precedent).
fn locale_number_string(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 9.2e18 {
        crate::services::primary_stream::to_locale_string(n as i64)
    } else {
        js_number_string(n)
    }
}

/// JS `Math.round(x)` = `floor(x + 0.5)` (half toward +∞).
fn js_math_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// v4 `lastLocalMidnightIso(now)` (autonomous-room-turn.ts:75): the ISO
/// timestamp of the most recent instance-local midnight. v4 constructs
/// `new Date(y, m, d, 0, 0, 0, 0)` in the process's local zone; here the zone
/// is the injected IANA `tz` via jiff (a nonexistent local midnight — a
/// spring-forward gap at 00:00 — resolves by jiff's compatible disambiguation,
/// matching V8's gap handling; the differential pins `UTC`, where midnight
/// always exists).
pub fn last_local_midnight_iso(now_ms: i64, tz: &str) -> String {
    use jiff::tz::TimeZone;
    let Ok(zone) = TimeZone::get(tz) else {
        // Defensive: v4 cannot reach this (the host's own zone always resolves).
        return iso_from_unix_ms(now_ms);
    };
    let Ok(ts) = jiff::Timestamp::from_millisecond(now_ms) else {
        return iso_from_unix_ms(now_ms);
    };
    let zoned = ts.to_zoned(zone.clone());
    let midnight = zoned
        .date()
        .to_datetime(jiff::civil::time(0, 0, 0, 0))
        .to_zoned(zone);
    match midnight {
        Ok(z) => iso_from_unix_ms(z.timestamp().as_millisecond()),
        Err(_) => iso_from_unix_ms(now_ms),
    }
}

/// v4 `recomputeNextRun(cronExpr, anchor)` (autonomous-room-turn.ts:375): the
/// next cron occurrence strictly after the anchor, as ISO. `None` when the cron
/// is missing/invalid/never-fires — the caller leaves `scheduleNextRunAt`
/// untouched so a misconfigured cron does not clear the timestamp.
fn recompute_next_run(cron_expr: Option<&str>, anchor_ms: i64, tz: &str) -> Option<String> {
    let expr = cron_expr?;
    if expr.is_empty() {
        // v4 `if (!cronExpr) return null` — JS falsy includes the empty string.
        return None;
    }
    crate::enclave::cron::next_occurrence(expr, anchor_ms, tz).map(iso_from_unix_ms)
}

/// v4 `nextCronFireFrom(cronExpr, anchor)` (schedule-tick.ts:41) — identical
/// collapse (parse error → warn + null), different call-shape (a required expr).
fn next_cron_fire_from(cron_expr: &str, anchor_ms: i64, tz: &str) -> Option<i64> {
    crate::enclave::cron::next_occurrence(cron_expr, anchor_ms, tz)
}

/// v4 `transitionRunState(chatId, to, extra)` — one `chats.update` with
/// `runState` + the extras (never mints `updatedAt`).
async fn transition_run_state(db: &Db, chat_id: &str, patch: ChatUpdate) -> Result<(), DbError> {
    let cid = chat_id.to_string();
    db.write(move |writers| ChatsRepository::new(writers.main().connection()).update(&cid, &patch))
        .await?;
    Ok(())
}

/// The [`BudgetState`] view of a hydrated chat `Value` (v4 passes the chat
/// object straight into `checkBudget`; `Date.parse` of an unparseable
/// `runStartedAt` yields NaN, whose comparisons are all false — `None` here
/// skips the wall-clock gate identically).
fn budget_state_from(chat: &Value) -> BudgetState {
    BudgetState {
        budget_max_turns: chat_f64(chat, "budgetMaxTurns").map(|v| v as i64),
        budget_max_tokens: chat_f64(chat, "budgetMaxTokens").map(|v| v as i64),
        budget_max_wall_clock_ms: chat_f64(chat, "budgetMaxWallClockMs").map(|v| v as i64),
        run_started_at_ms: chat_str(chat, "runStartedAt").and_then(iso_to_ms),
        run_paused_accum_ms: chat_f64(chat, "runPausedAccumMs").map(|v| v as i64),
        run_turns_consumed: chat_f64(chat, "runTurnsConsumed").map(|v| v as i64),
        run_tokens_consumed: chat_f64(chat, "runTokensConsumed").map(|v| v as i64),
    }
}

/// v4 `grantGraceTurn` (autonomous-room-turn.ts:399): record the grace bit,
/// have the Host invite a final word, and re-enqueue one more turn — without
/// ending the run. Exactly one grace turn is ever granted per run, since the
/// bit is checked before granting.
async fn grant_grace_turn(
    db: &Db,
    now_ms: NowMsFn<'_>,
    mint_uuid: MintUuidFn<'_>,
    chat_id: &str,
    user_id: &str,
    run_id: &str,
    current_mask: i64,
) -> Result<(), DbError> {
    transition_run_state(
        db,
        chat_id,
        ChatUpdate {
            // v4 updates ONLY `runMilestonesAnnounced` here (no runState field
            // in the patch — `grantGraceTurn` calls `repos.chats.update`
            // directly, not `transitionRunState`).
            run_milestones_announced: Some(grant_grace_mask(current_mask) as f64),
            ..Default::default()
        },
    )
    .await?;
    post_autonomous_room_announcement(
        db,
        now_ms,
        mint_uuid,
        chat_id,
        AutonomousRoomKind::Grace,
        GRACE_CONTENT,
        Some(GRACE_OPAQUE),
    )
    .await;
    enqueue_autonomous_room_turn(db, user_id, chat_id, run_id).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// step() — v4 handleAutonomousRoomTurn
// ---------------------------------------------------------------------------

/// One autonomous-room turn (v4 `handleAutonomousRoomTurn`, whole function).
/// See the module docs for the shell-swallow + direct-write decisions. The
/// spine deps (`orch`) must carry the run-tagged executor + the run's
/// [`LogContext`] is threaded into the `process_message` input here.
#[allow(clippy::too_many_lines, clippy::type_complexity)]
pub async fn step<EMB, CMP, STR, SNK, BCS, ORC, RTR, CONF, ACOMP, COST, CARQ, PROS, PF>(
    orch: &mut OrchestratorDeps<
        '_,
        EMB,
        CMP,
        STR,
        SNK,
        BCS,
        ORC,
        RTR,
        CONF,
        ACOMP,
        COST,
        CARQ,
        PROS,
        PF,
    >,
    sdeps: &StepDeps<'_>,
    job: &TurnJobMeta<'_>,
    payload: &AutonomousRoomTurnPayload,
) -> Result<StepOutcome, DbError>
where
    EMB: EmbeddingProvider + Sync,
    CMP: CompletionProvider + Sync,
    STR: StreamingCompletionProvider,
    SNK: EventSink + Sync,
    BCS: BuildContextSeams,
    ORC: OrchestratorSeams,
    RTR: DangerousContentRouter,
    CONF: AnswerConfirmationRunner,
    ACOMP: AsyncCompressionTrigger,
    COST: CostTracker,
    CARQ: RunCarinaQuery,
    PROS: PostProsperoCarinaError,
    PF: PricingFetch + Send + Sync,
{
    let db = orch.db;
    let user_id = job.user_id.to_string();

    // --- 1. Resolve chat ---
    let Some(chat_id) = payload.chat_id.clone() else {
        // v4 `findById(undefined)` → not found → warn + exit.
        return Ok(StepOutcome::ChatNotFound);
    };
    let cid = chat_id.clone();
    let chat = db.read_main(move |conn| chats_read::find_by_id(conn, &cid))?;
    let Some(mut chat) = chat else {
        return Ok(StepOutcome::ChatNotFound);
    };
    if chat_str(&chat, "chatType") != Some("autonomous") {
        return Ok(StepOutcome::NotAutonomous);
    }

    // --- 2. Stale-run guard ---
    // v4 `chat.currentRunId !== runId` — an absent column and an absent payload
    // key are BOTH `undefined`, which compare equal (strict), so two absents
    // pass the guard.
    if chat_str(&chat, "currentRunId") != payload.run_id.as_deref() {
        return Ok(StepOutcome::StaleRun);
    }
    // From here v4 uses `runId` freely; an all-absent pass-through would carry
    // `undefined` (corpus-unreachable — every enqueue writes both keys).
    let run_id = payload.run_id.clone().unwrap_or_default();

    // --- 2a. Concurrency guard ---
    // The stale-run guard only catches jobs whose runId was *displaced* by a
    // newer run; it does NOT catch two turn jobs PROCESSING in parallel with
    // the same runId (v4: the dispatcher's stuck-job sweep re-claiming a hung
    // job while the original handler is still alive). Resolution: if another
    // PROCESSING AUTONOMOUS_ROOM_TURN for this chat has an earlier
    // (createdAt, id) than mine, yield to it — the id tie-break makes the
    // decision deterministic at millisecond createdAt collisions.
    let cid = chat_id.clone();
    let in_flight =
        db.read_main(move |conn| BackgroundJobsRepository::new(conn).find_pending_for_chat(&cid))?;
    let elder_sibling = in_flight.iter().any(|j| {
        j.job_type == "AUTONOMOUS_ROOM_TURN"
            && j.id != job.job_id
            && j.status == "PROCESSING"
            // v4 compares ISO strings lexically (`j.createdAt < job.createdAt`).
            && (j.created_at.as_str() < job.job_created_at
                || (j.created_at.as_str() == job.job_created_at
                    && j.id.as_str() < job.job_id))
    });
    if elder_sibling {
        return Ok(StepOutcome::YieldedToSibling);
    }

    // --- 3. Lifecycle entry ---
    let run_state = chat_str(&chat, "runState").map(str::to_string);
    if matches!(
        run_state.as_deref(),
        Some("paused") | Some("stopped") | Some("budgetExhausted") | Some("error")
    ) {
        return Ok(StepOutcome::RunNotActive);
    }

    let now = (sdeps.now_ms)();
    let now_iso = iso_from_unix_ms(now);

    if run_state.is_none() || run_state.as_deref() == Some("idle") {
        // Defensive idle→running fallback. The run-start contract flips the row
        // straight to `running` at request time (the parent for a manual start,
        // the schedule-tick batch for a scheduled run), so a turn job normally
        // finds it `running` and skips this block. Kept for the only paths that
        // can still hand us an `idle` row: a turn job enqueued by a pre-upgrade
        // build, or a future caller that forgets the contract.
        transition_run_state(
            db,
            &chat_id,
            ChatUpdate {
                run_state: Some("running".to_string()),
                run_started_at: Some(now_iso.clone()),
                run_ended_at: Some(None),
                run_state_message: Some(None),
                run_turns_consumed: Some(0.0),
                run_tokens_consumed: Some(0.0),
                run_milestones_announced: Some(0.0),
                ..Default::default()
            },
        )
        .await?;
        // MUTATE the local snapshot — exactly the five fields v4 mutates. The
        // later phases (8 / 9 / 9b) deliberately read these LOCAL values, not a
        // re-read: in v4's forked child the reset above is still buffered, so a
        // re-read would see the PREVIOUS run's stale counters; in v5 the write
        // has committed, but the local snapshot is pinned anyway so the two
        // sides compute from identical values (and the oracle side, running
        // unforked, commits too — the pinning is behavior-identical there).
        if let Some(obj) = chat.as_object_mut() {
            obj.insert("runState".into(), Value::String("running".into()));
            obj.insert("runStartedAt".into(), Value::String(now_iso.clone()));
            obj.insert("runTurnsConsumed".into(), Value::from(0.0));
            obj.insert("runTokensConsumed".into(), Value::from(0.0));
            obj.insert("runMilestonesAnnounced".into(), Value::from(0.0));
        }

        // v4 `postRunStartAnnouncement(chatId, runId, chat)` — un-guarded (a
        // character-lookup error propagates; the addMessage write itself is
        // swallowed inside the announce helper).
        post_run_start_announcement(db, sdeps.now_ms, sdeps.mint_uuid, &chat_id, &run_id, &chat)
            .await?;
    }

    // --- 4. Pre-turn budget check ---
    // v4 `repos.chatSettings.findByUserId(userId)` is a safeQuery (errors →
    // null); reproduce the swallow.
    let uid = user_id.clone();
    let settings = db
        .read_main(move |conn| chat_settings::find_by_user_id(conn, &uid))
        .unwrap_or(None);
    let daily_token_budget: Option<i64> = settings
        .as_ref()
        .and_then(|s| s.get("autonomousRoomSettings"))
        .and_then(|a| a.get("dailyTokenBudget"))
        .and_then(Value::as_i64);
    let mut daily_tokens_spent: i64 = 0;
    if daily_token_budget.is_some() {
        let since = last_local_midnight_iso(now, sdeps.tz);
        // ⚠️ BROKEN-BUT-EXACT: `get_total_token_usage_since` always sums to 0
        // on SQLite (the `$ne: null` translator bug, empirically pinned — see
        // the repo method's docs). The daily gate therefore never binds through
        // actual spend, exactly as in v4-on-SQLite; the read still runs so the
        // composition shape (and any future fix) stays aligned.
        let uid = user_id.clone();
        let usage = db.read_llm_logs(move |conn| {
            Ok(LLMLogsRepository::new(conn).get_total_token_usage_since(&uid, &since))
        })?;
        daily_tokens_spent = usage.total_tokens as i64;
    }

    let budget = check_budget(
        &budget_state_from(&chat),
        now,
        daily_token_budget,
        daily_tokens_spent,
    );
    if let BudgetCheck::Exhausted { next_state, reason } = budget {
        let mask = chat_f64(&chat, "runMilestonesAnnounced").unwrap_or(0.0) as i64;
        match pre_turn_exhausted_action(mask) {
            ExhaustedAction::ProceedGraceTurn => {
                // This is the granted grace turn: let it run one last time even
                // though the budget is spent; the post-turn check (step 9) sees
                // the grace bit and ends the run cleanly.
            }
            ExhaustedAction::End => {
                // The company already had their near-end warning — end now.
                let next_run_iso =
                    recompute_next_run(chat_str(&chat, "scheduleCron"), now, sdeps.tz);
                let is_pause = next_state == NextRunState::Paused;
                transition_run_state(
                    db,
                    &chat_id,
                    ChatUpdate {
                        run_state: Some(next_state.as_str().to_string()),
                        run_ended_at: Some(Some(now_iso.clone())),
                        run_state_message: Some(Some(format!("budget:{}", reason.as_str()))),
                        // A daily-cap pause may be manually resumed before the
                        // cap rolls over; stamp runPausedAt so that resume can
                        // exclude the paused interval from the wall-clock budget.
                        run_paused_at: if is_pause {
                            Some(Some(now_iso.clone()))
                        } else {
                            None
                        },
                        schedule_next_run_at: next_run_iso.clone().map(Some),
                        ..Default::default()
                    },
                )
                .await?;
                let kind = if is_pause {
                    AutonomousRoomKind::Paused
                } else {
                    AutonomousRoomKind::End
                };
                let reason_text = if reason.as_str() == "tokens_user_daily" {
                    "Daily user-token budget reached. The room will resume when the budget rolls over (instance-local midnight).".to_string()
                } else {
                    format!("Budget exhausted (reason: {}).", reason.as_str())
                };
                post_autonomous_room_announcement(
                    db,
                    sdeps.now_ms,
                    sdeps.mint_uuid,
                    &chat_id,
                    kind,
                    &reason_text,
                    None,
                )
                .await;
                return Ok(StepOutcome::EndedPreTurn {
                    reason: reason.as_str().to_string(),
                    next_state: next_state.as_str().to_string(),
                });
            }
            ExhaustedAction::GrantGrace => {
                grant_grace_turn(
                    db,
                    sdeps.now_ms,
                    sdeps.mint_uuid,
                    &chat_id,
                    &user_id,
                    &run_id,
                    mask,
                )
                .await?;
                return Ok(StepOutcome::GraceGrantedPreTurn);
            }
        }
    }

    // --- 5. Speaker selection (v4 turn-manager over the ported leaves) ---
    let participants = participants_array(&chat);
    let filter_participants: Vec<_> = participants.iter().map(to_filter_participant).collect();
    let active = get_active_character_participants(&filter_participants);
    // v4 builds a Map<characterId, Character> via `repos.characters.findById`
    // per active participant (the vault-overlaid read).
    let talkativeness = load_talkativeness_map(db, &active)?;
    let cid = chat_id.clone();
    let messages = db.read_main(move |conn| chats_messages_read::get_messages(conn, &cid))?;
    // v4 filters `type === 'message'` before the turn manager (to_message_views
    // applies the same filter).
    let message_views = to_message_views(&messages);
    // Autonomous rooms have no user participant by definition
    // (`userParticipantId: null` — unused by the ported selection, as by v4's).
    let turn_state = calculate_turn_state_from_history(
        &message_views,
        chat_str(&chat, "spokenThisCycleParticipantIds"),
    );
    let speaker_participants: Vec<_> = participants.iter().map(to_speaker_participant).collect();
    let selection = select_next_speaker(
        &speaker_participants,
        &talkativeness,
        &turn_state.queue,
        &turn_state.spoken_since_user_turn,
        turn_state.last_speaker_id.as_deref(),
        sdeps.random01,
    );

    let Some(responding_participant_id) = selection.next_speaker_id.clone() else {
        transition_run_state(
            db,
            &chat_id,
            ChatUpdate {
                run_state: Some("error".to_string()),
                run_ended_at: Some(Some(now_iso.clone())),
                run_state_message: Some(Some(format!("no_eligible_speaker:{}", selection.reason))),
                ..Default::default()
            },
        )
        .await?;
        return Ok(StepOutcome::NoEligibleSpeaker {
            reason: selection.reason.to_string(),
        });
    };

    // --- 6/7. The turn: the ordinary message pipeline with the autonomous flags ---
    // v4 wraps generation + stream drain in `runWithAutonomousRunId(runId, …)`
    // so every llm_logs row written during the turn is tagged with the run's id
    // — here the explicit LogContext threaded through the input (spec decision
    // #4). `suppressAutomaticImages: true` is v4's fifth flag; it has NO
    // consumer anywhere in v4 at `6bf88959` (verified — declared in
    // SendMessageOptions, passed through the chain inheritance, never read), so
    // the ported SendMessageOptions faithfully omits it.
    let input = ProcessMessageInput {
        chat_id: chat_id.clone(),
        user_id: user_id.clone(),
        options: SendMessageOptions {
            continue_mode: true,
            content: String::new(),
            responding_participant_id: Some(responding_participant_id.clone()),
            never_pause_for_user: true,
            // One job = one character turn: the chain driver is skipped and the
            // handler re-enqueues at the end instead (v4's singleTurn rationale
            // — the forked child's chain re-read pathology — dissolves in v5,
            // but the per-turn budget check still requires one-turn jobs).
            single_turn: true,
            // Pace a token-budgeted room: cap this turn's context budget at its
            // slice of the per-run token budget (`remaining / turnsLeft`),
            // computed off the LOCAL chat snapshot (reset to 0 at run start).
            autonomous_context_cap: compute_autonomous_context_cap(&budget_state_from(&chat)),
            ..Default::default()
        },
        clock: ProcessClock {
            now_ms: now,
            local_offset_minutes: sdeps.local_offset_minutes,
            random01: sdeps.random01,
        },
        model_context_limit: sdeps.model_context_limit,
        timestamp_config: sdeps.timestamp_config.clone(),
        timezone: sdeps.timezone.clone(),
        provider_supports_web_search: sdeps.provider_supports_web_search,
        log_context: LogContext {
            autonomous_run_id: Some(run_id.clone()),
        },
    };
    // v4's `handleSendMessage` shell SWALLOWS every error thrown inside its
    // ReadableStream `start()` (`handleStreamError`: an SSE `error` frame to
    // the drained stream + close), so the turn handler's catch — the
    // `turn_error:` transition — is dead code at `6bf88959` and a mid-turn
    // failure proceeds to the post-turn bookkeeping. Reproduced: the Err is
    // dropped here (the frame belongs to the Phase-4 transport shell). With
    // `single_turn` the chain driver is a verified no-op, and v4's post-cycle
    // scene-tracking / conversation-render triggers are unported wave-4
    // subsystems (mocked no-op in the oracle), so `process_message` alone IS
    // the shell body.
    let turn_result = process_message(orch, &input).await;
    if let Err(e) = &turn_result {
        // The swallow otherwise hides e.g. a missing fixture table behind a
        // silent no-message turn. v4 logs the same swallow via
        // `handleStreamError` (error level). With P4.18 the core has a log
        // surface, so this is an unconditional `tracing::error!` — `RUST_LOG`
        // is the gate now, replacing the old `QT_STEP_DEBUG` env hook. The
        // harness/oracle installs no subscriber, so events are dropped there
        // (no NDJSON contamination).
        tracing::error!(
            target: "quilltap::enclave",
            error = ?e,
            "Enclave step turn error (swallowed, v4 shell semantics)",
        );
    }
    drop(turn_result);

    // --- 7.5 / 8. Post-turn bookkeeping ---
    // Re-read the chat to run the stale-run guard against fresh DB state; the
    // counter values deliberately come from the LOCAL `chat` snapshot, NOT the
    // re-read `post`. Origin (v4's forked child): the repo proxy buffers writes
    // and serves reads from a readonly connection, so in the idle→running
    // fallback the reset is still buffered and `post` reads the PREVIOUS run's
    // stale counters — a read-modify-write off `post` would make the counter
    // accumulate across runs forever. The local snapshot is the only reliable
    // post-reset view in every case (fresh run: the run-start contract
    // committed 0 before this job; fallback: mutated to 0 above; resumed run:
    // the genuine pre-pause count). v5 has no buffered reads, but the oracle
    // pins v4's unforked behavior — identical either way — and the pinning is
    // carried so the arithmetic provenance stays v4's.
    let cid = chat_id.clone();
    let post = db.read_main(move |conn| chats_read::find_by_id(conn, &cid))?;
    let superseded = match &post {
        None => true,
        Some(p) => chat_str(p, "currentRunId") != Some(run_id.as_str()),
    };
    if superseded {
        return Ok(StepOutcome::Superseded);
    }

    // Token accounting: sum the llm_logs rows tagged with this run's id. This
    // isolates the run's own turn spend — conversational turns plus their
    // agent-mode sub-calls — and excludes overlapping chat activity and
    // fire-and-forget auxiliary jobs. Cache-read tokens are excluded by default
    // (the provider plugins subtract them from usage.totalTokens at the
    // source); a room opts into counting every token with
    // `budgetExcludeCacheHits = 0`, adding the stripped reads back.
    let include_cache_hits = chat_f64(&chat, "budgetExcludeCacheHits").unwrap_or(1.0) == 0.0;
    let rid = run_id.clone();
    let run_usage = db.read_llm_logs(move |conn| {
        Ok(LLMLogsRepository::new(conn).get_total_token_usage_for_run(&rid, include_cache_hits))
    })?;
    // Math.max keeps the counter monotonic (v4: a transient read-zero — the
    // forked child's one-turn-behind sum — can't un-exhaust the run). The floor
    // is the LOCAL snapshot, per the long comment above.
    let new_tokens_consumed = run_usage
        .total_tokens
        .max(chat_f64(&chat, "runTokensConsumed").unwrap_or(0.0));
    // Turn accounting: increment off the LOCAL snapshot.
    let new_turns_consumed = chat_f64(&chat, "runTurnsConsumed").unwrap_or(0.0) + 1.0;

    transition_run_state(
        db,
        &chat_id,
        ChatUpdate {
            run_turns_consumed: Some(new_turns_consumed),
            run_tokens_consumed: Some(new_tokens_consumed),
            ..Default::default()
        },
    )
    .await?;

    // --- 9. End-of-run check ---
    let cid = chat_id.clone();
    let post_check = db.read_main(move |conn| chats_read::find_by_id(conn, &cid))?;
    let Some(post_check) = post_check else {
        return Ok(StepOutcome::Superseded);
    };
    if chat_str(&post_check, "currentRunId") != Some(run_id.as_str()) {
        return Ok(StepOutcome::Superseded);
    }
    let mut post_daily_spent: i64 = 0;
    if daily_token_budget.is_some() {
        let since = last_local_midnight_iso((sdeps.now_ms)(), sdeps.tz);
        let uid = user_id.clone();
        // The same BROKEN-BUT-EXACT always-zero read as the pre-turn check.
        let usage = db.read_llm_logs(move |conn| {
            Ok(LLMLogsRepository::new(conn).get_total_token_usage_since(&uid, &since))
        })?;
        post_daily_spent = usage.total_tokens as i64;
    }
    // Pass the freshly-computed counters into the check rather than
    // `postCheck.run{Turns,Tokens}Consumed` (v4: the update just queued is
    // still buffered, so the re-read sees the previous turn's value and would
    // miss exhaustion that happened this turn). Same hazard for the wall-clock
    // anchor: `runStartedAt` / `runPausedAccumMs` are pinned from the LOCAL
    // snapshot (in the fallback case `postCheck.runStartedAt` would read the
    // PREVIOUS run's start and falsely exhaust a wall-clock-budgeted room).
    let verdict_state = BudgetState {
        run_started_at_ms: chat_str(&chat, "runStartedAt").and_then(iso_to_ms),
        run_paused_accum_ms: chat_f64(&chat, "runPausedAccumMs").map(|v| v as i64),
        run_turns_consumed: Some(new_turns_consumed as i64),
        run_tokens_consumed: Some(new_tokens_consumed as i64),
        ..budget_state_from(&post_check)
    };
    let verdict = check_budget(
        &verdict_state,
        (sdeps.now_ms)(),
        daily_token_budget,
        post_daily_spent,
    );
    if let BudgetCheck::Exhausted { next_state, reason } = verdict {
        let mask = chat_f64(&chat, "runMilestonesAnnounced").unwrap_or(0.0) as i64;
        match post_turn_exhausted_action(mask) {
            ExhaustedAction::GrantGrace => {
                // Reached budget this turn without a near-end warning ever
                // firing (a single turn vaulted the [90%, 100%) band) — grant
                // one grace turn; its own post-turn check ends the run.
                grant_grace_turn(
                    db,
                    sdeps.now_ms,
                    sdeps.mint_uuid,
                    &chat_id,
                    &user_id,
                    &run_id,
                    mask,
                )
                .await?;
                return Ok(StepOutcome::GraceGrantedPostTurn);
            }
            _ => {
                // Either the grace turn just completed, or the near-end warning
                // already fired — end now. NOTE (faithful asymmetry): unlike
                // the pre-turn branch, v4 does NOT stamp `runPausedAt` on a
                // daily-cap pause here.
                let end_now = (sdeps.now_ms)();
                let next_run_iso =
                    recompute_next_run(chat_str(&post_check, "scheduleCron"), end_now, sdeps.tz);
                transition_run_state(
                    db,
                    &chat_id,
                    ChatUpdate {
                        run_state: Some(next_state.as_str().to_string()),
                        run_ended_at: Some(Some(iso_from_unix_ms(end_now))),
                        run_state_message: Some(Some(format!("budget:{}", reason.as_str()))),
                        schedule_next_run_at: next_run_iso.clone().map(Some),
                        ..Default::default()
                    },
                )
                .await?;
                let kind = if next_state == NextRunState::Paused {
                    AutonomousRoomKind::Paused
                } else {
                    AutonomousRoomKind::End
                };
                // elapsedMs anchors on POSTCHECK.runStartedAt — NOT the local
                // snapshot (faithful: v4 reads `postCheck.runStartedAt` here,
                // the one place the re-read wins; on the fallback path this
                // makes the banner's seconds read the previous run's anchor —
                // a display-only quirk carried as-is).
                let elapsed_ms = match chat_str(&post_check, "runStartedAt").and_then(iso_to_ms) {
                    Some(started) => ((sdeps.now_ms)() - started) as f64,
                    None => 0.0,
                };
                let reason_text = if reason.as_str() == "tokens_user_daily" {
                    format!(
                        "Daily user-token budget reached after {} turn(s) and {} token(s). The room will resume when the budget rolls over.",
                        js_number_string(new_turns_consumed),
                        locale_number_string(new_tokens_consumed),
                    )
                } else {
                    format!(
                        "Autonomous run ended. Reason: {}. {} turn(s), {} token(s), {}s elapsed.",
                        reason.as_str(),
                        js_number_string(new_turns_consumed),
                        locale_number_string(new_tokens_consumed),
                        js_number_string(js_math_round(elapsed_ms / 1000.0)),
                    )
                };
                post_autonomous_room_announcement(
                    db,
                    sdeps.now_ms,
                    sdeps.mint_uuid,
                    &chat_id,
                    kind,
                    &reason_text,
                    None,
                )
                .await;
                return Ok(StepOutcome::EndedPostTurn {
                    reason: reason.as_str().to_string(),
                    next_state: next_state.as_str().to_string(),
                });
            }
        }
    }

    // --- 9b. Pacing milestones (the run is continuing) ---
    // Inputs off the LOCAL snapshot — the bitmask, the caps, and the wall-clock
    // anchor — for the same buffered-write reason as the counters (v4's
    // comment); the cross-room daily cap is passed separately.
    let progress = compute_budget_progress(
        &budget_state_from(&chat),
        new_turns_consumed as i64,
        new_tokens_consumed as i64,
        (sdeps.now_ms)(),
        daily_token_budget,
        post_daily_spent,
    );
    if let Some(progress) = progress {
        let mask = chat_f64(&chat, "runMilestonesAnnounced").unwrap_or(0.0) as i64;
        if let Some(fire) = decide_milestone(progress.fraction, mask) {
            transition_run_state(
                db,
                &chat_id,
                ChatUpdate {
                    run_milestones_announced: Some(fire.next_mask as f64),
                    ..Default::default()
                },
            )
            .await?;
            if let Some(obj) = chat.as_object_mut() {
                obj.insert(
                    "runMilestonesAnnounced".into(),
                    Value::from(fire.next_mask as f64),
                );
            }
            let msg = build_milestone_message(progress.binding, fire.milestone);
            post_autonomous_room_announcement(
                db,
                sdeps.now_ms,
                sdeps.mint_uuid,
                &chat_id,
                match msg.system_kind {
                    "autonomous-room-halfway" => AutonomousRoomKind::Halfway,
                    _ => AutonomousRoomKind::NearingEnd,
                },
                &msg.content,
                Some(&msg.opaque_content),
            )
            .await;
        }
    }

    // --- 9c. Context-summary fold ---
    // Run HERE — outside the run-id scope (the fold's cheap-LLM call is
    // housekeeping, not turn spend: `sdeps.fold_executor` carries
    // `LogContext::none()`, faithful to v4's `getAutonomousRunId() === null`
    // after the scope exits) and awaited (v4's rationale — surviving the forked
    // child's write-buffer flush — dissolves in v5, but awaiting is the same
    // observable ordering). Best-effort: a fold failure must never wedge the
    // run. The finalizer inside `process_message` SKIPS the summary check for
    // autonomous chats (its `!isAutonomous` gate), so this is the room's only
    // fold site.
    let fold_gate = settings
        .as_ref()
        .and_then(|s| s.get("cheapLLMSettings"))
        .is_some_and(|v| !v.is_null());
    if fold_gate {
        let settings = settings.as_ref().expect("gate implies settings");
        let fold_result = run_summary_fold(
            db,
            orch.completion,
            orch.embedding,
            sdeps.fold_executor,
            &chat_id,
            &user_id,
            &responding_participant_id,
            &chat,
            settings,
        )
        .await;
        // v4 catch → warn + continue.
        let _ = fold_result;
    }

    // --- 10. Loop continues — enqueue the next turn ---
    enqueue_autonomous_room_turn(db, &user_id, &chat_id, &run_id).await?;
    Ok(StepOutcome::ReEnqueued)
}

/// The 9c fold body (v4 autonomous-room-turn.ts:850–876): resolve the fold
/// profile — the responding participant's connection profile ?? the default
/// profile ?? the first — and run the ported
/// [`crate::services::context_summary::check_and_generate_summary_if_needed_with_seams`]
/// with `awaitFold: true`. Errors bubble to the caller's swallow.
///
/// The fold-time episode pass runs LIVE here via
/// [`crate::services::context_summary::FoldEpisodePassSeams`] — v4's
/// `checkAndGenerateSummaryIfNeeded` → `generateContextSummary` calls the real
/// `runFoldEpisodePass` on this path too (the enclave-step oracle cans only the
/// provider, so the differential pins the fold's cheap-LLM call); the
/// cross-subsystem arms stay no-ops per the orchestrator-oracle mock set the
/// enclave oracle reuses (the same tracked deferral as the orchestrator's
/// in-loop check).
#[allow(clippy::too_many_arguments)]
async fn run_summary_fold<C: CompletionProvider + Sync, E: EmbeddingProvider + Sync>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    chat_id: &str,
    user_id: &str,
    responding_participant_id: &str,
    chat: &Value,
    settings: &Value,
) -> Result<(), DbError> {
    let uid = user_id.to_string();
    let available = db.read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid))?;

    let responding_cp_id = participants_array(chat)
        .into_iter()
        .find(|p| chat_str(p, "id") == Some(responding_participant_id))
        .and_then(|p| chat_str(&p, "connectionProfileId").map(str::to_string));

    let fold_profile = available
        .iter()
        .find(|p| responding_cp_id.is_some() && chat_str(p, "id") == responding_cp_id.as_deref())
        .or_else(|| {
            available
                .iter()
                .find(|p| p.get("isDefault").and_then(Value::as_bool) == Some(true))
        })
        .or_else(|| available.first());
    let Some(fold_profile) = fold_profile else {
        return Ok(());
    };

    let cheap_profile = crate::services::orchestrator::cheap_llm_profile_from_value(fold_profile);
    let available_profiles: Vec<_> = available
        .iter()
        .map(crate::services::orchestrator::cheap_llm_profile_from_value)
        .collect();

    // v4 passes `chatSettings.cheapLLMSettings` straight through (the stored
    // row always carries the Zod-defaulted object).
    let cheap = settings.get("cheapLLMSettings");
    let cheap_settings = crate::services::context_summary::CheapLlmSettings {
        strategy: cheap
            .and_then(|c| c.get("strategy"))
            .and_then(Value::as_str)
            .unwrap_or("AUTO")
            .to_string(),
        user_defined_profile_id: cheap
            .and_then(|c| c.get("userDefinedProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        default_cheap_profile_id: cheap
            .and_then(|c| c.get("defaultCheapProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        fallback_to_local: cheap
            .and_then(|c| c.get("fallbackToLocal"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };

    // The fold's dangerous-chat escalation reads the user's danger settings
    // (v4 resolves them INSIDE `generateContextSummary` — a fresh
    // `chatSettings.findByUserId` + `resolveDangerousContentSettings` — and
    // ONLY when the chat is actively dangerous; the ported fold takes the
    // resolved settings injected, consulted only on a dangerous chat, so a
    // non-dangerous room never reads them on either side).
    let global_danger: Option<chat_settings::DangerousContentSettings> = settings
        .get("dangerousContentSettings")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let resolved = crate::services::dangerous_content::resolver::resolve_dangerous_content_settings(
        global_danger,
        Some(chat),
    );
    let danger_settings = Some(crate::cheap_llm::DangerousContentSettings {
        mode: resolved.settings.mode.clone(),
        uncensored_text_profile_id: resolved.settings.uncensored_text_profile_id.clone(),
    });

    // The fold's cheap-LLM call runs outside the run-id scope (see the 9c call
    // site), so the seams carry the same UNtagged executor as the fold itself.
    let seams = crate::services::context_summary::FoldEpisodePassSeams {
        db,
        embedding,
        completion,
        executor,
    };
    crate::services::context_summary::check_and_generate_summary_if_needed_with_seams(
        db,
        completion,
        executor,
        chat_id,
        &cheap_profile,
        &cheap_settings,
        &available_profiles,
        user_id,
        danger_settings.as_ref(),
        // The registry-cheapest seam: None (the spine precedent — Round-3
        // Group 8; the differential's fixture providers resolve identically).
        None,
        true, // awaitFold
        &seams,
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// schedule_tick — v4 handleAutonomousRoomScheduleTick
// ---------------------------------------------------------------------------

/// The injected impurities for one [`schedule_tick`] call.
pub struct TickDeps<'a> {
    pub now_ms: NowMsFn<'a>,
    pub mint_uuid: MintUuidFn<'a>,
    /// The instance's IANA timezone for cron evaluation.
    pub tz: &'a str,
}

/// v4 `handleAutonomousRoomScheduleTick(job)` — scan the user's autonomous
/// rooms for cron-scheduled runs that are due, apply the freshness-window
/// rule, and either start a run (advancing `scheduleNextRunAt`) or skip a
/// stale slot; then the wedge-heal sweep.
pub async fn schedule_tick(
    db: &Db,
    deps: &TickDeps<'_>,
    user_id: &str,
) -> Result<TickOutcome, DbError> {
    // v4 `repos.chatSettings.findByUserId` (safeQuery → null on error).
    let uid = user_id.to_string();
    let settings = db
        .read_main(move |conn| chat_settings::find_by_user_id(conn, &uid))
        .unwrap_or(None);
    let default_freshness_ms = settings
        .as_ref()
        .and_then(|s| s.get("autonomousRoomSettings"))
        .and_then(|a| a.get("defaultFreshnessWindowMs"))
        .and_then(Value::as_f64)
        .unwrap_or(DEFAULT_FRESHNESS_WINDOW_MS);

    // Enumerate user-owned autonomous chats with a cron and a non-terminal run
    // state — the scheduler only touches idle/paused/budgetExhausted rooms
    // (stopped and running are skipped; a NULL runState passes, as v4's
    // `!== 'running' && !== 'stopped'` does for undefined).
    let uid = user_id.to_string();
    let user_chats = db.read_main(move |conn| chats_read::find_by_user_id(conn, &uid))?;
    let autonomous_chats: Vec<&Value> = user_chats
        .iter()
        .filter(|c| {
            chat_str(c, "chatType") == Some("autonomous")
                && chat_str(c, "scheduleCron").is_some_and(|s| !s.is_empty())
                && chat_str(c, "runState") != Some("running")
                && chat_str(c, "runState") != Some("stopped")
        })
        .collect();

    let now = (deps.now_ms)();
    let now_iso = iso_from_unix_ms(now);

    let mut out = TickOutcome {
        autonomous_chats: autonomous_chats.len(),
        ..Default::default()
    };

    for chat in &autonomous_chats {
        let Some(cron) = chat_str(chat, "scheduleCron") else {
            continue;
        };
        let Some(chat_id) = chat_str(chat, "id") else {
            continue;
        };
        let next_run_iso = chat_str(chat, "scheduleNextRunAt");
        let Some(next_run_iso) = next_run_iso.filter(|s| !s.is_empty()) else {
            // No next run computed yet — seed it from cron and skip this tick.
            if let Some(seeded) = next_cron_fire_from(cron, now, deps.tz) {
                let cid = chat_id.to_string();
                let patch = ChatUpdate {
                    schedule_next_run_at: Some(Some(iso_from_unix_ms(seeded))),
                    ..Default::default()
                };
                db.write(move |writers| {
                    ChatsRepository::new(writers.main().connection()).update(&cid, &patch)
                })
                .await?;
            }
            continue;
        };
        // v4 `Date.parse(nextRunIso)` — an UNPARSEABLE slot yields NaN, whose
        // comparisons are all false: `NaN > now` fails (so the slot reads as
        // due) AND `NaN > window` fails (so it reads as fresh) → v4 STARTS a
        // run off a corrupt slot. Reproduced: `None` falls through both tests.
        let next_run_ms = iso_to_ms(next_run_iso);
        if next_run_ms.is_some_and(|ms| ms > now) {
            continue; // not due yet
        }
        out.due_count += 1;
        let window = chat_f64(chat, "scheduleFreshnessWindowMs").unwrap_or(default_freshness_ms);
        let overdue_by = next_run_ms.map(|ms| (now - ms) as f64);

        if overdue_by.is_some_and(|o| o > window) {
            // Stale slot: advance past it, computed from NOW — the missed slot
            // is skipped, not caught up.
            if let Some(advanced) = next_cron_fire_from(cron, now, deps.tz) {
                let cid = chat_id.to_string();
                let patch = ChatUpdate {
                    schedule_next_run_at: Some(Some(iso_from_unix_ms(advanced))),
                    ..Default::default()
                };
                db.write(move |writers| {
                    ChatsRepository::new(writers.main().connection()).update(&cid, &patch)
                })
                .await?;
            }
            out.stale_count += 1;
            continue;
        }

        // Within the freshness window — start a new run through the shared
        // run-start core (v4's parent-ordered `startScheduledAutonomousRun`;
        // in v5 the single-writer runtime already guarantees the ordering).
        let run_id = (deps.mint_uuid)();
        let next_next = next_cron_fire_from(cron, now, deps.tz);
        let tz = deps.tz.to_string();
        let cron_seam = move |expr: &str, anchor: i64| -> Result<Option<i64>, String> {
            crate::enclave::cron::try_next_occurrence(expr, anchor, &tz)
        };
        let ldeps = LifecycleDeps {
            now_ms: deps.now_ms,
            mint_uuid: deps.mint_uuid,
            next_occurrence: &cron_seam,
        };
        let result = start_scheduled_autonomous_run(
            db,
            &ldeps,
            BeginAutonomousRunInput {
                chat_id: chat_id.to_string(),
                user_id: user_id.to_string(),
                run_id,
                now_iso: now_iso.clone(),
                schedule_last_run_at: Some(Some(now_iso.clone())),
                // v4 `nextNext ? nextNext.toISOString() : null` — an explicit
                // null clears the slot.
                schedule_next_run_at: Some(next_next.map(iso_from_unix_ms)),
                on_enqueue_failure: OnEnqueueFailure::Idle,
            },
        )
        .await?;
        match result {
            BeginAutonomousRunResult::Ok { .. } => out.enqueued_count += 1,
            BeginAutonomousRunResult::Failed { .. } => {
                // The core already rolled the row back (to 'idle');
                // scheduleNextRunAt has advanced, so the next due slot retries.
                // v4 logs + continues.
            }
        }
    }

    // Defense-in-depth: re-engage any room left `running` with nothing in flight.
    out.healed_count = heal_wedged_runs(db, deps.now_ms, user_id).await?;

    Ok(out)
}

/// v4 `healWedgedRuns(userId)` — the self-heal sweep. A healthy `running` room
/// always has a turn job PENDING or PROCESSING (each turn enqueues the next
/// before it finishes); a `running` room with NO in-flight turn, untouched
/// longer than the grace window, is wedged (the tick's start filter excludes
/// `running`, so nothing else would re-engage it). Re-enqueue a turn for the
/// LIVE run (`currentRunId` — not a fresh start; a rare double-enqueue is
/// harmless, the turn handler's stale/concurrency guards let at most one turn
/// proceed). Per-chat enqueue failures are swallowed (v4 logs + continues).
pub async fn heal_wedged_runs(
    db: &Db,
    now_ms: NowMsFn<'_>,
    user_id: &str,
) -> Result<usize, DbError> {
    let uid = user_id.to_string();
    let user_chats = db.read_main(move |conn| chats_read::find_by_user_id(conn, &uid))?;
    let running: Vec<&Value> = user_chats
        .iter()
        .filter(|c| {
            chat_str(c, "chatType") == Some("autonomous")
                && chat_str(c, "runState") == Some("running")
        })
        .collect();
    if running.is_empty() {
        return Ok(0);
    }

    let now = now_ms();
    let mut healed = 0usize;
    for chat in running {
        // v4 `if (!chat.currentRunId) continue` (JS truthiness).
        let Some(run_id) = chat_str(chat, "currentRunId").filter(|s| !s.is_empty()) else {
            continue;
        };
        let Some(chat_id) = chat_str(chat, "id") else {
            continue;
        };
        // Grace window off `updatedAt`. v4: `Number.isFinite(updatedMs) &&
        // now - updatedMs < WEDGE_GRACE_MS` — an absent `updatedAt` reads 0
        // (finite, ancient → not skipped) and an unparseable one reads NaN
        // (not finite → not skipped); both fall through to the heal.
        let updated_ms = chat_str(chat, "updatedAt").and_then(iso_to_ms).unwrap_or(0);
        if now - updated_ms < WEDGE_GRACE_MS {
            continue;
        }

        let cid = chat_id.to_string();
        let in_flight = db.read_main(move |conn| {
            BackgroundJobsRepository::new(conn).find_pending_for_chat(&cid)
        })?;
        if in_flight
            .iter()
            .any(|j| j.job_type == "AUTONOMOUS_ROOM_TURN")
        {
            continue;
        }

        // v4 try/catch: an enqueue failure is logged + swallowed per chat.
        if enqueue_autonomous_room_turn(db, user_id, chat_id, run_id)
            .await
            .is_ok()
        {
            healed += 1;
        }
    }
    Ok(healed)
}

// ---------------------------------------------------------------------------
// The runner dispatch rows (W4.8 JobHandler pattern)
// ---------------------------------------------------------------------------

/// The `AUTONOMOUS_ROOM_TURN` [`crate::services::job_runner::JobHandler`].
///
/// The turn's dependencies are the WHOLE send spine
/// ([`crate::services::orchestrator::OrchestratorDeps`] — thirteen model/seam
/// generics plus per-call mutable finalizer seams), which only the composing
/// host can construct; so unlike the small-seam handlers (avatar / refit) this
/// one is generic over a **step-runner closure** the host supplies: it receives
/// the claimed job + decoded payload and must build the deps and call
/// [`step`] once. The handler owns the decode + the outcome mapping (an `Err`
/// becomes a FAILED job, which trips the runner's
/// `reconcile_failed_autonomous_turn` hook — v4 `job-dispatcher.ts:230`).
pub struct AutonomousRoomTurnHandler<F> {
    pub run_step: F,
}

/// The boxed step future a [`AutonomousRoomTurnHandler`] closure returns.
pub type StepFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<StepOutcome, DbError>> + Send + 'a>>;

impl<F> crate::services::job_runner::JobHandler for AutonomousRoomTurnHandler<F>
where
    F: for<'a> Fn(
            &'a Db,
            &'a crate::db::background_jobs::BackgroundJob,
            AutonomousRoomTurnPayload,
        ) -> StepFuture<'a>
        + Send
        + Sync,
{
    fn handle<'a>(
        &'a self,
        db: &'a Db,
        job: &'a crate::db::background_jobs::BackgroundJob,
    ) -> crate::services::job_runner::JobFuture<'a> {
        Box::pin(async move {
            let payload_json: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let payload = AutonomousRoomTurnPayload::from_json(&payload_json);
            match (self.run_step)(db, job, payload).await {
                Ok(_) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e.to_string()),
            }
        })
    }
}

/// The `AUTONOMOUS_ROOM_SCHEDULE_TICK` [`crate::services::job_runner::JobHandler`]
/// — fully concrete (the tick needs no model seams): the production system
/// clock/UUID over the host's configured timezone. v4's tick job payload is
/// `{}`; only `job.userId` is read.
pub struct AutonomousRoomScheduleTickHandler {
    /// The instance's IANA timezone (cron evaluation; v4 uses the process zone).
    pub tz: String,
}

impl crate::services::job_runner::JobHandler for AutonomousRoomScheduleTickHandler {
    fn handle<'a>(
        &'a self,
        db: &'a Db,
        job: &'a crate::db::background_jobs::BackgroundJob,
    ) -> crate::services::job_runner::JobFuture<'a> {
        Box::pin(async move {
            let now_ms = crate::enclave::announce::system_now_ms;
            let mint = crate::enclave::announce::system_mint_uuid;
            let deps = TickDeps {
                now_ms: &now_ms,
                mint_uuid: &mint,
                tz: &self.tz,
            };
            match schedule_tick(db, &deps, &job.user_id).await {
                Ok(_) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e.to_string()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::runtime::{Db, DbPaths};
    use crate::db::Writer;
    use crate::model::completion::CannedCompletionProvider;
    use crate::model::embedding::CannedEmbeddingProvider;
    use crate::model::stream::CannedStreamingProvider;
    use crate::services::build_context::NoopSeams;
    use crate::services::chat_events::RecordingSink;
    use crate::services::job_runner::{HandlerRegistry, JobRunner};
    use crate::services::message_finalizer::{
        NoAnswerConfirmation, NoAsyncCompression, NoCostTracking,
    };
    use crate::services::orchestrator::NoopOrchestratorSeams;
    use crate::services::pricing_fetcher::PricingFetcher;
    use crate::services::primary_stream::EffectiveProfile;
    use crate::services::provider_failover::{DangerSettings, RouteResult};
    use crate::tools::ask_carina::ErasedAskCarina;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// The full `chats` column set (loose types — these self-tests exercise the
    /// runner-loop mechanics; the tier-3 differential runs on the real generated
    /// schema). Mirrors the lifecycle self-test DDL.
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
            "timelineMode TEXT",
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
            "turnSkippingEnabled INTEGER",
        ];
        format!("CREATE TABLE chats ({});", cols.join(", "))
    }

    /// Every column `chats_messages{,_read}` reads or writes (from the canonical
    /// SELECT list + `chatId`).
    const CHAT_MESSAGES_DDL: &str = "CREATE TABLE chat_messages (\
        id TEXT PRIMARY KEY, chatId TEXT, type TEXT, role TEXT, content TEXT, \
        rawResponse TEXT, tokenCount REAL, promptTokens REAL, completionTokens REAL, \
        swipeGroupId TEXT, swipeIndex REAL, attachments TEXT DEFAULT '[]', \
        debugMemoryLogs TEXT, thoughtSignature TEXT, reasoningContent TEXT, \
        reasoningSegments TEXT, participantId TEXT, recoveryType TEXT, renderedHtml TEXT, \
        dangerFlags TEXT, targetParticipantIds TEXT, isSilentMessage TEXT, systemSender TEXT, \
        systemKind TEXT, opaqueContent TEXT, hostEvent TEXT, customAnnouncer TEXT, \
        carinaMeta TEXT, pascalMeta TEXT, pendingExternalPrompt TEXT, pendingExternalPromptFull TEXT, \
        pendingExternalAttachments TEXT, summaryAnchor TEXT, context TEXT, \
        systemEventType TEXT, description TEXT, totalTokens REAL, provider TEXT, \
        modelName TEXT, estimatedCostUSD REAL, createdAt TEXT, confirmed INTEGER, \
        confirmationChecked INTEGER, confirmationRevised INTEGER, confirmationNotes TEXT, \
        confirmationOriginalContent TEXT);";

    const BACKGROUND_JOBS_DDL: &str = "CREATE TABLE background_jobs (\
        id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, status TEXT NOT NULL, \
        payload TEXT NOT NULL, priority REAL NOT NULL, attempts REAL NOT NULL, \
        maxAttempts REAL NOT NULL, lastError TEXT, scheduledAt TEXT NOT NULL, \
        startedAt TEXT, completedAt TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

    /// The slim `characters` column set `characters_read` selects (left EMPTY —
    /// name lookups resolve to None, banking v4's `c?.name` skip).
    const CHARACTERS_DDL: &str = "CREATE TABLE characters (\
        id TEXT PRIMARY KEY, userId TEXT, name TEXT, defaultImageId TEXT, \
        defaultConnectionProfileId TEXT, defaultPartnerId TEXT, defaultRoleplayTemplateId TEXT, \
        defaultImageProfileId TEXT, sillyTavernData TEXT, isFavorite INTEGER, npc INTEGER, \
        controlledBy TEXT, defaultAgentModeEnabled INTEGER, defaultHelpToolsEnabled INTEGER, \
        defaultTimestampConfig TEXT, defaultScenarioId TEXT, defaultSystemPromptId TEXT, \
        characterDocumentMountPointId TEXT, canDressThemselves INTEGER, canCreateOutfits INTEGER, \
        systemTransparency INTEGER, coreWhisperEnabled INTEGER, canBeCarina INTEGER, \
        partnerLinks TEXT, tags TEXT, avatarOverrides TEXT, createdAt TEXT, updatedAt TEXT);";

    const LLM_LOGS_DDL: &str = "CREATE TABLE llm_logs (\
        id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, chatId TEXT, \
        characterId TEXT, autonomousRunId TEXT, provider TEXT, modelName TEXT, \
        request TEXT, response TEXT, usage TEXT, cacheUsage TEXT, rawProviderUsage TEXT, \
        requestHashes TEXT, durationMs REAL, createdAt TEXT, updatedAt TEXT);";

    /// A three-partition in-memory-style Db (temp files): main with the turn's
    /// tables, an EMPTY mount-index (the vaultless character reads never touch
    /// it), and an llm-logs partition (the token-accounting substrate).
    fn make_instance() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.db");
        let mount = dir.path().join("mount.db");
        let ll = dir.path().join("llm-logs.db");
        {
            let w = Writer::open_writable(&main, PEPPER).unwrap();
            w.connection().execute_batch(&chats_ddl()).unwrap();
            w.connection().execute_batch(CHAT_MESSAGES_DDL).unwrap();
            w.connection().execute_batch(BACKGROUND_JOBS_DDL).unwrap();
            w.connection().execute_batch(CHARACTERS_DDL).unwrap();
        }
        {
            let _w = Writer::open_writable(&mount, PEPPER).unwrap();
        }
        {
            let w = Writer::open_writable(&ll, PEPPER).unwrap();
            w.connection().execute_batch(LLM_LOGS_DDL).unwrap();
        }
        let db = Db::open(
            DbPaths {
                main,
                mount_index: Some(mount),
                llm_logs: Some(ll),
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    async fn seed_chat(db: &Db, columns: &[(&'static str, Value)]) {
        let mut names = vec![
            "id",
            "userId",
            "participants",
            "title",
            "chatType",
            "turnQueue",
            "spokenThisCycleParticipantIds",
            "createdAt",
            "updatedAt",
        ];
        let mut vals: Vec<Value> = vec![
            json!("room-1"),
            json!("u1"),
            json!([{
                "id": "p-1", "type": "CHARACTER", "characterId": "c-1",
                "controlledBy": "llm", "status": "active",
                "createdAt": "2020-01-01T00:00:00.000Z",
                "updatedAt": "2020-01-01T00:00:00.000Z",
            }]),
            json!("Room"),
            json!("autonomous"),
            json!("[]"),
            json!("[]"),
            json!("2020-01-01T00:00:00.000Z"),
            json!("2020-01-01T00:00:00.000Z"),
        ];
        for (k, v) in columns {
            names.push(k);
            vals.push(v.clone());
        }
        let placeholders: Vec<String> = (1..=vals.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "INSERT INTO chats ({}) VALUES ({})",
            names.join(", "),
            placeholders.join(", ")
        );
        db.write(move |ws| {
            let params: Vec<Box<dyn rusqlite::types::ToSql>> = vals
                .iter()
                .map(|v| -> Box<dyn rusqlite::types::ToSql> {
                    match v {
                        Value::String(s) => Box::new(s.clone()),
                        Value::Number(n) => Box::new(n.as_f64().unwrap()),
                        other => Box::new(other.to_string()),
                    }
                })
                .collect();
            let refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|b| b.as_ref()).collect();
            ws.main().connection().execute(&sql, refs.as_slice())?;
            Ok::<(), DbError>(())
        })
        .await
        .unwrap();
    }

    /// A never-reroute router (the E2E turn fails before any danger resolution).
    struct NoRouter;
    impl DangerousContentRouter for NoRouter {
        fn resolve(
            &self,
            p: &EffectiveProfile,
            k: &str,
            _s: &DangerSettings,
            _u: &str,
        ) -> impl std::future::Future<Output = RouteResult> + Send {
            let profile = p.clone();
            let key = k.to_string();
            async move {
                RouteResult {
                    rerouted: false,
                    connection_profile: profile,
                    api_key: key,
                }
            }
        }
    }

    struct NoCarina;
    impl RunCarinaQuery for NoCarina {
        #[allow(clippy::manual_async_fn)]
        fn run(
            &mut self,
            _o: crate::services::carina_runner::RunCarinaQueryOptions,
        ) -> impl std::future::Future<
            Output = Result<
                crate::services::carina_runner::CarinaResult,
                crate::services::carina_runner::CarinaRunError,
            >,
        > + Send {
            async {
                Err(crate::services::carina_runner::CarinaRunError(
                    "no carina".into(),
                ))
            }
        }
    }

    struct NoPricingFetch;
    impl PricingFetch for NoPricingFetch {
        fn openrouter_public_models(&self) -> Option<Value> {
            None
        }
        fn openrouter_sdk_models(&self, _api_key: &str) -> Option<Value> {
            None
        }
        fn ollama_tags(&self, _base_url: &str) -> Option<Value> {
            None
        }
    }

    /// The step-runner closure the E2E turn handler registers: canned providers +
    /// no-op seams around the REAL [`step`]. The sparse fixture has no
    /// `connection_profiles` table, so `process_message` fails at participant
    /// resolution — which the step SWALLOWS (v4's `handleSendMessage` shell
    /// catch), exercising the turn-counting loop without a model boundary (the
    /// full model composition is the tier-3 capstone differential's).
    ///
    /// NOTE the Send bridge: `step`'s future is non-`Send` (the finalizer's
    /// carina-markup callbacks are a plain `&mut dyn` trait object deep in the
    /// spine), while [`crate::services::job_runner::JobFuture`] requires `Send`.
    /// The closure therefore runs the step on its own dedicated thread +
    /// current-thread runtime (the differential-harness `block_on` pattern) —
    /// the composition shape a Phase-4 host reproduces.
    fn e2e_step_runner() -> impl for<'a> Fn(
        &'a Db,
        &'a crate::db::background_jobs::BackgroundJob,
        AutonomousRoomTurnPayload,
    ) -> StepFuture<'a>
           + Send
           + Sync {
        |db: &Db, job: &crate::db::background_jobs::BackgroundJob, payload| {
            let db = db.clone();
            let job_id = job.id.clone();
            let job_created_at = job.created_at.clone();
            let user_id = job.user_id.clone();
            Box::pin(async move {
                let handle = std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("inner runtime");
                    rt.block_on(run_one_step(
                        &db,
                        &job_id,
                        &job_created_at,
                        &user_id,
                        &payload,
                    ))
                });
                match handle.join() {
                    Ok(r) => r,
                    Err(_) => Err(DbError::Key("step thread panicked".into())),
                }
            })
        }
    }

    /// The non-`Send` inner body the E2E step thread block_ons.
    async fn run_one_step(
        db: &Db,
        job_id: &str,
        job_created_at: &str,
        user_id: &str,
        payload: &AutonomousRoomTurnPayload,
    ) -> Result<StepOutcome, DbError> {
        {
            let sink = RecordingSink::new();
            let embedding = CannedEmbeddingProvider::new();
            let completion = CannedCompletionProvider::new();
            let streaming = CannedStreamingProvider::new();
            let executor = CheapLlmTaskExecutor::new();
            let fold_executor = CheapLlmTaskExecutor::new();
            let bc = NoopSeams;
            let orc = NoopOrchestratorSeams;
            let router = NoRouter;
            let pricing = PricingFetcher::new(NoPricingFetch);
            let ask_carina = ErasedAskCarina::not_available();
            let mut confirmation = NoAnswerConfirmation;
            let mut compression = NoAsyncCompression;
            let mut cost = NoCostTracking;
            let mut carina = NoCarina;
            let prospero_fn: fn(
                crate::services::carina_runner::ProsperoCarinaErrorArgs,
            )
                -> Result<(), crate::services::carina_runner::CarinaRunError> = |_a| Ok(());
            let mut prospero = crate::services::carina_runner::ClosureProspero(prospero_fn);
            let mut rng_bytes = crate::tools::rng::FixedBytes::new(vec![]);
            let mut orch = OrchestratorDeps {
                db,
                embedding: &embedding,
                completion: &completion,
                streaming: &streaming,
                executor: &executor,
                ask_carina: &ask_carina,
                sink: &sink,
                pricing: &pricing,
                build_context_seams: &bc,
                orchestrator_seams: &orc,
                file_bytes: &crate::services::chat_files::NotConfiguredBytes,
                image_transcoder: &crate::files::image_processing::NotConfiguredTranscoder,
                danger_router: &router,
                confirmation: &mut confirmation,
                compression: &mut compression,
                cost: &mut cost,
                carina_query: &mut carina,
                prospero: &mut prospero,
                rng_bytes: &mut rng_bytes,
            };
            let now_ms = crate::enclave::announce::system_now_ms;
            let mint = crate::enclave::announce::system_mint_uuid;
            let sdeps = StepDeps {
                now_ms: &now_ms,
                mint_uuid: &mint,
                tz: "UTC",
                random01: 0.0,
                fold_executor: &fold_executor,
                model_context_limit: 200_000,
                timestamp_config: None,
                timezone: Some("UTC".to_string()),
                local_offset_minutes: 0,
                provider_supports_web_search: false,
            };
            let meta = TurnJobMeta {
                job_id,
                job_created_at,
                user_id,
            };
            step(&mut orch, &sdeps, &meta, payload).await
        }
    }

    fn chat_row(db: &Db) -> Value {
        db.read_main(|conn| chats_read::find_by_id(conn, "room-1"))
            .unwrap()
            .unwrap()
    }

    fn announcement_kinds(db: &Db) -> Vec<String> {
        db.read_main(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT systemKind FROM chat_messages WHERE chatId = 'room-1' \
                     ORDER BY createdAt ASC, systemKind ASC",
                )
                .unwrap();
            let rows = stmt
                .query_map([], |r| r.get::<_, Option<String>>(0))
                .unwrap()
                .filter_map(|r| r.unwrap())
                .collect::<Vec<_>>();
            Ok(rows)
        })
        .unwrap()
    }

    /// End-to-end through the REAL runner: enqueue a schedule tick → the tick
    /// handler starts the run (fresh cron slot) → the turn handler drives the
    /// REAL `step()` per claimed job, each re-enqueueing the next — halfway
    /// milestone at turn 1/2, budget reached at turn 2 → grace turn → budget
    /// end at turn 3 (`budget:turns`), with the four Host announcements posted.
    #[tokio::test]
    async fn runner_e2e_tick_to_budget_end() {
        let (_dir, db) = make_instance();
        let now = crate::clock::now_unix_ms();
        seed_chat(
            &db,
            &[
                ("scheduleCron", json!("* * * * *")),
                // Due (1 s ago) and well inside the 12 h default freshness window.
                ("scheduleNextRunAt", json!(iso_from_unix_ms(now - 1000))),
                ("budgetMaxTurns", json!(2.0)),
                ("runState", json!("idle")),
            ],
        )
        .await;

        let mut reg = HandlerRegistry::new();
        reg.register(
            "AUTONOMOUS_ROOM_SCHEDULE_TICK",
            Box::new(AutonomousRoomScheduleTickHandler {
                tz: "UTC".to_string(),
            }),
        );
        reg.register(
            "AUTONOMOUS_ROOM_TURN",
            Box::new(AutonomousRoomTurnHandler {
                run_step: e2e_step_runner(),
            }),
        );
        let runner = JobRunner::new(db.clone(), reg);

        crate::services::queue_service::enqueue_autonomous_room_schedule_tick(&db, "u1")
            .await
            .unwrap();

        // Pump until the queue drains (each turn re-enqueues the next; the run
        // ends itself at the budget).
        for _ in 0..10 {
            let out = runner.pump_claim().await;
            if out.dispatched == 0 {
                break;
            }
        }

        let chat = chat_row(&db);
        assert_eq!(chat["runState"], "budgetExhausted");
        assert_eq!(chat["runStateMessage"], "budget:turns");
        assert_eq!(chat["runTurnsConsumed"], 3.0);
        // halfway (1) + grace (4) — near-end never fired (2/2 vaulted the band).
        assert_eq!(chat["runMilestonesAnnounced"], 5.0);
        assert!(chat.get("runEndedAt").is_some());
        // The end transition advanced the cron slot.
        assert!(chat.get("scheduleNextRunAt").is_some());

        // The four Host announcements, in order.
        assert_eq!(
            announcement_kinds(&db),
            vec![
                "autonomous-room-start",
                "autonomous-room-halfway",
                "autonomous-room-grace",
                "autonomous-room-end",
            ]
        );

        // Every job COMPLETED: 1 tick + 3 turns (the end path enqueues nothing).
        let (total, completed): (i64, i64) = db
            .read_main(|c| {
                Ok((
                    c.query_row("SELECT COUNT(*) FROM background_jobs", [], |r| r.get(0))?,
                    c.query_row(
                        "SELECT COUNT(*) FROM background_jobs WHERE status = 'COMPLETED'",
                        [],
                        |r| r.get(0),
                    )?,
                ))
            })
            .unwrap();
        assert_eq!(total, 4);
        assert_eq!(completed, 4);
    }

    /// The wedge-heal + failure-reconcile wiring through the runner: a `running`
    /// room with no in-flight turn is re-enqueued by the tick's heal sweep with
    /// the LIVE run id; with no turn handler registered the claimed turn hits
    /// the loud fallback (FAILED), and the runner's post-failure hook runs
    /// `reconcile_failed_autonomous_turn` — the room flips to a resumable
    /// `paused` with `turn_failed:<lastError>` and a bumped `currentRunId`.
    #[tokio::test]
    async fn runner_e2e_wedge_heal_and_failure_reconcile() {
        let (_dir, db) = make_instance();
        seed_chat(
            &db,
            &[
                ("runState", json!("running")),
                ("currentRunId", json!("run-wedged")),
                // updatedAt far in the past (seeded 2020) → outside the 60 s grace.
            ],
        )
        .await;

        let mut reg = HandlerRegistry::new();
        reg.register(
            "AUTONOMOUS_ROOM_SCHEDULE_TICK",
            Box::new(AutonomousRoomScheduleTickHandler {
                tz: "UTC".to_string(),
            }),
        );
        // Deliberately NO turn handler — the healed turn hits the loud fallback.
        let runner = JobRunner::new(db.clone(), reg);

        crate::services::queue_service::enqueue_autonomous_room_schedule_tick(&db, "u1")
            .await
            .unwrap();
        for _ in 0..5 {
            let out = runner.pump_claim().await;
            if out.dispatched == 0 {
                break;
            }
        }

        // The healed turn carried the LIVE run id.
        let payload: String = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM background_jobs WHERE type = 'AUTONOMOUS_ROOM_TURN'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&payload).unwrap()["runId"],
            "run-wedged"
        );

        // The loud-fallback failure tripped the reconcile: resumable paused,
        // `turn_failed:` cause, currentRunId bumped past the wedged run.
        let chat = chat_row(&db);
        assert_eq!(chat["runState"], "paused");
        let msg = chat["runStateMessage"].as_str().unwrap();
        assert!(
            msg.starts_with("turn_failed:Job type \"AUTONOMOUS_ROOM_TURN\" is recognized"),
            "{msg}"
        );
        assert_ne!(chat["currentRunId"], "run-wedged");
        // Counters preserved (the reconcile pauses resumably).
        assert!(chat.get("runEndedAt").is_none());
    }

    #[test]
    fn local_midnight_is_tz_aware() {
        // 2026-07-08T16:30:00Z.
        let now = iso_to_ms("2026-07-08T16:30:00.000Z").unwrap();
        assert_eq!(
            last_local_midnight_iso(now, "UTC"),
            "2026-07-08T00:00:00.000Z"
        );
        // In New York (UTC-4 in July), 16:30Z is 12:30 local → local midnight
        // is 04:00Z.
        assert_eq!(
            last_local_midnight_iso(now, "America/New_York"),
            "2026-07-08T04:00:00.000Z"
        );
        // In Tokyo (UTC+9), 16:30Z is 01:30 next day → local midnight 15:00Z.
        assert_eq!(
            last_local_midnight_iso(now, "Asia/Tokyo"),
            "2026-07-08T15:00:00.000Z"
        );
    }
}
