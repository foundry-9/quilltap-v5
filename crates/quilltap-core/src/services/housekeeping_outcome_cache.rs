//! Housekeeping outcome cache (v4 `lib/memory/housekeeping-outcome-cache.ts`):
//! a per-process in-memory record of the most recent completed housekeeping
//! sweep per character, gating the post-extraction watermark enqueue path so
//! an ineffective sweep isn't immediately re-run.
//!
//! **Rust home decision:** v4 holds this as a module-global `Map` (deliberately
//! in-memory — losing it on restart costs one extra sweep attempt per
//! character, cheap next to a persistent per-character outcome row). The port
//! keeps the same process-global shape: a `OnceLock<Mutex<HashMap>>` static.
//! Entries are keyed by characterId, so parallel tests on distinct characters
//! stay isolated; the watermark differential runs in its own test process.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::clock::now_unix_ms;

const INEFFECTIVE_BACKOFF_MS: i64 = 60 * 60 * 1000; // 1 hour

/// A sweep counts as "effective" when it deleted at least this many rows.
const MIN_EFFECTIVE_DELETIONS: f64 = 10.0;

/// Above this excess (count − cap), a sweep must delete at least
/// `excess × EXCESS_RATIO_THRESHOLD` rows to count as effective.
const EXCESS_RATIO_THRESHOLD: f64 = 0.01;

#[derive(Clone, Copy, Debug)]
struct SweepOutcome {
    completed_at_ms: i64,
    deleted: f64,
    total_before: f64,
    cap: f64,
}

fn outcomes() -> &'static Mutex<HashMap<String, SweepOutcome>> {
    static OUTCOMES: OnceLock<Mutex<HashMap<String, SweepOutcome>>> = OnceLock::new();
    OUTCOMES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// v4 `recordHousekeepingOutcome`: note a completed sweep's effect for the
/// watermark gate. Called by the housekeeping job driver after a sweep, with
/// what the sweep moved out of the hot tier — effective when it demoted (or
/// deleted) rows (v4 `d58548051`: the driver passes `demoted + deleted`).
pub fn record_housekeeping_outcome(character_id: &str, deleted: f64, total_before: f64, cap: f64) {
    // A non-finite count would make every "ineffective?" comparison false and
    // silently disarm the backoff; treat it as nothing deleted.
    let deleted = if deleted.is_finite() { deleted } else { 0.0 };
    outcomes().lock().expect("outcome cache lock").insert(
        character_id.to_string(),
        SweepOutcome {
            completed_at_ms: now_unix_ms(),
            deleted,
            total_before,
            cap,
        },
    );
}

/// v4 `shouldSkipWatermarkSweep`: true when the previous sweep for this
/// character was ineffective within the last hour — it deleted fewer rows
/// than both a small floor (10) and 1% of the excess over the cap.
pub fn should_skip_watermark_sweep(character_id: &str) -> bool {
    let outcomes = outcomes().lock().expect("outcome cache lock");
    let Some(last) = outcomes.get(character_id) else {
        return false;
    };
    if now_unix_ms() - last.completed_at_ms >= INEFFECTIVE_BACKOFF_MS {
        return false;
    }

    let excess = (last.total_before - last.cap).max(0.0);
    let min_effective = MIN_EFFECTIVE_DELETIONS.max((excess * EXCESS_RATIO_THRESHOLD).floor());
    last.deleted < min_effective
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ineffective_sweep_backs_off_and_effective_does_not() {
        // Ineffective: 3 deletions on an excess of 96 (min effective = 10).
        record_housekeeping_outcome("cache-test-ineffective", 3.0, 100.0, 4.0);
        assert!(should_skip_watermark_sweep("cache-test-ineffective"));

        // Effective: 50 deletions clears both the floor and the 1% ratio.
        record_housekeeping_outcome("cache-test-effective", 50.0, 100.0, 4.0);
        assert!(!should_skip_watermark_sweep("cache-test-effective"));

        // Unknown character: never skipped.
        assert!(!should_skip_watermark_sweep("cache-test-unknown"));
    }

    /// v4 `3f7320138` F8 (`memory-recall-housekeeping-fixes.test.ts:326-335`): a
    /// non-finite count is recorded as nothing deleted, so the backoff ARMS
    /// rather than every comparison going false.
    #[test]
    fn a_non_finite_count_arms_the_backoff() {
        record_housekeeping_outcome("cache-test-nan", f64::NAN, 100.0, 4.0);
        assert!(should_skip_watermark_sweep("cache-test-nan"));
    }
}
