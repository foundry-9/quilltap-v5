//! Port of the pure enclave budget arithmetic from v4's
//! lib/background-jobs/handlers/autonomous-room-turn.ts — the two
//! side-effect-free decisions that govern an autonomous ("enclave") run:
//!
//!   * [`check_budget`] — the pre-turn exhaustion verdict over the four caps:
//!     wall-clock, per-room turns, per-room tokens, and the cross-room daily
//!     user-token cap. First cap to bind wins; the daily cap *pauses* the room
//!     (it resumes when the cap rolls over) while the others *end* the run.
//!   * [`compute_budget_progress`] — how far the run has progressed toward its
//!     *binding* cap (the one closest to exhaustion). Drives the Host's pacing
//!     nudges (halfway / near-end).
//!
//! The stateful orchestration that wraps these (run-state transitions, the
//! milestone bitmask, grace-turn granting, the daily-spend llm_logs read) is
//! Phase-3 state-machine territory, not tier-1 material. `lastLocalMidnightIso`
//! is deliberately *not* ported here: it resolves the daily-cap rollover at the
//! instance's local-time midnight (timezone + DST dependent), which is not pure
//! ms-arithmetic and doesn't belong in an exact-equivalence test.
//!
//! Time handling: v4 calls `Date.parse(chat.runStartedAt)` inside `checkBudget`.
//! Here the ISO→ms parse happens at the call boundary — the caller passes
//! `run_started_at_ms` (and `now_ms`) already resolved, the same seam the
//! differential harness uses (its `iso_to_ms` bridge, self-tested against the
//! fixed clock). `None` for `run_started_at_ms` mirrors a null/empty
//! `runStartedAt`, which disables the wall-clock cap.

/// The per-run budget caps and run-state counters read off the chat row.
///
/// `run_started_at_ms` is v4's `Date.parse(chat.runStartedAt)`; `None` mirrors a
/// null/empty `runStartedAt` (wall-clock cap disabled). The `run_*_consumed`
/// fields are read by [`check_budget`]; [`compute_budget_progress`] takes the
/// consumed counts as explicit arguments instead (matching v4, where they are
/// pinned to the in-flight buffered values rather than re-read off the row) and
/// ignores these two fields.
#[derive(Clone, Debug, Default)]
pub struct BudgetState {
    pub budget_max_turns: Option<i64>,
    pub budget_max_tokens: Option<i64>,
    pub budget_max_wall_clock_ms: Option<i64>,
    pub run_started_at_ms: Option<i64>,
    pub run_paused_accum_ms: Option<i64>,
    pub run_turns_consumed: Option<i64>,
    pub run_tokens_consumed: Option<i64>,
}

/// Why a run is over budget. The string forms match v4's `reason` literals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetReason {
    WallClock,
    Turns,
    TokensRoom,
    TokensUserDaily,
}

impl BudgetReason {
    pub fn as_str(self) -> &'static str {
        match self {
            BudgetReason::WallClock => "wall_clock",
            BudgetReason::Turns => "turns",
            BudgetReason::TokensRoom => "tokens_room",
            BudgetReason::TokensUserDaily => "tokens_user_daily",
        }
    }
}

/// The run-state a budget-exhausted verdict transitions into. The daily cap
/// *pauses* (resumes after rollover); every other cap *ends* the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextRunState {
    BudgetExhausted,
    Paused,
}

impl NextRunState {
    pub fn as_str(self) -> &'static str {
        match self {
            NextRunState::BudgetExhausted => "budgetExhausted",
            NextRunState::Paused => "paused",
        }
    }
}

/// The pre-turn budget verdict: either the run may proceed, or a cap has bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetCheck {
    Ok,
    Exhausted {
        next_state: NextRunState,
        reason: BudgetReason,
    },
}

