//! Port of the pure cost arithmetic from v4's lib/llm/pricing.ts.
//!
//! [`estimate_cost`] is the dollar cost of a completion given its
//! prompt/completion token counts and the model's per-1M-token rates. The
//! cost-aware model-selection siblings — [`get_average_cost_per_1m`],
//! [`sort_by_cost`], [`find_cheapest_model`], [`get_models_under_cost`],
//! [`calculate_cost_tier`], [`calculate_savings`] — operate over a fuller
//! [`ModelPricing`] row.
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

/// A model-pricing row, holding the fields the selection helpers consult. (v4's
/// `ModelPricing` carries identity/provider/fetch metadata too; the cost-aware
/// helpers read only the id, the two rates, the context window, and the two
/// capability flags.)
#[derive(Clone, Debug, PartialEq)]
pub struct ModelPricing {
    pub model_id: String,
    pub prompt_cost_per_1m: f64,
    pub completion_cost_per_1m: f64,
    /// `None` mirrors v4's `contextLength: null` (treated as "fits any window").
    pub context_length: Option<i64>,
    pub supports_vision: bool,
    pub supports_tools: bool,
}

/// The average cost per 1M tokens for a model — the simple mean of the input and
/// output rates. This is the sort/tier/threshold key used throughout selection.
pub fn get_average_cost_per_1m(pricing: &ModelPricing) -> f64 {
    (pricing.prompt_cost_per_1m + pricing.completion_cost_per_1m) / 2.0
}

/// Sort models by average cost, cheapest first. Returns a fresh `Vec` (v4 copies
/// before sorting); the sort is **stable**, matching JS `Array.prototype.sort`,
/// so models of equal average cost keep their input order.
pub fn sort_by_cost(models: &[ModelPricing]) -> Vec<ModelPricing> {
    let mut out = models.to_vec();
    out.sort_by(|a, b| {
        let ca = get_average_cost_per_1m(a);
        let cb = get_average_cost_per_1m(b);
        // Costs are finite, so partial_cmp never yields None here; Equal on a tie
        // leaves the stable sort to preserve input order (v4's `costA - costB`).
        ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Optional requirements for [`find_cheapest_model`].
#[derive(Clone, Copy, Debug, Default)]
pub struct FindCheapestOptions {
    pub require_vision: bool,
    pub require_tools: bool,
    pub min_context_length: Option<i64>,
}

/// Find the cheapest model satisfying the optional requirements, or `None` when
/// nothing qualifies.
///
/// Subtlety preserved from v4: the `minContextLength` filter is applied via JS
/// truthiness — `if (options?.minContextLength)` — so a value of `0` is *falsy*
/// and the filter is **skipped** (treated as "no minimum"), exactly like an
/// absent option. A model whose `context_length` is `None` always passes the
/// minimum (an unknown window is assumed large enough).
pub fn find_cheapest_model(
    models: &[ModelPricing],
    options: FindCheapestOptions,
) -> Option<ModelPricing> {
    let mut candidates: Vec<ModelPricing> = models.to_vec();
    if options.require_vision {
        candidates.retain(|m| m.supports_vision);
    }
    if options.require_tools {
        candidates.retain(|m| m.supports_tools);
    }
    // JS-truthiness on minContextLength: only filter when present AND non-zero.
    if let Some(min) = options.min_context_length {
        if min != 0 {
            candidates.retain(|m| m.context_length.is_none_or(|c| c >= min));
        }
    }
    if candidates.is_empty() {
        return None;
    }
    sort_by_cost(&candidates).into_iter().next()
}

/// All models whose average cost is at or below the threshold, in input order
/// (v4 filters the list as-is; it does not sort).
pub fn get_models_under_cost(
    models: &[ModelPricing],
    max_average_cost_per_1m: f64,
) -> Vec<ModelPricing> {
    models
        .iter()
        .filter(|m| get_average_cost_per_1m(m) <= max_average_cost_per_1m)
        .cloned()
        .collect()
}

/// Map a model's average cost to a coarse tier 1–5. (v4 has a leading `=== 0`
/// arm that also returns 1; it's subsumed by `< 0.5` since `0 < 0.5`, so it's
/// collapsed here — same result.) Thresholds: free/very-cheap → 1, `< 2.0` → 2,
/// `< 10.0` → 3, `< 50.0` → 4, else 5.
pub fn calculate_cost_tier(pricing: &ModelPricing) -> i64 {
    let avg = get_average_cost_per_1m(pricing);
    if avg < 0.5 {
        1
    } else if avg < 2.0 {
        2
    } else if avg < 10.0 {
        3
    } else if avg < 50.0 {
        4
    } else {
        5
    }
}

/// Cost savings of `cheaper` relative to `expensive`, as a percentage. Returns
/// `0.0` when the expensive model is free (avoids dividing by zero).
pub fn calculate_savings(expensive_model: &ModelPricing, cheaper_model: &ModelPricing) -> f64 {
    let expensive_cost = get_average_cost_per_1m(expensive_model);
    let cheaper_cost = get_average_cost_per_1m(cheaper_model);
    if expensive_cost == 0.0 {
        return 0.0;
    }
    ((expensive_cost - cheaper_cost) / expensive_cost) * 100.0
}
