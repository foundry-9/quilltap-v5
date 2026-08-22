//! Port of v4's lib/llm/model-context-data.ts — the context-window lookup
//! (`getModelContextLimit`) and its two thin consumers (`hasExtendedContext`,
//! `getSafeInputLimit`).
//!
//! Registry seam: the lookup threads two registry calls (`getProvider(provider)
//! ?.getModelInfo()` and `getDefaultContextWindow(provider)`) and the pricing
//! module's `FALLBACK_PRICING` table through the middle of an otherwise pure
//! sequence. The function's own constant tables (`MODEL_CONTEXT_OVERRIDES`,
//! `DEFAULT_CONTEXT_BY_PROVIDER`) are ported as constants below; the
//! registry/pricing data is injected as parameters — the plugin model-info, the
//! provider's `FALLBACK_PRICING` rows, and the registry default. This mirrors v4
//! exactly while keeping the volatile data tables out of the core (the same
//! seam-injection pattern as `cheap_model` and `token_estimation`).

/// A plugin model-info entry, as far as the limit lookup reads it. `None`
/// context window is v4's `undefined`/falsy (skipped).
#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub id: String,
    pub context_window: Option<i64>,
}

/// A `FALLBACK_PRICING` row, as far as the limit lookup reads it. `None` context
/// length is v4's `contextLength: null` (falsy → skipped).
#[derive(Clone, Debug)]
pub struct PricingRow {
    pub model_id: String,
    pub context_length: Option<i64>,
}

/// v4's `MODEL_CONTEXT_OVERRIDES` — per-model context windows that win over
/// everything else. Exact-key match (also used for the `provider/model` form).
fn model_context_override(key: &str) -> Option<i64> {
    let v = match key {
        // Ollama models
        "llama3.2:3b" => 131072,
        "llama3.1:8b" => 131072,
        "llama3.1:70b" => 131072,
        "mistral:7b" => 32768,
        "mixtral:8x7b" => 32768,
        "codellama:7b" => 16384,
        "phi3:mini" => 4096,
        "qwen2:7b" => 32768,
        // OpenRouter-specific models
        "anthropic/claude-3-opus" => 200000,
        "anthropic/claude-3-sonnet" => 200000,
        "anthropic/claude-3-haiku" => 200000,
        "openai/gpt-4-turbo" => 128000,
        "openai/gpt-4" => 8192,
        "google/gemini-pro" => 1000000,
        // Older OpenAI models
        "gpt-4-0613" => 8192,
        "gpt-4-32k" => 32768,
        "gpt-3.5-turbo-16k" => 16385,
        _ => return None,
    };
    Some(v)
}

/// v4's `DEFAULT_CONTEXT_BY_PROVIDER` — conservative per-provider fallback.
fn default_context_by_provider(provider: &str) -> Option<i64> {
    let v = match provider {
        "ANTHROPIC" => 200000,
        "OPENAI" => 128000,
        "GOOGLE" => 1000000,
        "GROK" => 131072,
        "OLLAMA" => 8192,
        "OPENROUTER" => 128000,
        "OPENAI_COMPATIBLE" => 8192,
        _ => return None,
    };
    Some(v)
}

/// v4's substring matcher: `m.id === name || m.id.includes(name) ||
/// name.includes(m.id)`. ASCII model ids, so `includes` is byte-substring.
fn id_matches(candidate: &str, name: &str) -> bool {
    candidate == name || candidate.contains(name) || name.contains(candidate)
}

/// Context-window size for a model, reproducing v4's `getModelContextLimit`
/// lookup order: exact override → `provider/model` override → plugin model-info
/// (when injected) → `FALLBACK_PRICING` → registry default (when it differs from
/// 8192) → the hardcoded provider default (else 8192).
///
/// `model_info` / `fallback_pricing` are the injected registry/pricing rows;
/// `registry_default` is `getDefaultContextWindow(provider)`. A zero/`None`
/// context value at any data stage is falsy in v4 and falls through, matching
/// the `if (modelInfo?.contextWindow)` / `if (modelPricing?.contextLength)`
/// guards.
pub fn get_model_context_limit(
    provider: &str,
    model_name: &str,
    model_info: &[ModelInfo],
    fallback_pricing: &[PricingRow],
    registry_default: i64,
) -> i64 {
    // 1. explicit override
    if let Some(v) = model_context_override(model_name) {
        return v;
    }
    // 2. provider-prefixed override (OpenRouter form)
    let prefixed = format!("{}/{}", provider.to_lowercase(), model_name);
    if let Some(v) = model_context_override(&prefixed) {
        return v;
    }
    // 3. plugin model-info (first match wins; falsy contextWindow falls through)
    if let Some(mi) = model_info.iter().find(|m| id_matches(&m.id, model_name)) {
        if let Some(cw) = mi.context_window {
            if cw != 0 {
                return cw;
            }
        }
    }
    // 4. fallback pricing (first match wins; null/0 contextLength falls through)
    if let Some(mp) = fallback_pricing
        .iter()
        .find(|m| id_matches(&m.model_id, model_name))
    {
        if let Some(cl) = mp.context_length {
            if cl != 0 {
                return cl;
            }
        }
    }
    // 5. registry default, only when it isn't the registry's own 8192 sentinel
    if registry_default != 8192 {
        return registry_default;
    }
    // 6. hardcoded provider default, else 8192
    default_context_by_provider(provider).unwrap_or(8192)
}

