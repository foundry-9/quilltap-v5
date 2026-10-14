//! Port of v4 `lib/memory/scoring/protection.ts` — how much a memory is worth
//! KEEPING.
//!
//! v4's why, carried: the blended protection score (content ≤ 0.40,
//! reinforcement ≤ 0.25, graph ≤ 0.10, recent access +0.10, episodic +0.10;
//! threshold 0.5) decides whether maintenance may demote a row, and it is
//! also the demotion order: the lowest-protection row goes first, so the
//! thing that decides whether a row may go also decides which goes first.
//! Ranking for a model is `salience`, never this.
//!
//! ⚠ The reinforcement bonus still reads `log2(count + 1)` — bug 216 moved
//! reinforced IMPORTANCE onto `log2(max(1, count))`, not this bonus (v4
//! `protection.ts:131`). Do not "fix" it.

use crate::jsnum::{math_max, math_min};

/// At or above this score an AUTO row is protected from demotion.
pub const PROTECTION_THRESHOLD: f64 = 0.5;

const DAY_MS: f64 = 86_400_000.0;

/// Configuration for the protection score (v4 `ProtectionScoreConfig`).
///
/// Distinct from retrieval ranking: protection decay runs on a longer
/// half-life with a lower floor, and the score blends the LLM-derived content
/// signal with usage evidence (reinforcement, graph degree, recency of
/// access) so a memory's fate doesn't hinge on the LLM's one-shot importance
/// guess.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProtectionConfig {
    /// Content-score half-life in days. Default 30. The original 365-day
    /// default protected fresh LLM-scored memories indefinitely on heavy
    /// characters, so the cap-enforcement pass deleted zero rows on a
    /// 19.5k-memory character; 30 days lets the usage bonuses — not just the
    /// LLM's one-shot importance guess — decide what stays.
    pub content_half_life_days: f64,
    /// Minimum fraction of the content score decay cannot erode past.
    pub content_floor: f64,
    /// Maximum bonus from reinforcement count (saturates via log2).
    pub max_reinforcement_bonus: f64,
    /// Per-reinforcement coefficient (applied to `log2(count + 1)`).
    pub reinforcement_coeff: f64,
    /// Maximum bonus from graph degree (related-memory links).
    pub max_graph_degree_bonus: f64,
    /// Per-link coefficient.
    pub graph_degree_coeff: f64,
    /// Bonus when `lastAccessedAt` is within the recency window.
    pub recent_access_bonus: f64,
    /// Recent-access window in days.
    pub recent_access_window_days: f64,
    /// Maximum contribution the content component alone can make. Without the
    /// cap an LLM-rated 0.7+ memory scores above the 0.5 threshold on content
    /// alone, so a memory must earn the remaining headroom from usage
    /// evidence — "LLM opinion is one input among several, not a final
    /// verdict."
    pub max_content_contribution: f64,
    /// Additive bonus for `kind: 'episodic'` rows. Episodic memories carry the
    /// exact statistical profile housekeeping deletes first yet are the
    /// hardest to regenerate — a one-off visit never recurs to be
    /// re-extracted — so the memory ROW should be the last to go.
    pub episodic_bonus: f64,
}

pub const DEFAULT_PROTECTION_CONFIG: ProtectionConfig = ProtectionConfig {
    content_half_life_days: 30.0,
    content_floor: 0.10,
    max_reinforcement_bonus: 0.25,
    reinforcement_coeff: 0.08,
    max_graph_degree_bonus: 0.10,
    graph_degree_coeff: 0.025,
    recent_access_bonus: 0.10,
    recent_access_window_days: 90.0,
    max_content_contribution: 0.40,
    episodic_bonus: 0.10,
};

/// The fields protection reads (v4 `ProtectionInput`). Times are epoch ms as
/// `new Date(x).getTime()` reads them (NaN when unparseable); an optional
/// stamp is `None` when absent or empty (JS-falsy).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProtectionInput<'a> {
    pub importance: f64,
    pub reinforced_importance: Option<f64>,
    pub created_at_ms: f64,
    pub last_reinforced_at_ms: Option<f64>,
    pub last_accessed_at_ms: Option<f64>,
    /// `reinforcementCount` — `None` for null / absent (`?? 1`).
    pub reinforcement_count: Option<f64>,
    /// `relatedMemoryIds?.length ?? 0`.
    pub graph_degree: usize,
    /// `kind` (`'episodic'` earns the bonus).
    pub kind: Option<&'a str>,
    /// `source` (`'MANUAL'` / `'CONSOLIDATED'` protect by source alone).
    pub source: Option<&'a str>,
}