/// Pre-turn budget verdict. Reads the per-row chat caps directly; for the daily
/// user-token cap the caller passes in the usage summed since instance-local
/// midnight (read off llm_logs in v4).
///
/// The four caps are checked in a fixed order — wall-clock, room-turns,
/// room-tokens, daily — and the *first* to bind wins. Note the asymmetry with
/// [`compute_budget_progress`]: this verdict has **no `> 0` guard** on the caps,
/// so a cap of `0` binds immediately (`consumed >= 0` is always true). A `None`
/// consumed count is treated as `0`.
pub fn check_budget(
    caps: &BudgetState,
    now_ms: i64,
    daily_token_budget: Option<i64>,
    daily_tokens_spent: i64,
) -> BudgetCheck {
    if let (Some(max_wall), Some(started)) = (caps.budget_max_wall_clock_ms, caps.run_started_at_ms)
    {
        // Exclude time spent paused: run_started_at stays fixed (it anchors
        // token accounting), so subtract the accumulated paused duration here.
        let elapsed = now_ms - started - caps.run_paused_accum_ms.unwrap_or(0);
        if elapsed >= max_wall {
            return BudgetCheck::Exhausted {
                next_state: NextRunState::BudgetExhausted,
                reason: BudgetReason::WallClock,
            };
        }
    }
    if let Some(max_turns) = caps.budget_max_turns {
        if caps.run_turns_consumed.unwrap_or(0) >= max_turns {
            return BudgetCheck::Exhausted {
                next_state: NextRunState::BudgetExhausted,
                reason: BudgetReason::Turns,
            };
        }
    }
    if let Some(max_tokens) = caps.budget_max_tokens {
        if caps.run_tokens_consumed.unwrap_or(0) >= max_tokens {
            return BudgetCheck::Exhausted {
                next_state: NextRunState::BudgetExhausted,
                reason: BudgetReason::TokensRoom,
            };
        }
    }
    // Daily user-token cap: transitions to 'paused' (not 'budgetExhausted')
    // because the room resumes tomorrow when the scheduler re-evaluates.
    if let Some(budget) = daily_token_budget {
        if daily_tokens_spent >= budget {
            return BudgetCheck::Exhausted {
                next_state: NextRunState::Paused,
                reason: BudgetReason::TokensUserDaily,
            };
        }
    }
    BudgetCheck::Ok
}

/// Default number of turns a token-budgeted autonomous run should aim to span
/// when the room sets no explicit `budget_max_turns`. Slices the per-run token
/// budget into a per-turn context cap so a run paces itself across several turns
/// instead of spending most of the budget on one oversized turn. (v4's
/// `DEFAULT_AUTONOMOUS_TARGET_TURNS`.)
pub const DEFAULT_AUTONOMOUS_TARGET_TURNS: i64 = 6;

/// Floor for the per-turn context cap (tokens). Below this a turn can't carry a
/// functioning context (system prompt + character cards + scene state + a little
/// history), so the cap never clamps below it — the run's own budget check ends
/// the run rather than ship a starved turn. (v4's `MIN_AUTONOMOUS_CONTEXT_TOKENS`.)
pub const MIN_AUTONOMOUS_CONTEXT_TOKENS: i64 = 16_000;

/// Derive this turn's context-budget cap (tokens) from the room's per-run token
/// budget, sliced across the turns the run should still span. The context
/// manager clamps its model-derived `maxAvailable` down to this value, so a room
/// running on a big-context model still paces its per-run token budget across
/// turns. Ports v4's `computeAutonomousContextCap`.
///
/// Returns `None` when the room has no token budget (`budget_max_tokens` is
/// `None`) — leaving the model-derived context budget untouched.
///
/// The slice is `remaining / turns_left`:
///   * `remaining` = `budget_max_tokens − run_tokens_consumed`, floored at 0.
///   * `turns_left` = `budget_max_turns − run_turns_consumed` (floored at 1) when
///     a turn budget is also set, else [`DEFAULT_AUTONOMOUS_TARGET_TURNS`].
///
/// Floored at [`MIN_AUTONOMOUS_CONTEXT_TOKENS`]. A `None` consumed count is `0`.
///
/// The division reproduces v4's `Math.floor(remaining / turns_left)` on f64 (both
/// operands are exact as f64 in the budget range), matching V8 rather than i64
/// truncation — identical for these non-negative operands, faithful by construction.
pub fn compute_autonomous_context_cap(caps: &BudgetState) -> Option<i64> {
    let max_tokens = caps.budget_max_tokens?;
    let remaining = (max_tokens - caps.run_tokens_consumed.unwrap_or(0)).max(0);
    let turns_left = match caps.budget_max_turns {
        Some(max_turns) => (max_turns - caps.run_turns_consumed.unwrap_or(0)).max(1),
        None => DEFAULT_AUTONOMOUS_TARGET_TURNS,
    };
    let sliced = (remaining as f64 / turns_left as f64).floor() as i64;
    Some(sliced.max(MIN_AUTONOMOUS_CONTEXT_TOKENS))
}

