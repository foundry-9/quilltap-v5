//! The memory-pool sizer — v4 `sizeMemoryPools`, its constants and
//! `PROACTIVE_RECALL_POOL_SIZE` (v4 `3f7320138` / `d58548051`; all in v4 Phase
//! E's `lib/memory/recall/assemble.ts:72-160` since the recall rewrite).
//!
//! Splits a turn's memory budget (`max(2000, 4% of context)` — about 8k tokens
//! on a 200k model) between the per-turn dynamic head and the frozen archive,
//! where the fixed 200-token / 5-entry head had left most of it unspent:
//!
//! - head tokens   = clamp(15% of memoryBudget, 200, 1200), bounded by the budget
//! - head entries  = clamp(round(headTokens / 40), 5, 15)
//! - retrospective = 2× both (floored at the historical retro sizes), tokens
//!   bounded by the budget
//! - archive size  = clamp(round((memoryBudget − ORDINARY head tokens) / 60),
//!   25, 60) — from the ordinary head even on a retrospective turn, so the
//!   archive's membership (and its bytes) stay stable across turns.
//!
//! Pure; tier-1 exact against v4's REAL export (`memory_pools_equivalence`).
//! Consumed by P4.D268 at every buildContext / pre-compute site and by
//! P4.D267's replay (this round, §R.14), and by the recall assembler. The four
//! FLOORS live here too since P4.D291 (they left the deleted injector's memory
//! formatters with v4 Phase E). JS `Math.round`
//! is [`crate::jsnum::math_round`]; every v4 caller passes a FINITE budget (the
//! replay floors a finite positive `memoryBudget`; the context builders compute
//! one from integer token counts), so `NaN` never reaches the sizer.

use crate::jsnum::math_round;

/// Phase 3b: hard cap on the dynamic-head rank instruction.
pub const DYNAMIC_HEAD_TOKEN_BUDGET: i64 = 200;
/// Phase 3b: how many rank-instruction entries to attempt.
pub const DYNAMIC_HEAD_DEFAULT_SIZE: usize = 5;

/// Recall-on-reference (episodic overhaul): a retrospective turn — the user is
/// invoking past shared events — gets an enlarged head, because that is
/// exactly the turn that needs rich recall. Tuning knobs; validate against
/// real chats via the recall-replay harness before tightening.
pub const RETRO_HEAD_TOKEN_BUDGET: i64 = 600;
/// Enlarged entry count for retrospective turns.
pub const RETRO_HEAD_SIZE: usize = 10;

/// Share of the memory budget the per-turn relevance head may spend.
pub const DYNAMIC_HEAD_BUDGET_RATIO: f64 = 0.15;
/// Ceiling on the ordinary dynamic-head token budget.
pub const DYNAMIC_HEAD_MAX_TOKEN_BUDGET: i64 = 1200;
/// Ceiling on the ordinary dynamic-head entry count.
pub const DYNAMIC_HEAD_MAX_SIZE: usize = 15;
/// Rough tokens per head line (id tag + age + summary + meta), for sizing.
/// Module-private in v4 (observed only through [`size_memory_pools`]).
pub(crate) const HEAD_TOKENS_PER_ENTRY: f64 = 40.0;
/// Floor on the frozen-archive entry count (the historical fixed size).
pub const FROZEN_ARCHIVE_MIN_SIZE: usize = 25;
/// Ceiling on the frozen-archive entry count.
pub const FROZEN_ARCHIVE_MAX_SIZE: usize = 60;
/// Rough tokens per archive line (summary + meta), for sizing. Module-private
/// in v4.
pub(crate) const ARCHIVE_TOKENS_PER_ENTRY: f64 = 60.0;

/// v4 `PROACTIVE_RECALL_POOL_SIZE` (`assemble.ts:90`): the archive
/// overlap (≤ [`FROZEN_ARCHIVE_MAX_SIZE`]) plus the largest retrospective head
/// — 90, computed from the constants as v4 does.
pub const PROACTIVE_RECALL_POOL_SIZE: usize = FROZEN_ARCHIVE_MAX_SIZE + DYNAMIC_HEAD_MAX_SIZE * 2;

/// v4 `MemoryPoolSizing`, field for field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryPoolSizing {
    /// Token cap for this turn's dynamic head.
    pub head_token_budget: i64,
    /// Entry cap for this turn's dynamic head.
    pub head_entries: usize,
    /// Token cap for formatting the frozen archive this turn.
    pub archive_token_budget: i64,
    /// How many memories the frozen archive holds (from the ORDINARY head).
    pub archive_size: usize,
}

/// v4's private `clamp(value, min, max)` = `Math.min(max, Math.max(min, value))`.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    max.min(min.max(value))
}

/// v4 `sizeMemoryPools(memoryBudget, retrospective)` (`memory-injector.ts:103-134`).
pub fn size_memory_pools(memory_budget: f64, retro: bool) -> MemoryPoolSizing {
    let budget = 0f64.max(memory_budget.floor());
    let base_head_tokens = budget.min(clamp(
        math_round(budget * DYNAMIC_HEAD_BUDGET_RATIO),
        DYNAMIC_HEAD_TOKEN_BUDGET as f64,
        DYNAMIC_HEAD_MAX_TOKEN_BUDGET as f64,
    ));
    let base_head_entries = clamp(
        math_round(base_head_tokens / HEAD_TOKENS_PER_ENTRY),
        DYNAMIC_HEAD_DEFAULT_SIZE as f64,
        DYNAMIC_HEAD_MAX_SIZE as f64,
    );

    let head_token_budget = if retro {
        budget.min((RETRO_HEAD_TOKEN_BUDGET as f64).max(base_head_tokens * 2.0))
    } else {
        base_head_tokens
    };
    let head_entries = if retro {
        (RETRO_HEAD_SIZE as f64).max(base_head_entries * 2.0)
    } else {
        base_head_entries
    };

    let archive_size = clamp(
        math_round(0f64.max(budget - base_head_tokens) / ARCHIVE_TOKENS_PER_ENTRY),
        FROZEN_ARCHIVE_MIN_SIZE as f64,
        FROZEN_ARCHIVE_MAX_SIZE as f64,
    );

    MemoryPoolSizing {
        head_token_budget: head_token_budget as i64,
        head_entries: head_entries as usize,
        archive_token_budget: 0f64.max(budget - head_token_budget) as i64,
        archive_size: archive_size as usize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The survey's two anchor points at the 2,000 floor.
    #[test]
    fn the_two_thousand_floor() {
        assert_eq!(
            size_memory_pools(2000.0, false),
            MemoryPoolSizing {
                head_token_budget: 300,
                head_entries: 8,
                archive_token_budget: 1700,
                archive_size: 28,
            }
        );
        assert_eq!(
            size_memory_pools(2000.0, true),
            MemoryPoolSizing {
                head_token_budget: 600,
                head_entries: 16,
                archive_token_budget: 1400,
                archive_size: 28,
            }
        );
        assert_eq!(PROACTIVE_RECALL_POOL_SIZE, 90);
    }
}
