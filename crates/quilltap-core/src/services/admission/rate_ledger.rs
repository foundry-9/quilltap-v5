//! The extraction rate limit's memory of recent admissions — v4
//! `lib/memory/admission/rate-ledger.ts` (`ac6348c5f`, memory refactor §3.7).
//!
//! v4's why, carried: "The limiter used to count a character's rows by
//! `createdAt` over the last hour, but `createdAt` is the source message's
//! time — a regenerate's rows are dated to their messages and never counted.
//! Admission runs in the parent, so the parent remembers when each AUTO row it
//! inserted landed, and the limiter asks it. The ledger lives in memory: a
//! restart forgets the last hour, which only ever errs towards admitting."
//!
//! v5 has no job child, so the count is always local (LR-4): v4's host-RPC
//! `countRecentAdmissions` and its WARN `[Memory] Could not read the admission
//! rate ledger; counting none` are unreachable and NO-PORT. On an instance
//! shared with a running v4, neither process sees the other's admissions
//! (R-AI — a hazards-block note, not code).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// How far back the ledger keeps insert times (v4 `RATE_LEDGER_WINDOW_MS`).
pub const RATE_LEDGER_WINDOW_MS: f64 = 60.0 * 60.0 * 1000.0;

/// characterId → insert times (ms), oldest first.
fn inserts() -> &'static Mutex<HashMap<String, Vec<f64>>> {
    static INSERTS: OnceLock<Mutex<HashMap<String, Vec<f64>>>> = OnceLock::new();
    INSERTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// v4 `prune`: keep the times within the window of `now`, drop an emptied
/// character.
fn prune(map: &mut HashMap<String, Vec<f64>>, character_id: &str, now: f64) -> Vec<f64> {
    let times: Vec<f64> = map
        .get(character_id)
        .map(|t| {
            t.iter()
                .copied()
                .filter(|t| now - t <= RATE_LEDGER_WINDOW_MS)
                .collect()
        })
        .unwrap_or_default();
    if times.is_empty() {
        map.remove(character_id);
    } else {
        map.insert(character_id.to_string(), times.clone());
    }
    times
}

/// Note an AUTO row admission just inserted (v4 `recordAdmissionInsert`;
/// prune, then push).
pub fn record_admission_insert(character_id: &str, at_ms: f64) {
    let mut map = inserts().lock().unwrap_or_else(|p| p.into_inner());
    let mut times = prune(&mut map, character_id, at_ms);
    times.push(at_ms);
    map.insert(character_id.to_string(), times);
}

/// AUTO rows inserted for a character since `since_ms`, as admission saw them
/// land (v4 `countRecentAdmissionsLocal`, the only arm v5 has).
pub fn count_recent_admissions(character_id: &str, since_ms: f64, now_ms: f64) -> usize {
    let mut map = inserts().lock().unwrap_or_else(|p| p.into_inner());
    prune(&mut map, character_id, now_ms)
        .into_iter()
        .filter(|t| *t >= since_ms)
        .count()
}

/// Test hook (v4 `resetRateLedgerForTests`).
pub fn reset_rate_ledger_for_tests() {
    inserts().lock().unwrap_or_else(|p| p.into_inner()).clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ledger is process-global; each test uses its own character id.
    #[test]
    fn counts_inserts_inside_the_window_and_forgets_older_ones() {
        let c = "rate-ledger-window";
        let t0 = 1_000_000_000_000.0;
        record_admission_insert(c, t0);
        record_admission_insert(c, t0 + 10.0);
        assert_eq!(count_recent_admissions(c, t0, t0 + 20.0), 2);
        assert_eq!(count_recent_admissions(c, t0 + 5.0, t0 + 20.0), 1);
        // An hour and a bit later the first two have aged out.
        let later = t0 + RATE_LEDGER_WINDOW_MS + 11.0;
        assert_eq!(count_recent_admissions(c, 0.0, later), 0);
    }

    #[test]
    fn the_window_edge_is_inclusive() {
        let c = "rate-ledger-edge";
        let t0 = 2_000_000_000_000.0;
        record_admission_insert(c, t0);
        assert_eq!(
            count_recent_admissions(c, 0.0, t0 + RATE_LEDGER_WINDOW_MS),
            1
        );
        assert_eq!(
            count_recent_admissions(c, 0.0, t0 + RATE_LEDGER_WINDOW_MS + 1.0),
            0
        );
    }

    #[test]
    fn an_unknown_character_counts_none() {
        assert_eq!(count_recent_admissions("rate-ledger-nobody", 0.0, 1.0), 0);
    }
}
