//! Port of the pure pieces of v4's lib/memory/memory-gate.ts: the
//! reinforced-importance formula, the deterministic regex-based
//! `extractNovelDetails`, and the episodic date guard
//! (`DATE_GUARD_DAYS` / `occasionsAreDistinct`). The stateful gate itself
//! (embedding search + the INSERT/REINFORCE/SKIP decision) lives in
//! [`crate::services::memory_gate`].
//!
//! **LANE-REMOVE shims (work order P4.D289, §R.10(o)).** v4 `8af764514`
//! DELETED `memory-gate.ts` for `lib/memory/admission/*` (ported as
//! [`crate::admission`] and [`crate::reinforcement_footnotes`]), and
//! `e0ac7493a` moved the formula into `lib/memory/scoring/` (ported as
//! [`crate::memory_scoring::reinforced_importance`]). The callers of this
//! module live in other lanes' files (the keystone's repository P4.D288,
//! admission P4.D290, maintenance P4.D297), so it stays until they re-point.
//! The items whose successor has IDENTICAL semantics FORWARD to it — and so
//! does the formula, deliberately (LR-1 / C1-4: ONE formula from `SCORING`
//! on). The 2-argument `append_capped_footnotes`, whose semantics MOVED (v4
//! added the skip-present and the cap argument), keeps its old body
//! verbatim, so no caller's behaviour moves before its owner re-points it.

/// When candidate and best-match `occurredAt` differ by more than this many
/// days, SKIP_NEAR_DUPLICATE / REINFORCE downgrade to INSERT_RELATED so
/// distinct occasions of the same activity both persist as rows (v4
/// `DATE_GUARD_DAYS`; now `AdmissionThresholds.date_guard_days`).
// LANE-REMOVE(P4.D288|P4.D290|P4.D297): v4 deleted memory-gate.ts's DATE_GUARD_DAYS in 8af764514
pub const DATE_GUARD_DAYS: i64 = 7;

/// True when both timestamps parse and sit more than [`DATE_GUARD_DAYS`] apart
/// — forwards to [`crate::admission::occasions_are_distinct`] with the guard
/// as its third argument.
// LANE-REMOVE(P4.D288|P4.D290|P4.D297): v4 deleted memory-gate.ts's 2-argument occasionsAreDistinct in 8af764514
pub fn occasions_are_distinct(
    candidate_occurred_at: Option<&str>,
    existing_occurred_at: Option<&str>,
) -> bool {
    crate::admission::occasions_are_distinct(
        candidate_occurred_at,
        existing_occurred_at,
        DATE_GUARD_DAYS as f64,
    )
}

/// Reinforced importance — FORWARDS to v4's ONE formula,
/// [`crate::memory_scoring::reinforced_importance`] (bug 216:
/// `min(1, i + log2(max(1, n)) × 0.05)`; was `log2(n + 1)` here). The one
/// deliberate behaviour forward of P4.D289's shims (LR-1 / C1-4): every
/// caller still on this path computes v4's new value from `SCORING` on, so
/// the old scale's live-data hazard closes toward v4.
// LANE-REMOVE(P4.D288|P4.D290|P4.D297): v4 deleted reinforced-importance.ts's calculateReinforcedImportance in e0ac7493a
pub fn calculate_reinforced_importance(base_importance: f64, reinforcement_count: f64) -> f64 {
    crate::memory_scoring::reinforced_importance(base_importance, reinforcement_count)
}

/// Most `[+]` footnote lines a memory may carry.
// LANE-REMOVE(P4.D288|P4.D290|P4.D297): v4 deleted memory-gate.ts's MAX_REINFORCEMENT_FOOTNOTES in 8af764514
pub use crate::reinforcement_footnotes::MAX_REINFORCEMENT_FOOTNOTES;

/// Number of `[+]` footnote lines already appended to a memory's content.
// LANE-REMOVE(P4.D288|P4.D290|P4.D297): v4 deleted memory-gate.ts's countReinforcementFootnotes in 8af764514
pub use crate::reinforcement_footnotes::count_reinforcement_footnotes;

/// Append novel details to `content` as `[+]` footnotes, stopping at
/// [`MAX_REINFORCEMENT_FOOTNOTES`] (v4 `appendCappedFootnotes`). Returns the new
/// content and the details actually appended (a prefix of `details`, possibly
/// empty — then the content comes back unchanged).
// LANE-REMOVE(P4.D288|P4.D290|P4.D297): v4 deleted memory-gate.ts's appendCappedFootnotes (no skip-present, no cap) in 8af764514
pub fn append_capped_footnotes(content: &str, details: &[String]) -> (String, Vec<String>) {
    let room = crate::reinforcement_footnotes::MAX_REINFORCEMENT_FOOTNOTES
        .saturating_sub(crate::reinforcement_footnotes::count_reinforcement_footnotes(content));
    let appended: Vec<String> = details.iter().take(room).cloned().collect();
    if appended.is_empty() {
        return (content.to_string(), appended);
    }
    let footnotes = appended
        .iter()
        .map(|d| format!("[+] {d}"))
        .collect::<Vec<_>>()
        .join("\n");
    (format!("{content}\n{footnotes}"), appended)
}

/// Extract novel, deterministic details — forwards to
/// [`crate::admission::extract_novel_details`] (v4 moved it byte-identical).
// LANE-REMOVE(P4.D288|P4.D290|P4.D297): v4 deleted memory-gate.ts's extractNovelDetails in 8af764514
pub use crate::admission::extract_novel_details;

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported case-for-case from v4's `__tests__/gate-date-guard.test.ts`
    /// (`describe('occasionsAreDistinct')`). The retro-signature half of that
    /// file belongs to `recall_history` (the sibling lane's file).
    #[test]
    fn occasions_are_distinct_beyond_the_guard_window() {
        assert!(occasions_are_distinct(
            Some("2026-03-10T00:00:00.000Z"),
            Some("2026-07-14T00:00:00.000Z")
        ));
    }

    #[test]
    fn occasions_within_the_guard_window_are_the_same_occasion() {
        assert!(!occasions_are_distinct(
            Some("2026-07-14T00:00:00.000Z"),
            Some("2026-07-16T00:00:00.000Z")
        ));
    }

    #[test]
    fn exactly_the_guard_window_is_the_same_occasion() {
        // Strictly-greater guard: exactly DATE_GUARD_DAYS apart does NOT fire.
        let a = "2026-07-07T00:00:00.000Z".to_string();
        let b = format!("2026-07-{}T00:00:00.000Z", 7 + DATE_GUARD_DAYS);
        assert!(!occasions_are_distinct(Some(&a), Some(&b)));
    }

    #[test]
    fn the_guard_never_fires_without_two_event_times() {
        assert!(!occasions_are_distinct(
            None,
            Some("2026-07-14T00:00:00.000Z")
        ));
        assert!(!occasions_are_distinct(
            Some("2026-07-14T00:00:00.000Z"),
            None
        ));
        assert!(!occasions_are_distinct(
            Some("garbage"),
            Some("2026-07-14T00:00:00.000Z")
        ));
        // JS-falsy: an empty string is "no event time" too.
        assert!(!occasions_are_distinct(
            Some(""),
            Some("2026-07-14T00:00:00.000Z")
        ));
    }
}
