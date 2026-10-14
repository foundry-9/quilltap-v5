//! Port of lib/memory/memory-weighting.ts — the pure scoring functions.
//!
//! **LANE-REMOVE shims (work order P4.D289, §R.10(o)).** v4 `e0ac7493a`
//! DELETED this module for `lib/memory/scoring/*` — ported as
//! [`crate::memory_scoring`] (salience, protection, the blend, the floors)
//! and [`crate::memory_time::format_memory_age`]. Its callers live in other
//! lanes' files (recall P4.D291, the optimizer P4.D292, housekeeping
//! P4.D297), so it stays until they re-point: the items whose successor has
//! IDENTICAL semantics FORWARD to it; the floored effective weight, which v4
//! deleted with no successor, keeps its old body verbatim so no caller's
//! behaviour moves before its owner re-points it (LR-1).
//!
//! Tier-1 differential target: deterministic, side-effect-free, exactly
//! checkable against the TS oracle. Every constant and the order of operations
//! mirror the source so results are byte-equal. The subtle bits that a port
//! gets wrong if rushed, preserved here verbatim from the TS comments:
//!   * reference time is max(createdAt, lastReinforcedAt) — passive access
//!     (lastAccessedAt) does NOT reset the decay clock;
//!   * base importance is reinforcedImportance ?? importance;
//!   * protection content component is capped at maxContentContribution (0.40);
//!   * reinforcement bonus saturates via log2(count+1);
//!   * final protection score is clamped to 1.0.

/// Milliseconds per day (matches the TS `86400000`).
const MS_PER_DAY: f64 = 86_400_000.0;

// LANE-REMOVE(P4.D291|P4.D292|P4.D297): v4 deleted MemoryWeightingConfig in e0ac7493a
#[derive(Clone, Copy)]
pub struct WeightingConfig {
    pub half_life_days: f64,
    pub importance_floor: f64,
    pub min_weight_threshold: f64,
}

// LANE-REMOVE(P4.D291|P4.D292|P4.D297): v4 deleted DEFAULT_WEIGHTING_CONFIG in e0ac7493a
pub const DEFAULT_WEIGHTING_CONFIG: WeightingConfig = WeightingConfig {
    half_life_days: 30.0,
    importance_floor: 0.70,
    min_weight_threshold: 0.05,
};

// LANE-REMOVE(P4.D291|P4.D292|P4.D297): v4 deleted ProtectionConfig / DEFAULT_PROTECTION_CONFIG / ProtectionScore in e0ac7493a
pub use crate::memory_scoring::{ProtectionConfig, ProtectionScore, DEFAULT_PROTECTION_CONFIG};

/// The subset of a Memory the scoring functions read. Times are epoch millis
/// (the harness converts ISO strings → millis the same way `new Date(x)` does).
// LANE-REMOVE(P4.D291|P4.D292|P4.D297): v4 deleted the memory-weighting input view in e0ac7493a
#[derive(Clone, Copy, Default)]
pub struct MemoryInputs {
    pub importance: f64,
    pub reinforced_importance: Option<f64>,
    pub created_at_ms: f64,
    pub last_reinforced_at_ms: Option<f64>,
    pub last_accessed_at_ms: Option<f64>,
    pub reinforcement_count: Option<u64>,
    pub graph_degree: usize, // relatedMemoryIds.length
    /// Episodic spine (v4 8bf3cb5f): `memory.kind === 'episodic'`.
    pub kind_episodic: bool,
    /// Pre-parsed event time — `Some` only when `occurredAt` is present,
    /// non-empty, and parsable (build via [`crate::episodic::event_time_ms`]);
    /// mirrors v4's `eventReferenceTimeMs(occurredAt, …)` fallback contract.
    pub occurred_at_ms: Option<f64>,
}

// LANE-REMOVE(P4.D291|P4.D292|P4.D297): v4 deleted EffectiveWeightResult in e0ac7493a
pub struct EffectiveWeight {
    pub effective_weight: f64,
    pub raw_weight: f64,
    pub min_weight: f64,
    pub time_decay_factor: f64,
    pub days_old: f64,
    pub base_importance: f64,
}

// LANE-REMOVE(P4.D291|P4.D292|P4.D297): v4 deleted calculateEffectiveWeight in e0ac7493a
pub fn calculate_effective_weight(
    m: &MemoryInputs,
    cfg: &WeightingConfig,
    now_ms: f64,
) -> EffectiveWeight {
    let base_importance = m.reinforced_importance.unwrap_or(m.importance);
    let reinforced = m.last_reinforced_at_ms.unwrap_or(0.0);
    let reference = m.created_at_ms.max(reinforced);
    let days_old = ((now_ms - reference) / MS_PER_DAY).max(0.0);
    let time_decay_factor = 0.5_f64.powf(days_old / cfg.half_life_days);
    let raw_weight = base_importance * time_decay_factor;
    let min_weight = base_importance * cfg.importance_floor;
    let effective_weight = raw_weight.max(min_weight);
    EffectiveWeight {
        effective_weight,
        raw_weight,
        min_weight,
        time_decay_factor,
        days_old,
        base_importance,
    }
}

// LANE-REMOVE(P4.D291|P4.D292|P4.D297): v4 deleted calculateProtectionScore's memory-weighting home in e0ac7493a
pub fn calculate_protection_score(
    m: &MemoryInputs,
    cfg: &ProtectionConfig,
    now_ms: f64,
) -> ProtectionScore {
    crate::memory_scoring::calculate_protection_score(
        &crate::memory_scoring::ProtectionInput {
            importance: m.importance,
            reinforced_importance: m.reinforced_importance,
            created_at_ms: m.created_at_ms,
            last_reinforced_at_ms: m.last_reinforced_at_ms,
            last_accessed_at_ms: m.last_accessed_at_ms,
            reinforcement_count: m.reinforcement_count.map(|n| n as f64),
            graph_degree: m.graph_degree,
            kind: m.kind_episodic.then_some("episodic"),
            source: None,
        },
        cfg,
        now_ms,
    )
}

// ---------------------------------------------------------------------------
// Retrieval ranking blend + relevance floors (ports of the same TS file).
// ---------------------------------------------------------------------------

// LANE-REMOVE(P4.D291|P4.D292|P4.D297): v4 deleted the memory-weighting blend + floors in e0ac7493a
pub use crate::memory_scoring::{
    compute_ranking_blend, default_min_cosine_for_provider, DEFAULT_MIN_COSINE_NEURAL,
    DEFAULT_MIN_COSINE_TFIDF, RANKING_PRIORITY_WEIGHT, RANKING_RELEVANCE_WEIGHT,
};

/// Human-readable relative age (v4's deleted `formatRelativeAge`) — the
/// event clock when `occurredAt` parsed, else the write clock
/// `max(createdAt, lastReinforcedAt)`, on the shared day ladder; the same
/// reading as [`crate::memory_time::format_memory_age`], over this module's
/// pre-parsed event time.
// LANE-REMOVE(P4.D291|P4.D292|P4.D297): v4 deleted formatRelativeAge in e0ac7493a
pub fn format_relative_age(m: &MemoryInputs, now_ms: f64) -> String {
    let write_reference =
        crate::memory_scoring::reference_time_ms(m.created_at_ms, m.last_reinforced_at_ms);
    crate::format_time::format_relative_days(m.occurred_at_ms.unwrap_or(write_reference), now_ms)
}