/// v4 `ProtectionScoreResult`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProtectionScore {
    /// Final blended score in [0, 1].
    pub score: f64,
    pub content_component: f64,
    pub reinforcement_bonus: f64,
    pub graph_degree_bonus: f64,
    pub recent_access_bonus: f64,
    pub episodic_bonus: f64,
    pub days_since_ref_time: f64,
}

/// The blended protection score (v4 `calculateProtectionScore`, byte-identical
/// to the pre-refactor body). Time-decay reference is
/// `max(createdAt, lastReinforcedAt)`: passive retrieval does NOT reset the
/// clock, consistent with salience.
pub fn calculate_protection_score(
    m: &ProtectionInput,
    cfg: &ProtectionConfig,
    now_ms: f64,
) -> ProtectionScore {
    let base_importance = m.reinforced_importance.unwrap_or(m.importance);

    let reference = super::reference_time_ms(m.created_at_ms, m.last_reinforced_at_ms);
    let days_since_ref_time = math_max(0.0, (now_ms - reference) / DAY_MS);

    let decay = 0.5_f64.powf(days_since_ref_time / cfg.content_half_life_days);
    let content_component = math_min(
        cfg.max_content_contribution,
        base_importance * math_max(decay, cfg.content_floor),
    );

    let reinforcement_count = m.reinforcement_count.unwrap_or(1.0);
    let reinforcement_bonus = math_min(
        cfg.max_reinforcement_bonus,
        (reinforcement_count + 1.0).log2() * cfg.reinforcement_coeff,
    );

    let graph_degree_bonus = math_min(
        cfg.max_graph_degree_bonus,
        (m.graph_degree as f64) * cfg.graph_degree_coeff,
    );

    let mut recent_access_bonus = 0.0;
    if let Some(accessed) = m.last_accessed_at_ms {
        let days_since_access = (now_ms - accessed) / DAY_MS;
        if days_since_access < cfg.recent_access_window_days {
            recent_access_bonus = cfg.recent_access_bonus;
        }
    }

    // Episodic protection bump: one-off dated occurrences are the first thing
    // the retention rules would otherwise delete and the hardest to regenerate.
    let episodic_bonus = if m.kind == Some("episodic") {
        cfg.episodic_bonus
    } else {
        0.0
    };

    let score = math_min(
        1.0,
        content_component
            + reinforcement_bonus
            + graph_degree_bonus
            + recent_access_bonus
            + episodic_bonus,
    );

    ProtectionScore {
        score,
        content_component,
        reinforcement_bonus,
        graph_degree_bonus,
        recent_access_bonus,
        episodic_bonus,
        days_since_ref_time,
    }
}

/// v4 `ProtectionBreakdown` (`ProtectionScoreResult` + the two flags).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProtectionBreakdown {
    /// The blended score — with `score.score` reported as 1 when the row's
    /// source alone protects it (v4's `{ ...result, score: 1 }`).
    pub score: ProtectionScore,
    /// True for MANUAL and CONSOLIDATED rows, and for AUTO rows at or above
    /// [`PROTECTION_THRESHOLD`].
    pub protected: bool,
    /// True when the row's source alone protects it.
    pub protected_by_source: bool,
}

/// How much a memory is worth keeping (v4 `protection`). A hand-written
/// (MANUAL) row or a digest (CONSOLIDATED) is never policy's to remove: it
/// reports `protected` with score 1. Any other row — an absent source
/// included — is protected when its blended score reaches the threshold.
pub fn protection(m: &ProtectionInput, now_ms: f64, cfg: &ProtectionConfig) -> ProtectionBreakdown {
    let mut score = calculate_protection_score(m, cfg, now_ms);
    if matches!(m.source, Some("MANUAL") | Some("CONSOLIDATED")) {
        score.score = 1.0;
        return ProtectionBreakdown {
            score,
            protected: true,
            protected_by_source: true,
        };
    }
    ProtectionBreakdown {
        protected: score.score >= PROTECTION_THRESHOLD,
        score,
        protected_by_source: false,
    }
}

