//! Port of v4 `lib/memory/scoring/importance.ts` — importance filters and
//! bands.
//!
//! v4's why, carried: every IN-MEMORY `minImportance` filter compares
//! `reinforcedImportance` (falling back to `importance` for a row that has
//! none), and the per-path values collapse to two named constants: every
//! prompt path recalls at [`RECALL_MIN_IMPORTANCE`]; the search tool and the
//! Memories API at [`TOOL_MIN_IMPORTANCE`].
//!
//! ⚠ v4's two SQL filters compare the `reinforcedImportance` COLUMN and
//! EXCLUDE a NULL — [`filter_importance`] is NOT their replacement (the
//! repository's, P4.D288).

/// The importance floor for every path that puts memories in a prompt
/// (head, proactive, inter-character, Carina, voice-rewrite, replay).
pub const RECALL_MIN_IMPORTANCE: f64 = 0.3;

/// The importance floor for the search tool and the Memories API: none.
pub const TOOL_MIN_IMPORTANCE: f64 = 0.0;

/// At or above this a memory is in the high band.
pub const IMPORTANCE_HIGH: f64 = 0.7;
/// Below this a memory is in the low band.
pub const IMPORTANCE_LOW: f64 = 0.3;

/// v4 `ImportanceBand`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportanceBand {
    High,
    Medium,
    Low,
}

impl ImportanceBand {
    /// v4's wire string: `"high"` / `"medium"` / `"low"`.
    pub fn as_str(self) -> &'static str {
        match self {
            ImportanceBand::High => "high",
            ImportanceBand::Medium => "medium",
            ImportanceBand::Low => "low",
        }
    }
}

/// The value every in-memory importance filter and band reads:
/// `reinforcedImportance ?? importance`.
pub fn filter_importance(importance: f64, reinforced_importance: Option<f64>) -> f64 {
    reinforced_importance.unwrap_or(importance)
}

/// The band of an importance value: high ≥ 0.7, low < 0.3, medium between
/// (a NaN fails both comparisons and reads medium, as v4's does).
pub fn importance_band(value: f64) -> ImportanceBand {
    if value >= IMPORTANCE_HIGH {
        ImportanceBand::High
    } else if value < IMPORTANCE_LOW {
        ImportanceBand::Low
    } else {
        ImportanceBand::Medium
    }
}