/// Resolve the effective context window — v4's `resolveContextWindow`
/// (`lib/llm/model-context-data.ts:154`) in the lookup form: the profile's
/// user-set Max Context wins, the name lookup is the fallback, and a zero or
/// negative column falls through rather than producing a zero-token budget.
///
/// v4 keeps both halves in one module; v5 splits them (see this module's header),
/// so the arithmetic half lives in [`crate::context_budget::resolve_context_window`]
/// and this is the composition with [`get_model_context_limit`]. Callers holding a
/// profile must route through one of the two rather than calling the lookup bare,
/// or the budget and the compression trigger end up on two different windows
/// (v4 bug 70).
pub fn resolve_context_window(
    provider: &str,
    model_name: &str,
    model_info: &[ModelInfo],
    fallback_pricing: &[PricingRow],
    registry_default: i64,
    profile_max_context: Option<i64>,
) -> i64 {
    // The profile wins before the lookup runs at all (v4 returns early).
    if let Some(c) = profile_max_context {
        if c > 0 {
            return c;
        }
    }
    get_model_context_limit(
        provider,
        model_name,
        model_info,
        fallback_pricing,
        registry_default,
    )
}

/// Whether the model supports extended (> 32k) context — v4's
/// `hasExtendedContext`. Deliberately NOT profile-aware: v4's `f933ba9c` left
/// this one on the bare lookup.
pub fn has_extended_context(
    provider: &str,
    model_name: &str,
    model_info: &[ModelInfo],
    fallback_pricing: &[PricingRow],
    registry_default: i64,
) -> bool {
    get_model_context_limit(
        provider,
        model_name,
        model_info,
        fallback_pricing,
        registry_default,
    ) > 32768
}

/// Safe input-context limit: the resolved window minus the response reserve and
/// a 10% safety buffer (`ceil`), floored at 1000 — v4's `getSafeInputLimit`.
///
/// Since `f933ba9c` v4 composes this from `computeSafeInputLimit` over
/// `resolveContextWindow` instead of re-deriving the arithmetic, so the formula
/// has exactly one owner ([`crate::context_budget::compute_safe_input_limit`])
/// and the profile's Max Context governs here too.
pub fn get_safe_input_limit(
    provider: &str,
    model_name: &str,
    model_info: &[ModelInfo],
    fallback_pricing: &[PricingRow],
    registry_default: i64,
    max_response_tokens: i64,
    profile_max_context: Option<i64>,
) -> i64 {
    let total = resolve_context_window(
        provider,
        model_name,
        model_info,
        fallback_pricing,
        registry_default,
        profile_max_context,
    );
    crate::context_budget::compute_safe_input_limit(total, max_response_tokens)
}

#[cfg(test)]
mod nanogpt_legacy_table_guard {
    use super::*;
    use crate::provider_manifest::Registry;

    /// P4.D101 — NanoGPT is deliberately ABSENT from v4's legacy per-provider
    /// fallback tables, and must stay absent.
    ///
    /// MEASURED at v4 `4cb1035e`: `NANOGPT` appears in v4's entire `lib/` tree
    /// exactly ONCE — `EmbeddingProfileProviderEnum` in
    /// `lib/schemas/common.types.ts`. It is in NONE of
    /// `DEFAULT_CONTEXT_BY_PROVIDER`, `LEGACY_RECOMMENDED_CHEAP_MODELS`,
    /// `PROVIDER_NAME_SUPPORT`, `PROVIDER_ATTACHMENT_CAPABILITIES`, or the
    /// pricing fallback data. Those tables are v4's PRE-plugin fallbacks; a
    /// plugin-era provider is served by the registry instead, and `Provider` is
    /// `z.string()` so no type forces an entry.
    ///
    /// Adding a NANOGPT row to any of them would be a v5-invented divergence.
    /// This test is the tripwire: it pins the absence AND proves the registry
    /// path still answers 131072, so the absence costs nothing.
    #[test]
    fn nanogpt_is_absent_from_the_legacy_table_but_served_by_the_registry() {
        assert_eq!(
            default_context_by_provider("NANOGPT"),
            None,
            "NANOGPT must NOT be in the legacy DEFAULT_CONTEXT_BY_PROVIDER table — \
             v4 has no such row (measured at 4cb1035e)"
        );

        // The manifest carries the real value, and the registry is consulted
        // first, so the legacy table is never reached for NanoGPT.
        let manifest = Registry::built_in().get_provider("NANOGPT").unwrap();
        assert_eq!(manifest.default_context_window, 131072);
        assert_eq!(
            resolve_context_window("NANOGPT", "openai/gpt-5-mini", &[], &[], 131072, None),
            131072
        );
    }
}
