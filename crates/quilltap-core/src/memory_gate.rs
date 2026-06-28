//! Port of the pure arithmetic from v4's lib/memory/memory-gate.ts.
//!
//! Only the reinforced-importance formula is ported here — the rest of the gate
//! (embedding similarity search, the INSERT/REINFORCE/SKIP decision, and the
//! regex-based `extractNovelDetails`) is stateful or regex-bearing and lands in
//! later phases.

/// Reinforced importance: `importance + log2(count + 1) * 0.05`, capped at 1.0.
/// `reinforcement_count` is an integer count; `log2(count + 1)` grows the floor
/// each time a memory is re-observed, saturating at the 1.0 ceiling.
pub fn calculate_reinforced_importance(base_importance: f64, reinforcement_count: f64) -> f64 {
    1.0_f64.min(base_importance + (reinforcement_count + 1.0).log2() * 0.05)
}