#[cfg(test)]
mod jest_twins {
    //! Case-for-case port of the `calculateProtectionScore()` and
    //! `DEFAULT_PROTECTION_CONFIG` describes of v4
    //! `__tests__/unit/lib/memory/scoring/protection-and-age.test.ts` (the pin
    //! `83334b9a5`). The file's `formatRelativeAge` describe is twinned in
    //! `memory_time::age_label`.
    //!
    //! v4's `makeMemory` fixture carries no `kind`, so `kind` is `None`; its
    //! `daysAgo(n)` builds an ISO string the formula re-parses, as here.
    //! jest's `toBeCloseTo(x, p)` passes when `|a − x| < 10^-p / 2`.

    use super::*;
    use crate::memory_scoring::{date_ms, truthy_date_ms};
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

    /// v4's `makeMemory(overrides)` — the fields the score reads.
    struct Mem {
        importance: f64,
        reinforced_importance: f64,
        created_at: String,
        last_reinforced_at: Option<String>,
        last_accessed_at: Option<String>,
        reinforcement_count: f64,
        related_memory_ids: Vec<&'static str>,
    }

    fn make_memory() -> Mem {
        Mem {
            importance: 0.5,
            reinforced_importance: 0.5,
            created_at: "2026-03-01T00:00:00.000Z".to_string(),
            last_reinforced_at: None,
            last_accessed_at: None,
            reinforcement_count: 1.0,
            related_memory_ids: vec![],
        }
    }

    fn score(m: &Mem) -> ProtectionScore {
        let input = ProtectionInput {
            importance: m.importance,
            reinforced_importance: Some(m.reinforced_importance),
            created_at_ms: date_ms(&m.created_at),
            last_reinforced_at_ms: truthy_date_ms(m.last_reinforced_at.as_deref()),
            last_accessed_at_ms: truthy_date_ms(m.last_accessed_at.as_deref()),
            reinforcement_count: Some(m.reinforcement_count),
            graph_degree: m.related_memory_ids.len(),
            kind: None,
            source: Some("AUTO"),
        };
        // v4 passes `undefined` for the config → the default.
        calculate_protection_score(&input, &DEFAULT_PROTECTION_CONFIG, now_ms())
    }

    const CFG: ProtectionConfig = DEFAULT_PROTECTION_CONFIG;

    /// `it('caps the content contribution of a fresh high-importance memory')`.
    #[test]
    fn caps_the_content_contribution_of_a_fresh_high_importance_memory() {
        let r = score(&Mem {
            importance: 0.8,
            reinforced_importance: 0.8,
            created_at: days_ago(0.0),
            ..make_memory()
        });
        assert!(close_to(
            r.content_component,
            CFG.max_content_contribution,
            3
        ));
        // content 0.40 + default reinforcement bonus 0.08 = 0.48 — below threshold
        assert!(r.score < 0.5);
    }

    /// `it('does not cap content when decay has brought it below the cap')`.
    #[test]
    fn does_not_cap_content_when_decay_has_brought_it_below_the_cap() {
        let r = score(&Mem {
            importance: 0.5,
            reinforced_importance: 0.5,
            created_at: days_ago(30.0),
            ..make_memory()
        });
        // decay at 30 days = 0.5, times 0.5 baseImportance = 0.25, below the 0.40 cap
        assert!(close_to(r.content_component, 0.25, 2));
    }

    /// `it('decays the content score over time with a 30-day half-life')`.
    #[test]
    fn decays_the_content_score_over_time_with_a_30_day_half_life() {
        let r = score(&Mem {
            importance: 0.8,
            reinforced_importance: 0.8,
            created_at: days_ago(30.0),
            ..make_memory()
        });
        // decay at 30 days = 0.5, times baseImportance 0.8 = 0.4, exactly at the cap
        assert!(close_to(r.content_component, 0.4, 2));
    }

    /// `it('floors the content decay at 10% of base importance')`.
    #[test]
    fn floors_the_content_decay_at_10_percent_of_base_importance() {
        let r = score(&Mem {
            importance: 0.8,
            reinforced_importance: 0.8,
            created_at: days_ago(3650.0), // 10 years
            ..make_memory()
        });
        // 10-year decay is ~1e-3 — floor kicks in at 0.10 * 0.8 = 0.08
        assert!(close_to(r.content_component, 0.08, 3));
    }

