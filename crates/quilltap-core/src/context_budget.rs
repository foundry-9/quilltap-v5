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
/// Fraction of the context window held back beyond the response reserve, to
/// absorb the gap between the character-based token estimates and the model's
/// real tokenizer (v4 `CONTEXT_SAFETY_MARGIN_RATIO`).
pub const CONTEXT_SAFETY_MARGIN_RATIO: f64 = 0.10;

/// Resolve the effective context window for a request — v4's
/// `resolveContextWindow` (`lib/llm/model-context-data.ts:154`) in this module's
/// injected-limit form.
///
/// **This is the single source of truth for "how big is this model's window".**
/// The profile's user-set Max Context wins over the table lookup: the tables are
/// keyed by model name and go stale the moment someone points Ollama or an
/// OpenAI-compatible endpoint at a model the table has never heard of, at which
/// point the lookup silently returns a conservative provider default (8192 for
/// OLLAMA / OPENAI_COMPATIBLE). Budgeting a 64k model as 8k truncates
/// conversation history on every turn (v4 bug 70).
///
/// A zero or negative `profile_max_context` falls through to the lookup rather
/// than producing a zero-token budget. `model_context_limit` is v4's
/// `getModelContextLimit(provider, model)` result, injected at the boundary
/// (registry resolution is out of scope — see the module note); the lookup form
/// that composes both halves is [`crate::model_context::resolve_context_window`].
pub fn resolve_context_window(model_context_limit: i64, profile_max_context: Option<i64>) -> i64 {
    match profile_max_context {
        Some(c) if c > 0 => c,
        _ => model_context_limit,
    }
}

/// The ceiling the outgoing payload must fit under: window, less the response
/// reserve, less the safety margin — v4's `computeSafeInputLimit` (:188), floored
/// at 1000.
///
/// **Everything that packs the payload and everything that validates it must use
/// this same number.** They used not to: the context builder filled to
/// `total_limit − response_reserve` while the pre-send check warned above
/// `total_limit − response_reserve − 10%`, so the builder always aimed past the
/// line the validator drew and a full context reported an overage it had been
/// told to produce.
pub fn compute_safe_input_limit(total_limit: i64, response_reserve: i64) -> i64 {
    let safety_margin = (total_limit as f64 * CONTEXT_SAFETY_MARGIN_RATIO).ceil() as i64;
    (total_limit - response_reserve - safety_margin).max(1000)
}

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
    // v4 `f933ba9c`: `resolveContextWindow(...) || DEFAULT_MAX_CONTEXT` — the
    // `|| DEFAULT` arm only fires if the resolved window is falsy (0), which it
    // never is in practice. The profile-first order this function already had is
    // now the shared `resolve_context_window` (v4 converged on it here).
    let max_context = match resolve_context_window(model_context_limit, profile_max_context) {
        0 => DEFAULT_MAX_CONTEXT,
        c => c,
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

/// Safe input context limit for a resolved window — v4's `getSafeInputLimit`
/// (:205) in injected-limit form: [`compute_safe_input_limit`] over
/// [`resolve_context_window`], so the profile's Max Context governs here too.
pub fn get_safe_input_limit(
    model_context_limit: i64,
    max_response_tokens: i64,
    profile_max_context: Option<i64>,
) -> i64 {
    compute_safe_input_limit(
        resolve_context_window(model_context_limit, profile_max_context),
        max_response_tokens,
    )
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
    /// Tokens held back beyond the response reserve to absorb estimator error
    /// (10% of the window). Part of `safe_input_limit`; broken out for logging.
    pub safety_margin: i64,
    /// The ceiling the outgoing payload must fit under:
    /// `total_limit − response_reserve − safety_margin`.
    pub safe_input_limit: i64,
}

/// Compute the recommended allocations for a window — v4's
/// `getRecommendedContextAllocation`. The window is resolved through
/// [`resolve_context_window`], so the profile's Max Context wins over the
/// injected model-name lookup (v4 bug 70: budgeting from the lookup alone
/// trimmed history to an 8192-token window a 64k profile never had).
///
/// Mirrors v4 exactly otherwise: `Math.floor` on the percentage buckets (with
/// floors), a tiered `response_reserve`, and an un-floored `recent_messages`
/// fraction.
pub fn get_recommended_context_allocation(
    model_context_limit: i64,
    profile_max_context: Option<i64>,
) -> ContextAllocation {
    let total_limit = resolve_context_window(model_context_limit, profile_max_context);
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
        safety_margin: (t * CONTEXT_SAFETY_MARGIN_RATIO).ceil() as i64,
        safe_input_limit: compute_safe_input_limit(total_limit, response_reserve),
    }
}
