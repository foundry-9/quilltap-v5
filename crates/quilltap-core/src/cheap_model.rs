//! Port of v4's cheap-model classifiers from lib/llm/cheap-llm.ts (`isCheapModel`,
//! `estimateModelCost`, `getCheapestModel`), plus the deprecated fallback tables
//! they consult from lib/llm/fallback-data.ts.
//!
//! Registry seam: each function first consults the plugin registry
//! (`getCheapModelConfig`) and falls back to the hardcoded tables here. The
//! registry-sourced values are injected as parameters (an established pattern in
//! this port — cf. `token_estimation`'s `chars_per_token`): the recommended-model
//! list for [`is_cheap_model`] / [`estimate_model_cost`], and the default model
//! for [`get_cheapest_model`]. Pass an empty slice / `None` to take the fallback
//! path, which is exactly what the differential oracle exercises (the registry
//! returns no cheap config in a bare run). The string heuristics are pure.

/// v4's `LEGACY_RECOMMENDED_CHEAP_MODELS[provider]` — models known to work for
/// cheap-LLM tasks. Empty slice for an unknown provider (JS `?? []`).
fn recommended_cheap_models(provider: &str) -> &'static [&'static str] {
    match provider {
        "ANTHROPIC" => &["claude-haiku-4-5-20251001", "claude-3-haiku-20240307"],
        "OPENAI" => &["gpt-4o-mini", "gpt-3.5-turbo"],
        "GOOGLE" => &["gemini-2.0-flash", "gemini-1.5-flash"],
        "GROK" => &["grok-2-mini"],
        "OPENROUTER" => &[
            "openai/gpt-4o-mini",
            "anthropic/claude-3-haiku",
            "google/gemini-2.0-flash",
            "mistralai/mistral-7b-instruct",
        ],
        "OLLAMA" => &[
            "llama3.2:3b",
            "llama3.2:1b",
            "phi3:mini",
            "mistral:7b",
            "gemma2:2b",
        ],
        "OPENAI_COMPATIBLE" => &["gpt-4o-mini", "gpt-3.5-turbo"],
        _ => &[],
    }
}

/// v4's `LEGACY_CHEAPEST_MODEL_MAP[provider]` — the single cheapest model per
/// provider. `None` for an unknown provider (JS would yield `undefined`).
fn legacy_cheapest_model(provider: &str) -> Option<&'static str> {
    match provider {
        "ANTHROPIC" => Some("claude-haiku-4-5-20251001"),
        "OPENAI" => Some("gpt-4o-mini"),
        "GOOGLE" => Some("gemini-2.0-flash"),
        "GROK" => Some("grok-2-mini"),
        "OPENROUTER" => Some("openai/gpt-4o-mini"),
        "OLLAMA" => Some("llama3.2:3b"),
        "OPENAI_COMPATIBLE" => Some("gpt-4o-mini"),
        _ => None,
    }
}

/// Whether `model_name` is considered a "cheap" model. `registry_recommended` is
/// the registry's recommended-model list (empty → use the fallback table).
///
/// Order matters and mirrors v4 exactly: an exact match against the resolved
/// recommended list wins first; then expensive indicators veto; then the
/// "`4o` without `mini`" and `sonnet` mid-tier vetoes; finally a cheap-indicator
/// substring decides. All substring checks are on the lowercased name.
pub fn is_cheap_model(provider: &str, model_name: &str, registry_recommended: &[String]) -> bool {
    // registryModels.length > 0 ? registryModels : RECOMMENDED_CHEAP_MODELS[provider]
    let exact_match = if registry_recommended.is_empty() {
        recommended_cheap_models(provider).contains(&model_name)
    } else {
        registry_recommended.iter().any(|m| m == model_name)
    };
    if exact_match {
        return true;
    }

    let lower = model_name.to_lowercase();

    // Exclude known expensive models first.
    const EXPENSIVE: [&str; 5] = ["opus", "o1", "o3", "ultra", "pro"];
    if EXPENSIVE.iter().any(|i| lower.contains(i)) {
        return false;
    }

    // "4o" alone (without "mini") is mid-tier, not cheap.
    if lower.contains("4o") && !lower.contains("mini") {
        return false;
    }
    if lower.contains("sonnet") {
        return false;
    }

    const CHEAP: [&str; 12] = [
        "mini", "flash", "haiku", "turbo", "3.5", ":1b", ":2b", ":3b", ":7b", "small", "tiny",
        "instant",
    ];
    CHEAP.iter().any(|i| lower.contains(i))
}

/// Relative model cost for UI display, 1 (cheapest) … 5 (most expensive).
/// `registry_recommended` is threaded into the [`is_cheap_model`] check.
pub fn estimate_model_cost(
    provider: &str,
    model_name: &str,
    registry_recommended: &[String],
) -> i64 {
    let lower = model_name.to_lowercase();

    // Local models are free.
    if provider == "OLLAMA" {
        return 1;
    }

    // High-tier models (checked first). NB: v4 uses the dashed forms here
    // (`o1-`/`o3-`), distinct from `is_cheap_model`'s undashed `o1`/`o3`.
    const HIGH_TIER: [&str; 4] = ["opus", "o1-", "o3-", "ultra"];
    if HIGH_TIER.iter().any(|i| lower.contains(i)) {
        return 5;
    }

    if is_cheap_model(provider, model_name, registry_recommended) {
        return 2;
    }

    // Mid-tier indicators (everything else also defaults to mid-tier).
    const MID_TIER: [&str; 5] = ["sonnet", "4o", "pro", "gemini-1.5", "gemini-2.0-pro"];
    if MID_TIER.iter().any(|i| lower.contains(i)) {
        return 3;
    }

    3
}

/// The cheapest model for a provider. `registry_default` is the registry's
/// configured default (`Some` → returned as-is); otherwise the fallback map.
/// Returns `None` only for an unknown provider with no registry default (v4
/// would return `undefined` there).
pub fn get_cheapest_model(provider: &str, registry_default: Option<&str>) -> Option<String> {
    if let Some(d) = registry_default {
        return Some(d.to_string());
    }
    legacy_cheapest_model(provider).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The registry-injected path can't be exercised by the differential oracle
    // (a bare run returns no cheap config), so cover it directly here.
    #[test]
    fn registry_recommended_list_takes_precedence() {
        // A name that fails every fallback heuristic becomes cheap when it is an
        // exact match in the injected registry list.
        let registry = vec!["weird-custom-9000".to_string()];
        assert!(!is_cheap_model("OPENAI", "weird-custom-9000", &[]));
        assert!(is_cheap_model("OPENAI", "weird-custom-9000", &registry));
        // estimate_model_cost threads the same list (cheap → 2).
        assert_eq!(
            estimate_model_cost("OPENAI", "weird-custom-9000", &registry),
            2
        );
        // A non-empty registry that lacks the name falls through to heuristics
        // (a neutral name matching no indicator stays non-cheap).
        let other = vec!["something-else".to_string()];
        assert!(!is_cheap_model("OPENAI", "big-model-x", &other));
    }

    #[test]
    fn registry_default_overrides_fallback_map() {
        assert_eq!(
            get_cheapest_model("OPENAI", Some("custom-default")),
            Some("custom-default".to_string())
        );
        assert_eq!(
            get_cheapest_model("OPENAI", None),
            Some("gpt-4o-mini".to_string())
        );
        assert_eq!(get_cheapest_model("UNKNOWN_PROVIDER", None), None);
    }
}