    /// `it('awards a reinforcement bonus that saturates')`.
    #[test]
    fn awards_a_reinforcement_bonus_that_saturates() {
        let saturated = score(&Mem {
            reinforcement_count: 1000.0,
            ..make_memory()
        });
        assert!(close_to(
            saturated.reinforcement_bonus,
            CFG.max_reinforcement_bonus,
            3
        ));
    }

    /// `it('awards a graph-degree bonus that saturates at four links')`.
    #[test]
    fn awards_a_graph_degree_bonus_that_saturates_at_four_links() {
        let r = score(&Mem {
            related_memory_ids: vec!["a", "b", "c", "d", "e", "f"],
            ..make_memory()
        });
        assert!(close_to(
            r.graph_degree_bonus,
            CFG.max_graph_degree_bonus,
            3
        ));
    }

    /// `it('adds a recent-access bonus when accessed within the recency window')`.
    #[test]
    fn adds_a_recent_access_bonus_when_accessed_within_the_recency_window() {
        let recently_accessed = score(&Mem {
            last_accessed_at: Some(days_ago(30.0)),
            ..make_memory()
        });
        assert_eq!(
            recently_accessed.recent_access_bonus,
            CFG.recent_access_bonus
        );

        let stale_access = score(&Mem {
            last_accessed_at: Some(days_ago(200.0)),
            ..make_memory()
        });
        assert_eq!(stale_access.recent_access_bonus, 0.0);
    }

    /// `it('sums capped components and never exceeds 1.0')`.
    #[test]
    fn sums_capped_components_and_never_exceeds_1_0() {
        let r = score(&Mem {
            importance: 1.0,
            reinforced_importance: 1.0,
            reinforcement_count: 1000.0,
            related_memory_ids: vec!["a", "b", "c", "d"],
            last_accessed_at: Some(days_ago(1.0)),
            ..make_memory()
        });
        assert!(r.score <= 1.0);
        assert!(close_to(
            r.score,
            CFG.max_content_contribution
                + CFG.max_reinforcement_bonus
                + CFG.max_graph_degree_bonus
                + CFG.recent_access_bonus,
            3
        ));
    }

    /// `it('uses max(createdAt, lastReinforcedAt) as the decay reference')`.
    #[test]
    fn uses_max_created_at_last_reinforced_at_as_the_decay_reference() {
        // Created a year ago but reinforced today — decay should be near zero,
        // giving a raw contentComponent of 0.8 that the cap then clips to 0.40.
        let r = score(&Mem {
            importance: 0.8,
            reinforced_importance: 0.8,
            created_at: days_ago(365.0),
            last_reinforced_at: Some(days_ago(0.0)),
            ..make_memory()
        });
        assert!(close_to(
            r.content_component,
            CFG.max_content_contribution,
            3
        ));
    }

    /// `it('drops a stale, low-importance, unused memory below a reasonable
    /// protection threshold')`.
    #[test]
    fn drops_a_stale_low_importance_unused_memory_below_a_reasonable_protection_threshold() {
        let r = score(&Mem {
            importance: 0.3,
            reinforced_importance: 0.3,
            created_at: days_ago(365.0),
            last_accessed_at: None,
            reinforcement_count: 1.0,
            related_memory_ids: vec![],
            ..make_memory()
        });
        assert!(r.score < 0.5);
    }

    /// `it('rescues a stale memory whose usage signals all fire')`.
    #[test]
    fn rescues_a_stale_memory_whose_usage_signals_all_fire() {
        let r = score(&Mem {
            importance: 0.2,
            reinforced_importance: 0.4,
            created_at: days_ago(365.0),
            last_reinforced_at: Some(days_ago(15.0)),
            last_accessed_at: Some(days_ago(10.0)),
            reinforcement_count: 12.0,
            related_memory_ids: vec!["a", "b", "c", "d"],
        });
        assert!(r.score >= 0.5);
    }

    /// `describe('DEFAULT_PROTECTION_CONFIG')` → `it('has expected default values')`.
    #[test]
    fn has_expected_default_values() {
        assert_eq!(CFG.content_half_life_days, 30.0);
        assert_eq!(CFG.content_floor, 0.10);
        assert_eq!(CFG.max_reinforcement_bonus, 0.25);
        assert_eq!(CFG.recent_access_window_days, 90.0);
        assert_eq!(CFG.max_content_contribution, 0.40);
    }
}
