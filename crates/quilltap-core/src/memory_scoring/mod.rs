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

#[cfg(test)]
mod jest_twins {
    //! Case-for-case port of v4
    //! `__tests__/unit/lib/memory/scoring/scoring-formulas.test.ts` (the pin
    //! `83334b9a5`) — "The three numbers (memory refactor §3.4, Phase A):
    //! relevance, salience and protection, plus the stored
    //! reinforcedImportance and the importance bands."
    //!
    //! The suite exercises v4's scoring BARREL, so its twin lives at the
    //! barrel. v4's `daysAgo(d)` builds an ISO string
    //! (`new Date(NOW_MS - d × 86_400_000).toISOString()`) that the formulas
    //! re-parse; the twin does the same through `js_iso_string` + the
    //! `from_iso` / [`truthy_date_ms`] readers. jest's `toBeCloseTo(x, p)`
    //! passes when `|a − x| < 10^-p / 2`; every other assertion is exact.

    use super::*;
    use crate::memory_time::calendar::js_iso_string;

    fn now_ms() -> f64 {
        date_ms("2026-03-01T00:00:00.000Z")
    }

    fn days_ago(d: f64) -> String {
        js_iso_string(now_ms() - d * 86_400_000.0).expect("in range")
    }

    fn close_to(actual: f64, expected: f64, digits: i32) -> bool {
        (actual - expected).abs() < 10f64.powi(-digits) / 2.0
    }

    // -- describe('reinforcedImportance(i, n) = min(1, i + log2(n) × 0.05)') --

    /// `it.each(...)('importance %f at count %f → %f')`.
    #[test]
    fn importance_f_at_count_f_to_f() {
        let rows: [(f64, f64, f64); 8] = [
            (0.5, 1.0, 0.5),
            (0.5, 2.0, 0.55),
            (0.5, 4.0, 0.6),
            (0.5, 8.0, 0.65),
            (0.6, 3.0, 0.6 + 3f64.log2() * 0.05),
            (0.98, 4.0, 1.0),
            (0.5, 0.0, 0.5), // a count below 1 reads as 1
            (0.5, f64::NAN, 0.5),
        ];
        for (i, n, expected) in rows {
            let got = reinforced_importance(i, n);
            assert!(
                close_to(got, expected, 10),
                "importance {i} at count {n}: got {got}, want {expected}"
            );
        }
    }

