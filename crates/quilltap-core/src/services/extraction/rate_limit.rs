//! The extraction rate limit — port of v4
//! `lib/memory/extraction/rate-limit.ts` (`ac6348c5f`; pin `83334b9a5`): a
//! live counter.
//!
//! v4's why, carried: with `memoryExtractionLimits.enabled`, a character who
//! has gained `maxPerHour` memories in the last hour is skipped, and past
//! `softStartFraction` of it only candidates at or above `softFloor` are kept.
//! The count is the process's ledger of admissions it has seen land
//! ([`count_recent_admissions`], admission's), read once per character per
//! unit of work, plus every insert this unit of work makes as it lands — so
//! the last candidates of a long fold see the first ones, and a dry run counts
//! what it would insert. (Pre-refactor v5 counted `memories.createdAt` rows in
//! the last wall-clock hour; that read is gone.)
//!
//! The decision is taken PER HOLDER PER CALL by the pass runner, so a
//! throttled speaker's line appears once for its SELF call and once for its
//! OTHER call.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::db::instance_settings::MemoryExtractionLimits;
use crate::memory_extraction::parse::MemoryDraft;
use crate::services::admission::{count_recent_admissions, RATE_LEDGER_WINDOW_MS};

/// v4 `RateLimitDecision`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RateLimitDecision {
    Allow,
    Throttle {
        floor: f64,
        recent_count: f64,
        cap: f64,
    },
    Skip {
        recent_count: f64,
        cap: f64,
    },
}

impl RateLimitDecision {
    pub fn is_skip(&self) -> bool {
        matches!(self, Self::Skip { .. })
    }
}

/// The decision for a count under `limits` (v4 `rateLimitDecision`). Pure.
pub fn rate_limit_decision(
    recent_count: f64,
    limits: &MemoryExtractionLimits,
) -> RateLimitDecision {
    if !limits.enabled {
        return RateLimitDecision::Allow;
    }
    let cap = limits.max_per_hour as f64;
    if recent_count >= cap {
        return RateLimitDecision::Skip { recent_count, cap };
    }
    let soft_start = (cap * limits.soft_start_fraction).floor();
    if recent_count >= soft_start {
        return RateLimitDecision::Throttle {
            floor: limits.soft_floor,
            recent_count,
            cap,
        };
    }
    RateLimitDecision::Allow
}

/// Drafts at or above the importance floor (v4 `applyImportanceFloor`).
pub fn apply_importance_floor(drafts: &[MemoryDraft], floor: f64) -> Vec<MemoryDraft> {
    drafts
        .iter()
        .filter(|d| d.importance >= floor)
        .cloned()
        .collect()
}

/// A baseline reader: `(character_id, since_ms) -> Result<count, error
/// text>`. v4's `readBaseline` defaults to the ledger; tests pass their own.
pub type BaselineReader = dyn Fn(&str, f64) -> Result<f64, String> + Send + Sync;

#[derive(Default)]
struct LimiterState {
    /// characterId → the baseline read for this unit (v4 `baselines`).
    baselines: HashMap<String, f64>,
    /// characterId → inserts this unit landed (v4 `landed`).
    landed: HashMap<String, f64>,
}

/// A limiter for one unit of work (v4 `createRateLimiter`). The process
/// ledger's read never fails in v5, but the seam keeps v4's WARN arm for a
/// reader that does.
pub struct ExtractionRateLimiter {
    limits: MemoryExtractionLimits,
    read_baseline: Box<BaselineReader>,
    state: Mutex<LimiterState>,
}

impl ExtractionRateLimiter {
    /// The limiter over admission's process-global ledger.
    pub fn new(limits: MemoryExtractionLimits) -> Self {
        Self::with_baseline(
            limits,
            Box::new(|character_id, since_ms| {
                Ok(count_recent_admissions(
                    character_id,
                    since_ms,
                    crate::clock::now_unix_ms() as f64,
                ) as f64)
            }),
        )
    }

    /// The limiter over a caller's baseline reader (v4's `readBaseline`).
    pub fn with_baseline(
        limits: MemoryExtractionLimits,
        read_baseline: Box<BaselineReader>,
    ) -> Self {
        Self {
            limits,
            read_baseline,
            state: Mutex::new(LimiterState::default()),
        }
    }

    pub fn limits(&self) -> &MemoryExtractionLimits {
        &self.limits
    }

    fn baseline_for(&self, character_id: &str) -> f64 {
        if let Some(b) = self.state.lock().unwrap().baselines.get(character_id) {
            return *b;
        }
        let since = crate::clock::now_unix_ms() as f64 - RATE_LEDGER_WINDOW_MS;
        let baseline = match (self.read_baseline)(character_id, since) {
            Ok(n) => n,
            Err(error) => {
                tracing::warn!(
                    characterId = %character_id,
                    error = %error,
                    "[Memory] Rate-limit lookup failed; allowing extraction"
                );
                0.0
            }
        };
        self.state
            .lock()
            .unwrap()
            .baselines
            .insert(character_id.to_string(), baseline);
        baseline
    }

    /// What this character may admit now (v4 `decide`).
    pub fn decide(&self, character_id: &str) -> RateLimitDecision {
        if !self.limits.enabled {
            return RateLimitDecision::Allow;
        }
        let landed = self
            .state
            .lock()
            .unwrap()
            .landed
            .get(character_id)
            .copied()
            .unwrap_or(0.0);
        let recent = self.baseline_for(character_id) + landed;
        rate_limit_decision(recent, &self.limits)
    }

    /// Count inserts that just landed (or, in a dry run, would have) — v4
    /// `record(characterId, inserted = 1)`. Before the baseline is read, the
    /// ledger already holds these inserts.
    pub fn record(&self, character_id: &str, inserted: f64) {
        let mut state = self.state.lock().unwrap();
        if !self.limits.enabled || !state.baselines.contains_key(character_id) || inserted <= 0.0 {
            return;
        }
        *state.landed.entry(character_id.to_string()).or_insert(0.0) += inserted;
    }
}
