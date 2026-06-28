//! Port of lib/memory/memory-weighting.ts — the pure scoring functions.
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

#[derive(Clone, Copy)]
pub struct WeightingConfig {
    pub half_life_days: f64,
    pub importance_floor: f64,
    pub min_weight_threshold: f64,
}

pub const DEFAULT_WEIGHTING_CONFIG: WeightingConfig = WeightingConfig {
    half_life_days: 30.0,
    importance_floor: 0.70,
    min_weight_threshold: 0.05,
};

#[derive(Clone, Copy)]
pub struct ProtectionConfig {
    pub content_half_life_days: f64,
    pub content_floor: f64,
    pub max_reinforcement_bonus: f64,
    pub reinforcement_coeff: f64,
    pub max_graph_degree_bonus: f64,
    pub graph_degree_coeff: f64,
    pub recent_access_bonus: f64,
    pub recent_access_window_days: f64,
    pub max_content_contribution: f64,
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
};

/// The subset of a Memory the scoring functions read. Times are epoch millis
/// (the harness converts ISO strings → millis the same way `new Date(x)` does).
#[derive(Clone, Copy, Default)]
pub struct MemoryInputs {
    pub importance: f64,
    pub reinforced_importance: Option<f64>,
    pub created_at_ms: f64,
    pub last_reinforced_at_ms: Option<f64>,
    pub last_accessed_at_ms: Option<f64>,
    pub reinforcement_count: Option<u64>,
    pub graph_degree: usize, // relatedMemoryIds.length
}

pub struct EffectiveWeight {
    pub effective_weight: f64,
    pub raw_weight: f64,
    pub min_weight: f64,
    pub time_decay_factor: f64,
    pub days_old: f64,
    pub base_importance: f64,
}

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

pub struct ProtectionScore {
    pub score: f64,
    pub content_component: f64,
    pub reinforcement_bonus: f64,
    pub graph_degree_bonus: f64,
    pub recent_access_bonus: f64,
    pub days_since_ref_time: f64,
}

pub fn calculate_protection_score(
    m: &MemoryInputs,
    cfg: &ProtectionConfig,
    now_ms: f64,
) -> ProtectionScore {
    let base_importance = m.reinforced_importance.unwrap_or(m.importance);
    let reinforced = m.last_reinforced_at_ms.unwrap_or(0.0);
    let reference = m.created_at_ms.max(reinforced);
    let days_since_ref_time = ((now_ms - reference) / MS_PER_DAY).max(0.0);

    let decay = 0.5_f64.powf(days_since_ref_time / cfg.content_half_life_days);
    let content_component = cfg
        .max_content_contribution
        .min(base_importance * decay.max(cfg.content_floor));

    let reinforcement_count = m.reinforcement_count.unwrap_or(1);
    let reinforcement_bonus = cfg.max_reinforcement_bonus.min(
        ((reinforcement_count as f64) + 1.0).log2() * cfg.reinforcement_coeff,
    );

    let graph_degree_bonus = cfg
        .max_graph_degree_bonus
        .min((m.graph_degree as f64) * cfg.graph_degree_coeff);

    let mut recent_access_bonus = 0.0;
    if let Some(accessed) = m.last_accessed_at_ms {
        let days_since_access = (now_ms - accessed) / MS_PER_DAY;
        if days_since_access < cfg.recent_access_window_days {
            recent_access_bonus = cfg.recent_access_bonus;
        }
    }

    let score = 1.0_f64
        .min(content_component + reinforcement_bonus + graph_degree_bonus + recent_access_bonus);

    ProtectionScore {
        score,
        content_component,
        reinforcement_bonus,
        graph_degree_bonus,
        recent_access_bonus,
        days_since_ref_time,
    }
}

// ---------------------------------------------------------------------------
// Retrieval ranking blend + relevance floors (ports of the same TS file).
// ---------------------------------------------------------------------------

pub const RANKING_RELEVANCE_WEIGHT: f64 = 0.75;
pub const RANKING_PRIORITY_WEIGHT: f64 = 0.25;

/// Blend cosine relevance (primary) with decaying priority (`rawWeight`, no
/// floor). Mirrors `computeRankingBlend`.
pub fn compute_ranking_blend(cosine: f64, raw_weight: f64) -> f64 {
    RANKING_RELEVANCE_WEIGHT * cosine + RANKING_PRIORITY_WEIGHT * raw_weight
}

pub const DEFAULT_MIN_COSINE_NEURAL: f64 = 0.30;
pub const DEFAULT_MIN_COSINE_TFIDF: f64 = 0.10;

/// Provider-aware relevance floor. `BUILTIN` is the local TF-IDF provider;
/// everything else (incl. None) is treated as neural. Mirrors
/// `defaultMinCosineForProvider`.
pub fn default_min_cosine_for_provider(provider: Option<&str>) -> f64 {
    if provider == Some("BUILTIN") {
        DEFAULT_MIN_COSINE_TFIDF
    } else {
        DEFAULT_MIN_COSINE_NEURAL
    }
}

/// Human-readable relative age. Port of `formatRelativeAge`. Reference time is
/// max(createdAt, lastReinforcedAt); `days_old` is clamped at 0. Branch
/// boundaries and `Math.floor` semantics match the TS exactly (note JS
/// `Math.floor` on a non-negative f64 == Rust `.floor()`; the year branch
/// pluralizes only when floor(years) > 1).
pub fn format_relative_age(m: &MemoryInputs, now_ms: f64) -> String {
    let reinforced = m.last_reinforced_at_ms.unwrap_or(0.0);
    let reference = m.created_at_ms.max(reinforced);
    let days_old = ((now_ms - reference) / MS_PER_DAY).max(0.0);

    if days_old < 1.0 {
        "today".to_string()
    } else if days_old < 2.0 {
        "yesterday".to_string()
    } else if days_old < 7.0 {
        format!("{} days ago", days_old.floor() as i64)
    } else if days_old < 14.0 {
        "last week".to_string()
    } else if days_old < 30.0 {
        format!("{} weeks ago", (days_old / 7.0).floor() as i64)
    } else if days_old < 60.0 {
        "last month".to_string()
    } else if days_old < 365.0 {
        format!("{} months ago", (days_old / 30.0).floor() as i64)
    } else {
        let years = (days_old / 365.0).floor() as i64;
        format!("{} year{} ago", years, if years > 1 { "s" } else { "" })
    }
}
