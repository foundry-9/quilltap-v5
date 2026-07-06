//! The pure scheduler decision leaves the host driver consults (v4's four
//! `scheduled-*.ts` timers, reduced to their portable pure cores).
//!
//! v4's schedulers are `setInterval` timers that assume an always-on host; per
//! the enclave `step()` philosophy (`docs/developer/porting/api-boundary.md`
//! Part 3), the native core owns **no timers**. Instead it exposes the *decisions*
//! a per-host driver (Tauri / CLI / a tokio interval in tests) makes on each tick:
//!
//!   - [`clamp_wake_delay`] — the claim loop's next-`scheduledAt` wake delay,
//!     v4 `armWakeTimer`'s `Math.min(Math.max(rawMs, 100), MAX_WAKE_DELAY_MS)`.
//!   - [`should_run_startup_tick`] — the daily maintenance / housekeeping startup
//!     short-circuit, v4's "skip the initial tick if a successful sweep ran within
//!     the recent-run window" (`RECENT_RUN_WINDOW_MS = 20 h`), lifted out of the
//!     `setTimeout`/`setInterval` shell so the host decides when to call it.
//!
//! The per-kind cadence *values* (24 h daily interval, 5-minute stuck-reset
//! cadence, 5-minute startup grace, the 10-minute danger scan) are exported as
//! constants for the host driver — the core never sleeps on them.

/// v4 `MAX_WAKE_DELAY_MS` (`job-dispatcher.ts`): the claim loop's wake timer is
/// clamped to at most 5 minutes so a far-future `scheduledAt` still gets a
/// periodic re-check.
pub const MAX_WAKE_DELAY_MS: i64 = 5 * 60 * 1000;

/// v4's claim loop poll cadence (`POLL_INTERVAL_MS`, 2 s). The host driver ticks
/// [`super::job_runner::JobRunner::pump_claim`] this often; the core does not
/// sleep.
pub const POLL_INTERVAL_MS: i64 = 2_000;

/// v4's stuck-job sweep cadence (`STUCK_JOB_CHECK_INTERVAL_MS`, 5 min).
pub const STUCK_JOB_CHECK_INTERVAL_MS: i64 = 5 * 60 * 1000;

/// v4's daily maintenance / housekeeping / cleanup interval (24 h).
pub const DAILY_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

/// v4's daily-tick startup grace (`STARTUP_GRACE_MS`, 5 min) — the host defers
/// the first daily tick this long after boot so the UI finishes loading.
pub const STARTUP_GRACE_MS: i64 = 5 * 60 * 1000;

/// v4's daily-tick recent-run window (`RECENT_RUN_WINDOW_MS`, 20 h): skip the
/// startup tick when a successful sweep completed within this window (dev-restart
/// friendly).
pub const RECENT_RUN_WINDOW_MS: i64 = 20 * 60 * 60 * 1000;

/// v4's danger-scan interval (`DEFAULT_SCAN_INTERVAL_MS`, 10 min).
pub const DANGER_SCAN_INTERVAL_MS: i64 = 10 * 60 * 1000;

/// Clamp a raw wake delay to `[100 ms, MAX_WAKE_DELAY_MS]` (v4 `armWakeTimer`:
/// `Math.min(Math.max(rawMs, 100), MAX_WAKE_DELAY_MS)`).
///
/// `raw_ms` is `new Date(scheduledAt).getTime() - Date.now()` — negative when the
/// job is already due (clamped up to the 100 ms floor so the loop re-pumps
/// promptly), huge when a retry is far out (clamped to the 5-minute ceiling).
pub fn clamp_wake_delay(raw_ms: i64) -> i64 {
    // v4 `Math.min(Math.max(rawMs, 100), MAX_WAKE_DELAY_MS)` — equivalently a clamp
    // (the 100 ms floor is below the 5-minute ceiling, so `clamp` never panics).
    raw_ms.clamp(100, MAX_WAKE_DELAY_MS)
}

/// Whether a daily startup tick should run, given the last successful sweep's
/// instant (v4's `runStartup{Maintenance,Housekeeping}Tick` short-circuit).
///
/// Returns `false` (skip) only when `last_run_ms` is present AND `now_ms -
/// last_run_ms < RECENT_RUN_WINDOW_MS` — i.e. a sweep completed within the last
/// 20 h. A missing/never-run `last_run_ms` (or a failed recent-run check) always
/// runs, matching v4's "run anyway on a check failure" fallback (the caller passes
/// `None`). The recurring daily interval is unaffected — this gates only the
/// startup tick.
pub fn should_run_startup_tick(now_ms: i64, last_run_ms: Option<i64>) -> bool {
    match last_run_ms {
        Some(last) => now_ms - last >= RECENT_RUN_WINDOW_MS,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_delay_clamps_both_ends() {
        // Already-due (negative) → 100 ms floor.
        assert_eq!(clamp_wake_delay(-5_000), 100);
        assert_eq!(clamp_wake_delay(0), 100);
        assert_eq!(clamp_wake_delay(99), 100);
        // In-range passes through.
        assert_eq!(clamp_wake_delay(100), 100);
        assert_eq!(clamp_wake_delay(1_234), 1_234);
        assert_eq!(clamp_wake_delay(MAX_WAKE_DELAY_MS), MAX_WAKE_DELAY_MS);
        // Far-future → 5-minute ceiling.
        assert_eq!(clamp_wake_delay(MAX_WAKE_DELAY_MS + 1), MAX_WAKE_DELAY_MS);
        assert_eq!(clamp_wake_delay(i64::MAX), MAX_WAKE_DELAY_MS);
    }

    #[test]
    fn startup_tick_recent_run_window() {
        let now = 1_000_000_000_000;
        // Never run → run.
        assert!(should_run_startup_tick(now, None));
        // Ran just now → skip.
        assert!(!should_run_startup_tick(now, Some(now)));
        // Ran 19 h ago (< 20 h) → skip.
        assert!(!should_run_startup_tick(
            now,
            Some(now - 19 * 60 * 60 * 1000)
        ));
        // Ran exactly 20 h ago (boundary is inclusive-run) → run.
        assert!(should_run_startup_tick(
            now,
            Some(now - RECENT_RUN_WINDOW_MS)
        ));
        // Ran 21 h ago → run.
        assert!(should_run_startup_tick(
            now,
            Some(now - 21 * 60 * 60 * 1000)
        ));
    }
}
