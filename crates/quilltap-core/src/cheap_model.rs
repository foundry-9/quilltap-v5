//! Port of v4's cheap-model selector from lib/llm/cheap-llm.ts
//! (`getCheapestModel`), plus the deprecated fallback table it consults from
//! lib/llm/fallback-data.ts.
//!
//! Registry seam: the function first consults the plugin registry
//! (`getCheapModelConfig`) and falls back to the hardcoded table here. The
//! registry-sourced value is injected as a parameter (an established pattern in
//! this port — cf. `token_estimation`'s `chars_per_token`): the default model
//! for [`get_cheapest_model`]. Pass `None` to take the fallback path, which is
//! exactly what the differential oracle exercises (the registry returns no cheap
//! config in a bare run).
//!
//! P4.D157 (v4 `d4138b96b`, the 4.9 dead-code sweep): v4 deleted `isCheapModel`
//! and `estimateModelCost` as unreferenced. Measured on this side too — v5's
//! `is_cheap_model` was reached only from `estimate_model_cost`'s ladder, and
//! nothing called `estimate_model_cost` outside the differential — so both twins
//! and the `LEGACY_RECOMMENDED_CHEAP_MODELS` table they alone consulted were
//! deleted with them. `getCheapestModel` survives on both sides and stays live
//! here (`cheap_llm::select_cheap_llm`).

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

    // The registry-injected default can't be exercised by the differential
    // oracle (a bare run returns no cheap config), so cover it directly here.
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