/// Which cap is binding (closest to exhaustion). The string forms match v4's
/// `MilestoneBinding` literals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MilestoneBinding {
    Time,
    Turns,
    Tokens,
    Daily,
}

impl MilestoneBinding {
    pub fn as_str(self) -> &'static str {
        match self {
            MilestoneBinding::Time => "time",
            MilestoneBinding::Turns => "turns",
            MilestoneBinding::Tokens => "tokens",
            MilestoneBinding::Daily => "daily",
        }
    }
}

/// How far the run has progressed toward its binding cap, and which cap that is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BudgetProgress {
    pub fraction: f64,
    pub binding: MilestoneBinding,
}

/// How far the current run has progressed toward its *binding* budget — the cap
/// closest to exhaustion, which will halt the run first. Considers the three
/// per-run room caps (turns / tokens / wall-clock) and the cross-room daily
/// user-token cap. Returns `None` when none is configured.
///
/// A candidate is skipped when its fraction is non-finite or negative (e.g. a
/// `run_started_at_ms` in the future yields negative elapsed). Unlike
/// [`check_budget`], each cap carries a `> 0` guard (a zero or negative cap is
/// skipped — there is nothing to count down toward).
///
/// On a tie the order of consideration decides: turns, then tokens, then time,
/// then daily — the replacement test is strictly-greater, so the *first*
/// considered wins. The per-run caps are considered before the daily cap, so an
/// "ending" nudge is preferred over a "pausing" one when both are equally close.
pub fn compute_budget_progress(
    caps: &BudgetState,
    turns_consumed: i64,
    tokens_consumed: i64,
    now_ms: i64,
    daily_budget: Option<i64>,
    daily_spent: i64,
) -> Option<BudgetProgress> {
    let mut best: Option<BudgetProgress> = None;
    let mut consider = |fraction: f64, binding: MilestoneBinding| {
        if !fraction.is_finite() || fraction < 0.0 {
            return;
        }
        // `!best || fraction > best.fraction` — strictly greater, so a tie keeps
        // the earlier-considered binding.
        match best {
            Some(b) if fraction <= b.fraction => {}
            _ => best = Some(BudgetProgress { fraction, binding }),
        }
    };

    if let Some(max) = caps.budget_max_turns {
        if max > 0 {
            consider(turns_consumed as f64 / max as f64, MilestoneBinding::Turns);
        }
    }
    if let Some(max) = caps.budget_max_tokens {
        if max > 0 {
            consider(
                tokens_consumed as f64 / max as f64,
                MilestoneBinding::Tokens,
            );
        }
    }
    if let (Some(max), Some(started)) = (caps.budget_max_wall_clock_ms, caps.run_started_at_ms) {
        if max > 0 {
            let elapsed = now_ms - started - caps.run_paused_accum_ms.unwrap_or(0);
            consider(elapsed as f64 / max as f64, MilestoneBinding::Time);
        }
    }
    if let Some(budget) = daily_budget {
        if budget > 0 {
            consider(daily_spent as f64 / budget as f64, MilestoneBinding::Daily);
        }
    }

    best
}
