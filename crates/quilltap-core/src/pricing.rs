//! Port of the pure cost arithmetic from v4's lib/llm/pricing.ts.
//!
//! Only [`estimate_cost`] is ported here — the dollar cost of a completion given
//! its prompt/completion token counts and the model's per-1M-token rates. The
//! sibling helpers in that module (`getAverageCostPer1M`, `sortByCost`,
//! `findCheapestModel`, `calculateCostTier`, `calculateSavings`, …) are equally
//! pure and can follow as their own oracle-checked units when the cheap-LLM
//! selection path is built; this unit keeps to the single function asked for.
//!
//! NB: in v4 `estimateCost` is defined but not yet enforced in the autonomous
//! run loop (the spend cap `budgetEstimatedSpendCapUSD` is carried but unchecked
//! — see the note in `enclave_budget`). It's ported now as ready arithmetic.

/// The pricing fields [`estimate_cost`] consults: USD cost per 1,000,000 tokens
/// for prompt (input) and completion (output) respectively. (v4's full
/// `ModelPricing` carries identity/metadata too, but the cost calc reads only
/// these two rates.)
#[derive(Clone, Copy, Debug)]
pub struct ModelCost {
    pub prompt_cost_per_1m: f64,
    pub completion_cost_per_1m: f64,
}

/// Estimated USD cost for a completion: each token bucket is priced as
/// `(tokens / 1_000_000) * ratePer1M`, prompt + completion summed.
///
/// The arithmetic mirrors v4 exactly — divide-then-multiply per bucket, in that
/// order, then add — so the IEEE-754 result is bit-identical. Token counts are
/// integral (well under 2^53, so exact as `f64`).
pub fn estimate_cost(pricing: &ModelCost, prompt_tokens: i64, completion_tokens: i64) -> f64 {
    let prompt_cost = (prompt_tokens as f64 / 1_000_000.0) * pricing.prompt_cost_per_1m;
    let completion_cost = (completion_tokens as f64 / 1_000_000.0) * pricing.completion_cost_per_1m;
    prompt_cost + completion_cost
}
