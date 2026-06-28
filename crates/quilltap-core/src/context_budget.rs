//! Port of the pure context-budget arithmetic from v4's
//! lib/llm/model-context-data.ts — the token-allocation math that decides how
//! much of a model's context window goes to each purpose and whether a
//! conversation needs summarizing.
//!
//! Resolving a model's raw context window (`getModelContextLimit`) is a
//! *separate* concern: it consults a static override table AND the plugin
//! provider-registry, so it belongs with the registry subsystem in a later
//! phase. Here the resolved `total_limit` / `model_context_limit` is injected at
//! the call boundary (the same seam used for `now`/`runStartedAt` in
//! `enclave_budget`); these functions are the pure arithmetic on top of it.
//!
//! `isCheapModel` / `estimateModelCost` from the sibling cheap-llm module are
//! likewise deferred — they read the registry's cheap-model config and a
//! fallback table before their keyword heuristics.

use crate::model_classes::get_model_class;

/// Default max context window when no profile/model info is available.
pub const DEFAULT_MAX_CONTEXT: i64 = 128000;
/// Default max output tokens when no profile/model info is available.
pub const DEFAULT_MAX_TOKENS: i64 = 8000;
/// Minimum floor for max_available to prevent degenerate cases.
pub const MIN_MAX_AVAILABLE: i64 = 4096;

/// Whether a conversation should be summarized: more than 60% context usage, or
/// more than 20 messages. (Usage is `estimated/limit*100`, a float compare.)
pub fn should_summarize_conversation(
    message_count: i64,
    estimated_tokens: i64,
    context_limit: i64,
) -> bool {
    let usage_percent = (estimated_tokens as f64 / context_limit as f64) * 100.0;
    if usage_percent > 60.0 {
        return true;
    }
    if message_count > 20 {
        return true;
    }
    false
}

/// How many recent messages to keep in full given the token budget: floor of
/// `available / average`, clamped to [4, 100].
///
/// The floor is a JS `Math.floor` (rounds toward −∞), not integer truncation —
/// it differs for negative `available_tokens`, so it's computed in `f64`. (The
/// clamp masks the difference here, but the floor is kept faithful regardless.)
pub fn calculate_recent_message_count(available_tokens: i64, average_message_tokens: i64) -> i64 {
    let count = (available_tokens as f64 / average_message_tokens as f64).floor() as i64;
    count.clamp(4, 100)
}

/// Resolve the effective max output tokens for a profile: an explicit positive
/// `max_tokens` wins; else the profile's model-class `maxOutput`; else the
/// default (8000).
pub fn resolve_max_tokens(profile_max_tokens: Option<i64>, model_class: Option<&str>) -> i64 {
    if let Some(mt) = profile_max_tokens {
        if mt > 0 {
            return mt;
        }
    }
    if let Some(mc_name) = model_class {
        // v4 guards on truthiness, so an empty model-class string is skipped.
        if !mc_name.is_empty() {
            if let Some(mc) = get_model_class(mc_name) {
                return mc.max_output;
            }
        }
    }
    DEFAULT_MAX_TOKENS
}

/// The result of [`calculate_max_available`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaxAvailable {
    pub max_available: i64,
    pub max_context: i64,
    pub max_tokens: i64,
}

/// Maximum available tokens for a prompt: `maxContext − 2·cappedMaxTokens`,
/// floored at [`MIN_MAX_AVAILABLE`]. `max_tokens` is capped so it never exceeds
/// 20% of `maxContext` (model classes often set output to the absolute ceiling,
/// which would make the budget negative).
///
/// `model_context_limit` is v4's `getModelContextLimit(provider, model)` result,
/// injected here (registry resolution is out of scope — see the module note). It
/// is only consulted when `profile_max_context` is absent or non-positive.
pub fn calculate_max_available(
    model_context_limit: i64,
    profile_max_context: Option<i64>,
    profile_max_tokens: Option<i64>,
    model_class: Option<&str>,
) -> MaxAvailable {
    let max_context = match profile_max_context {
        Some(c) if c > 0 => c,
        // v4: `getModelContextLimit(...) || DEFAULT_MAX_CONTEXT` — the `|| DEFAULT`
        // arm only fires if the limit is falsy (0), which it never is in practice.
        _ if model_context_limit != 0 => model_context_limit,
        _ => DEFAULT_MAX_CONTEXT,
    };

    let max_tokens = resolve_max_tokens(profile_max_tokens, model_class);

    let capped_max_tokens = max_tokens.min((max_context as f64 * 0.20).floor() as i64);
    let max_available = (max_context - 2 * capped_max_tokens).max(MIN_MAX_AVAILABLE);

    MaxAvailable {
        max_available,
        max_context,
        max_tokens: capped_max_tokens,
    }
}

/// Safe input context limit: `total − maxResponse − ceil(total·10%)`, floored at
/// 1000 (reserves the response plus a 10% safety buffer).
pub fn get_safe_input_limit(total_limit: i64, max_response_tokens: i64) -> i64 {
    let safety_buffer = (total_limit as f64 * 0.10).ceil() as i64;
    let safe_limit = total_limit - max_response_tokens - safety_buffer;
    safe_limit.max(1000)
}

/// Whether a model supports extended context (> 32768 tokens).
pub fn has_extended_context(total_limit: i64) -> bool {
    total_limit > 32768
}

/// Recommended per-purpose token allocations for a context window. The
/// percentage-scaled buckets carry minimum floors; `recent_messages` is a raw
/// fraction of the total (deliberately *not* floored in v4, so it stays `f64`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextAllocation {
    pub total_limit: i64,
    pub system_prompt: i64,
    pub memories: i64,
    pub knowledge: i64,
    pub conversation_summary: i64,
    pub recent_messages: f64,
    pub response_reserve: i64,
}

/// Compute the recommended allocations for a resolved `total_limit`. Mirrors v4
/// exactly: `Math.floor` on the percentage buckets (with floors), a tiered
/// `response_reserve`, and an un-floored `recent_messages` fraction.
pub fn get_recommended_context_allocation(total_limit: i64) -> ContextAllocation {
    let t = total_limit as f64;
    let system_prompt = 4000.max((t * 0.20).floor() as i64);
    let memories = 2000.max((t * 0.04).floor() as i64);
    let knowledge = 800.max((t * 0.02).floor() as i64);
    let conversation_summary = 1000.max((t * 0.02).floor() as i64);
    // v4 lists the >=100000 and >=32000 tiers separately but both yield 4096, so
    // they're collapsed here (clippy would flag the identical arms otherwise).
    let response_reserve = if total_limit >= 200000 {
        8192
    } else if total_limit >= 32000 {
        4096
    } else {
        2048
    };
    let recent_messages = if total_limit >= 200000 {
        t * 0.6
    } else if total_limit >= 100000 {
        t * 0.55
    } else if total_limit >= 32000 {
        t * 0.5
    } else {
        t * 0.4
    };

    ContextAllocation {
        total_limit,
        system_prompt,
        memories,
        knowledge,
        conversation_summary,
        recent_messages,
        response_reserve,
    }
}
