//! Phase 3 — demote, and Phase 4 — retire: v4
//! `lib/memory/maintenance/demote.ts` (`104e986c7`, at the `83334b9a5` pin).
//! Both pure planners over the run's corpus; `run.rs` issues the writes.
//!
//! v4's why, carried: "**Demote.** The cap applies to **demotable** rows only
//! — hot AUTO rows; MANUAL rows and digests are never demoted and never
//! counted (bug 210, decision 3). When demotable rows exceed the cap, exactly
//! the excess goes cold, lowest `protection` first (ties to the oldest). Age
//! and inactivity live in protection's content term; the old retention pass
//! and its three never-persisted knobs are gone (decision 7). **Retire.**
//! Cold, superseded AUTO rows whose `supersededAt` (when they went cold under
//! a digest, bug 225) is older than `coldRetentionDays` → `retireMemories(…,
//! { reason: 'retention' })`, which never takes a digest with its last
//! member."
//!
//! JS fidelity: the demotion order is V8's sort over `a.score − b.score ||
//! Date.parse(a.createdAt) − Date.parse(b.createdAt)` — a NaN date difference
//! compares equal ([`js_array_sort_by`], stable) — then `.slice(0, excess)`.

use serde_json::Value;

use super::corpus::{is_hot, tier_of};
use super::types::DemotePhaseReport;
use crate::jsnum::math_max;
use crate::memory_scoring::js_sort::js_array_sort_by;
use crate::memory_scoring::{
    date_ms, protection, truthy_date_ms, ProtectionBreakdown, ProtectionInput,
    DEFAULT_PROTECTION_CONFIG,
};

const DAY_MS: f64 = 86_400_000.0;

fn str_of<'a>(m: &'a Value, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str)
}

fn num_of(m: &Value, key: &str) -> Option<f64> {
    m.get(key).and_then(Value::as_f64)
}

/// v4 `Date.parse(m.createdAt)` (NaN when unparseable or absent).
pub fn created_ms(m: &Value) -> f64 {
    str_of(m, "createdAt").map_or(f64::NAN, date_ms)
}

/// v4 `protection(memory, { nowMs })` over a memory row's JSON: the fields
/// `ProtectionInput` picks (`importance`, `createdAt`, `reinforcedImportance`,
/// `lastReinforcedAt`, `reinforcementCount`, `relatedMemoryIds`,
/// `lastAccessedAt`, `kind`, `source`), each read as v4's `?? ` / truthiness
/// reads it.
pub fn protection_of(m: &Value, now_ms: f64) -> ProtectionBreakdown {
    let input = ProtectionInput {
        importance: num_of(m, "importance").unwrap_or(f64::NAN),
        reinforced_importance: num_of(m, "reinforcedImportance"),
        created_at_ms: created_ms(m),
        last_reinforced_at_ms: truthy_date_ms(str_of(m, "lastReinforcedAt")),
        last_accessed_at_ms: truthy_date_ms(str_of(m, "lastAccessedAt")),
        reinforcement_count: num_of(m, "reinforcementCount"),
        graph_degree: m
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        kind: str_of(m, "kind"),
        source: str_of(m, "source"),
    };
    protection(&input, now_ms, &DEFAULT_PROTECTION_CONFIG)
}

/// v4 `isDemotable(m)` — the rows the cap measures and may demote: hot AUTO
/// rows.
pub fn is_demotable(m: &Value) -> bool {
    is_hot(m) && str_of(m, "source") == Some("AUTO")
}

/// v4 `planDemotion(rows, cap, nowMs)` — which rows to demote so the
/// demotable count fits the cap.
pub fn plan_demotion(rows: &[&Value], cap: f64, now_ms: f64) -> DemotePhaseReport {
    let hot: Vec<&Value> = rows.iter().copied().filter(|m| is_hot(m)).collect();
    let demotable: Vec<&Value> = hot.iter().copied().filter(|m| is_demotable(m)).collect();
    let excess = math_max(0.0, demotable.len() as f64 - cap);
    let mut report = DemotePhaseReport {
        cap,
        hot_total: hot.len(),
        demotable: demotable.len(),
        protected: hot.len() - demotable.len(),
        excess,
        demoted: 0,
        demoted_ids: Vec::new(),
        highest_score_demoted: None,
    };
    if excess == 0.0 {
        return report;
    }

    let mut scored: Vec<(&Value, f64)> = demotable
        .into_iter()
        .map(|m| (m, protection_of(m, now_ms).score.score))
        .collect();
    js_array_sort_by(&mut scored, |a, b| {
        let d = a.1 - b.1;
        if d != 0.0 && !d.is_nan() {
            d
        } else {
            created_ms(a.0) - created_ms(b.0)
        }
    });
    // `.slice(0, excess)` — ToIntegerOrInfinity (a NaN excess takes nothing).
    let take = if excess.is_nan() {
        0
    } else {
        excess.trunc().min(scored.len() as f64) as usize
    };
    scored.truncate(take);
    report.demoted_ids = scored
        .iter()
        .map(|(m, _)| str_of(m, "id").unwrap_or_default().to_string())
        .collect();
    report.demoted = report.demoted_ids.len();
    report.highest_score_demoted = scored.last().map(|(_, s)| *s);
    report
}

/// v4 `planRetention(rows, retentionDays, nowMs)` — cold, superseded AUTO rows
/// past the retention window, by `supersededAt` ("rows superseded before the
/// column existed were backfilled from `updatedAt`" — `supersededAt ??
/// updatedAt`).
pub fn plan_retention(rows: &[&Value], retention_days: f64, now_ms: f64) -> Vec<String> {
    let cutoff_ms = now_ms - retention_days * DAY_MS;
    rows.iter()
        .filter(|m| {
            let superseded = m
                .get("supersededById")
                .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()));
            if tier_of(m) != "cold" || !superseded || str_of(m, "source") != Some("AUTO") {
                return false;
            }
            // `supersededAt ?? updatedAt` — `??`: an EMPTY `supersededAt` is
            // taken as it stands (and does not parse).
            let at = match m.get("supersededAt") {
                Some(Value::String(s)) => date_ms(s),
                Some(Value::Null) | None => str_of(m, "updatedAt").map_or(f64::NAN, date_ms),
                Some(_) => f64::NAN,
            };
            at.is_finite() && at < cutoff_ms
        })
        .map(|m| str_of(m, "id").unwrap_or_default().to_string())
        .collect()
}
