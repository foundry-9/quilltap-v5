//! Port of v4 `lib/memory/scoring/reinforced-importance.ts` — the stored
//! column every importance filter compares.
//!
//! v4's why, carried: pure and dependency-free, so the memories repository
//! can compute it inside its atomic reinforcement write, and the
//! `recompute-reinforced-importance-v1` migration shares the ONE formula
//! (no second formula anywhere — C1-4).

/// Per-doubling bonus that reinforcement adds to a memory's base importance.
pub const REINFORCED_IMPORTANCE_STEP: f64 = 0.05;

/// `min(1, importance + log2(count) × 0.05)` — v4 `reinforcedImportance`.
///
/// Locked decision 6 of the memory system refactor (bug 216): a row observed
/// once (count 1) scores exactly its base importance, so a fresh row — stored
/// with `reinforcedImportance = importance` — already agrees with the
/// formula, and its first reinforcement is a step along the curve rather than
/// a jump. A count below 1 (legacy rows, defensive callers) is treated as 1,
/// and so is a non-finite one (`Number.isFinite(count) ? Math.max(1, count) :
/// 1`). The cap is JS `Math.min`, so a NaN importance stays NaN.
pub fn reinforced_importance(importance: f64, count: f64) -> f64 {
    let n = if count.is_finite() {
        crate::jsnum::math_max(1.0, count)
    } else {
        1.0
    };
    crate::jsnum::math_min(1.0, importance + n.log2() * REINFORCED_IMPORTANCE_STEP)
}

#[cfg(test)]
mod tests {
    use super::reinforced_importance;

    #[test]
    fn count_one_is_the_base_and_low_counts_read_as_one() {
        assert_eq!(reinforced_importance(0.5, 1.0), 0.5);
        assert_eq!(reinforced_importance(0.5, 0.0), 0.5);
        assert_eq!(reinforced_importance(0.5, -3.0), 0.5);
        assert_eq!(reinforced_importance(0.5, f64::NAN), 0.5);
        assert_eq!(reinforced_importance(0.5, f64::INFINITY), 0.5);
        assert!((reinforced_importance(0.5, 2.0) - 0.55).abs() < 1e-15);
        assert_eq!(reinforced_importance(0.97, 64.0), 1.0);
    }
}
