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

/// Whether the model supports extended (> 32k) context — v4's
/// `hasExtendedContext`.
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

/// Safe input-context limit: the total window minus the response reserve and a
/// 10% safety buffer (`ceil`), floored at 1000 — v4's `getSafeInputLimit`.
pub fn get_safe_input_limit(
    provider: &str,
    model_name: &str,
    model_info: &[ModelInfo],
    fallback_pricing: &[PricingRow],
    registry_default: i64,
    max_response_tokens: i64,
) -> i64 {
    let total = get_model_context_limit(
        provider,
        model_name,
        model_info,
        fallback_pricing,
        registry_default,
    );
    // Math.ceil(total * 0.10) — total is integral and well under 2^53, so the
    // f64 product is exact and the ceil is unambiguous.
    let safety_buffer = ((total as f64) * 0.10).ceil() as i64;
    let safe = total - max_response_tokens - safety_buffer;
    safe.max(1000)
}