    /// `it('a fresh row (count 1) equals its importance — no
    /// first-reinforcement step (bug 216)')`.
    #[test]
    fn a_fresh_row_count_1_equals_its_importance_no_first_reinforcement_step_bug_216() {
        for i in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(reinforced_importance(i, 1.0), i);
        }
    }

    // -- describe('salience = reinforcedImportance × 0.5^(days / 30), no floor') --

    /// `it.each(...)('ri %f created %f days ago, reinforced %p → %f')`.
    #[test]
    fn ri_f_created_f_days_ago_reinforced_p_to_f() {
        // [reinforcedImportance, createdDaysAgo, lastReinforcedDaysAgo, expected]
        let rows: [(f64, f64, Option<f64>, f64); 7] = [
            (0.8, 0.0, None, 0.8),
            (0.8, 30.0, None, 0.4),
            (0.8, 60.0, None, 0.2),
            (0.8, 90.0, None, 0.1),
            (0.8, 365.0, None, 0.8 * 0.5f64.powf(365.0 / 30.0)), // far below the old 0.70 floor
            (0.6, 60.0, Some(30.0), 0.3),                        // reinforcement resets the clock
            (0.6, 60.0, Some(90.0), 0.15),                       // an older reinforcement does not
        ];
        for (ri, created, reinforced, expected) in rows {
            let reinforced_iso = reinforced.map(days_ago);
            let input = SalienceInput::from_iso(
                0.1,
                Some(ri),
                &days_ago(created),
                reinforced_iso.as_deref(),
            );
            let s = salience(&input, now_ms());
            assert!(
                close_to(s.salience, expected, 10),
                "ri {ri} created {created} reinforced {reinforced:?}: got {}, want {expected}",
                s.salience
            );
            assert_eq!(s.base, ri);
        }
    }

    /// `it('falls back to importance when reinforcedImportance is absent')`.
    #[test]
    fn falls_back_to_importance_when_reinforced_importance_is_absent() {
        let input = SalienceInput::from_iso(0.4, None, &days_ago(0.0), None);
        assert!(close_to(salience(&input, now_ms()).salience, 0.4, 2));
    }

    /// `it('never reads lastAccessedAt and never decays a future-dated row
    /// above its base')`. (`SalienceInput` has no `lastAccessedAt` field at
    /// all — the first half is structural in Rust.)
    #[test]
    fn never_reads_last_accessed_at_and_never_decays_a_future_dated_row_above_its_base() {
        let input = SalienceInput::from_iso(0.5, None, &days_ago(-5.0), None);
        let s = salience(&input, now_ms());
        // jest `toBe(0)` is `Object.is` — a positive zero.
        assert_eq!(s.days_since, 0.0);
        assert!(s.days_since.is_sign_positive());
        assert_eq!(s.salience, 0.5);
    }

    /// `it('referenceTimeMs is max(createdAt, lastReinforcedAt)')`.
    #[test]
    fn reference_time_ms_is_max_created_at_last_reinforced_at() {
        assert_eq!(
            reference_time_ms(
                date_ms(&days_ago(10.0)),
                truthy_date_ms(Some(&days_ago(2.0)))
            ),
            now_ms() - 2.0 * 86_400_000.0
        );
        assert_eq!(
            reference_time_ms(date_ms(&days_ago(10.0)), truthy_date_ms(None)),
            now_ms() - 10.0 * 86_400_000.0
        );
    }

    /// `it('sortBySalience orders highest first and keeps ties stable')`.
    #[test]
    fn sort_by_salience_orders_highest_first_and_keeps_ties_stable() {
        let rows = vec![
            ("old-high", 0.9, days_ago(120.0)), // 0.05625
            ("new-low", 0.3, days_ago(0.0)),    // 0.3
            ("tie-a", 0.6, days_ago(30.0)),     // 0.3
            ("mid", 0.5, days_ago(0.0)),        // 0.5
        ];
        let sorted = sort_by_salience(
            rows,
            |r| SalienceInput::from_iso(r.1, None, &r.2, None),
            now_ms(),
        );
        let ids: Vec<&str> = sorted.iter().map(|r| r.0).collect();
        assert_eq!(ids, vec!["mid", "new-low", "tie-a", "old-high"]);
    }

    // -- describe('protection — formula unchanged, MANUAL and digests always protected') --

    /// v4's `base` fixture, as owned strings the overrides replace.
    #[derive(Clone)]
    struct Base {
        importance: f64,
        reinforced_importance: f64,
        created_at: String,
        reinforcement_count: f64,
        related_memory_ids: Vec<&'static str>,
        last_accessed_at: Option<String>,
        kind: &'static str,
        source: &'static str,
    }

    fn base() -> Base {
        Base {
            importance: 0.6,
            reinforced_importance: 0.6,
            created_at: days_ago(0.0),
            reinforcement_count: 1.0,
            related_memory_ids: vec![],
            last_accessed_at: None,
            kind: "semantic",
            source: "AUTO",
        }
    }

    fn protect(b: &Base) -> ProtectionBreakdown {
        let input = ProtectionInput {
            importance: b.importance,
            reinforced_importance: Some(b.reinforced_importance),
            created_at_ms: date_ms(&b.created_at),
            last_reinforced_at_ms: None,
            last_accessed_at_ms: truthy_date_ms(b.last_accessed_at.as_deref()),
            reinforcement_count: Some(b.reinforcement_count),
            graph_degree: b.related_memory_ids.len(),
            kind: Some(b.kind),
            source: Some(b.source),
        };
        protection(&input, now_ms(), &DEFAULT_PROTECTION_CONFIG)
    }

    /// `it.each(...)('%s')` — the ten labelled protection rows.
    #[test]
    fn protection_labelled_rows() {
        let rows: Vec<(&str, Base, f64)> = vec![
            (
                "fresh, content capped at 0.40",
                base(),
                0.4 + 2f64.log2() * 0.08,
            ),
            (
                "60 days old",
                Base {
                    created_at: days_ago(60.0),
                    ..base()
                },
                0.6 * 0.25 + 0.08,
            ),
            (
                "very old hits the content floor",
                Base {
                    created_at: days_ago(3650.0),
                    ..base()
                },
                0.6 * 0.1 + 0.08,
            ),
            (
                "reinforced 7 times",
                Base {
                    reinforcement_count: 7.0,
                    ..base()
                },
                0.4 + 0.24,
            ),
            (
                "reinforcement bonus caps at 0.25",
                Base {
                    reinforcement_count: 63.0,
                    ..base()
                },
                0.4 + 0.25,
            ),
            (
                "three links",
                Base {
                    related_memory_ids: vec!["a", "b", "c"],
                    ..base()
                },
                0.4 + 0.08 + 0.075,
            ),
            (
                "graph bonus caps at 0.10",
                Base {
                    related_memory_ids: vec!["a", "b", "c", "d", "e", "f"],
                    ..base()
                },
                0.4 + 0.08 + 0.1,
            ),
            (
                "accessed recently",
                Base {
                    last_accessed_at: Some(days_ago(10.0)),
                    ..base()
                },
                0.4 + 0.08 + 0.1,
            ),
            (
                "accessed long ago",
                Base {
                    last_accessed_at: Some(days_ago(120.0)),
                    ..base()
                },
                0.4 + 0.08,
            ),
            (
                "episodic",
                Base {
                    kind: "episodic",
                    ..base()
                },
                0.4 + 0.08 + 0.1,
            ),
        ];
        for (label, b, expected) in rows {
            let result = protect(&b);
            assert!(
                close_to(result.score.score, expected, 10),
                "{label}: got {}, want {expected}",
                result.score.score
            );
            assert_eq!(
                result.protected,
                result.score.score >= PROTECTION_THRESHOLD,
                "{label}"
            );
            assert!(!result.protected_by_source, "{label}");
        }
    }

    /// `it.each(['MANUAL', 'CONSOLIDATED'])('%s rows are protected with score 1')`.
    #[test]
    fn s_rows_are_protected_with_score_1() {
        for source in ["MANUAL", "CONSOLIDATED"] {
            let result = protect(&Base {
                source,
                created_at: days_ago(3650.0),
                importance: 0.01,
                reinforced_importance: 0.01,
                ..base()
            });
            assert_eq!(result.score.score, 1.0, "{source}");
            assert!(result.protected, "{source}");
            assert!(result.protected_by_source, "{source}");
        }
    }

    // -- describe('relevance and the blend') --

    /// `it('relevance is the cosine')`.
    #[test]
    fn relevance_is_the_cosine() {
        assert_eq!(relevance(0.42), 0.42);
    }

    /// `it.each(...)('blend(%f, %f) = %f')`.
    #[test]
    fn blend_f_f_equals_f() {
        for (cos, sal, expected) in [(1.0, 0.0, 0.75), (0.0, 1.0, 0.25), (0.6, 0.4, 0.55)] {
            let got = compute_ranking_blend(cos, sal);
            assert!(
                close_to(got, expected, 10),
                "blend({cos}, {sal}): got {got}"
            );
        }
    }

    /// `it('the cosine floor is provider-aware')`.
    #[test]
    fn the_cosine_floor_is_provider_aware() {
        assert_eq!(default_min_cosine_for_provider(Some("BUILTIN")), 0.1);
        assert_eq!(default_min_cosine_for_provider(Some("OPENAI")), 0.3);
        assert_eq!(default_min_cosine_for_provider(None), 0.3);
    }

    // -- describe('importance filters and bands') --

    /// `it('two named floors')`.
    #[test]
    fn two_named_floors() {
        assert_eq!(RECALL_MIN_IMPORTANCE, 0.3);
        assert_eq!(TOOL_MIN_IMPORTANCE, 0.0);
    }

    /// `it.each(...)('importanceBand(%f) = %s')`.
    #[test]
    fn importance_band_f_equals_s() {
        let rows = [
            (1.0, "high"),
            (0.7, "high"),
            (0.69, "medium"),
            (0.3, "medium"),
            (0.29, "low"),
            (0.0, "low"),
        ];
        for (value, band) in rows {
            assert_eq!(
                importance_band(value).as_str(),
                band,
                "importanceBand({value})"
            );
        }
    }

    /// `it('filters read reinforcedImportance, then importance')`. v4's
    /// absent and `null` `reinforcedImportance` are both `None` here.
    #[test]
    fn filters_read_reinforced_importance_then_importance() {
        assert_eq!(filter_importance(0.65, Some(0.73)), 0.73);
        assert_eq!(filter_importance(0.65, None), 0.65);
        assert_eq!(filter_importance(0.65, None), 0.65);
    }
}
