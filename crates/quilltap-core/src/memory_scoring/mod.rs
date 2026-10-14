//! Port of v4's `lib/memory/scoring/` (`e0ac7493a`, Phase A of the memory
//! system refactor; bug 216's formula from `c2795733f`) — the ONE home of
//! every memory worth score.
//!
//! v4's header, carried: "Every worth-ordering in the memory system goes
//! through `salience` or `protection`; nothing else re-derives a worth
//! score." Salience is how much a memory is worth SAYING (the only ordering
//! a model sees); protection is how much it is worth KEEPING (maintenance's
//! demotion gate and order). The old floored effective weight is gone — v4
//! deleted it, and its 0.70 floor had leaked into ranking at four sites.
//!
//! `rank.ts` (`rankForRecall` / `rankByBlend` / `reserveBackgroundSlots`) is
//! a recall stage and lives with the recall lane (P4.D291), not here.
//!
//! Every submodule is pure; times are epoch milliseconds the caller parsed
//! (`SalienceInput::from_iso` / `ProtectionInput` constructors read v4's
//! `new Date(x)` semantics — an unparseable stamp is NaN, which propagates).

pub mod importance;
pub mod js_sort;
pub mod protection;
pub mod reinforced_importance;
pub mod relevance;
pub mod salience;

pub use importance::{
    filter_importance, importance_band, ImportanceBand, IMPORTANCE_HIGH, IMPORTANCE_LOW,
    RECALL_MIN_IMPORTANCE, TOOL_MIN_IMPORTANCE,
};
pub use protection::{
    calculate_protection_score, protection, ProtectionBreakdown, ProtectionConfig, ProtectionInput,
    ProtectionScore, DEFAULT_PROTECTION_CONFIG, PROTECTION_THRESHOLD,
};
pub use reinforced_importance::{reinforced_importance, REINFORCED_IMPORTANCE_STEP};
pub use relevance::{
    compute_ranking_blend, default_min_cosine_for_provider, relevance, DEFAULT_MIN_COSINE_NEURAL,
    DEFAULT_MIN_COSINE_TFIDF, RANKING_PRIORITY_WEIGHT, RANKING_RELEVANCE_WEIGHT,
};
pub use salience::{
    reference_time_ms, salience, sort_by_salience, SalienceBreakdown, SalienceInput,
    SALIENCE_HALF_LIFE_DAYS,
};

/// v4 `new Date(s).getTime()` for a timestamp string: the epoch milliseconds,
/// or NaN when `Date.parse` refuses it. (Zone-less datetimes read as UTC —
/// the `ZONELESS_PARSE_AS_UTC` ruled divergence; see
/// [`crate::episodic::js_date_parse_ms`].)
pub fn date_ms(s: &str) -> f64 {
    crate::episodic::js_date_parse_ms(s)
        .map(|ms| ms as f64)
        .unwrap_or(f64::NAN)
}

/// v4's `x ? new Date(x).getTime() : <absent>` for an optional stamp: `None`
/// for an ABSENT or EMPTY string (JS-falsy), else [`date_ms`] (NaN when
/// unparseable).
pub fn truthy_date_ms(s: Option<&str>) -> Option<f64> {
    match s {
        Some(v) if !v.is_empty() => Some(date_ms(v)),
        _ => None,
    }
}
