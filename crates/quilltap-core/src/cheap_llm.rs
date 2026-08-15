//! Cheap-LLM provider selection (v4 `lib/llm/cheap-llm.ts`): the pure
//! selection logic that picks a cost-effective provider for background tasks
//! (memory extraction, summarization, titling), plus the uncensored-provider
//! resolution for dangerous chats and the per-character prompt-cache key
//! (v4 `lib/llm/cache-key.ts`).
//!
//! The registry seam (v4 `getCheapModelConfig` from the plugin provider
//! registry) is injected, as in [`crate::cheap_model`]: the caller passes the
//! registry's cheapest-model answer for the current profile's provider (or
//! `None` for the legacy fallback map) — the differential runs with no plugins
//! registered, so `None` is the faithful wiring there.

use serde_json::Value;

use crate::cheap_model::get_cheapest_model;

/// Major version of the cacheable prompt structure (v4
/// `PROMPT_CACHE_STRUCTURE_VERSION`, `lib/llm/cache-key.ts`).
///
/// v4's history, carried verbatim:
/// 1 — initial chatId-scoped structure (post-commit 68a9eba6).
/// 2 — re-keyed to per-character (persona block became the actual prefix);
///     see docs/developer/features/per-character-prompt-caching.md.
/// 3 — Taboo section added to the universal portion of the system prompt
///     (between the math note and tool instructions); see
///     docs/developer/features/complete/taboo.md.
pub const PROMPT_CACHE_STRUCTURE_VERSION: u32 = 3;

/// v4 `buildCharacterCacheKey`: the per-character provider cache identifier.
pub fn build_character_cache_key(character_id: Option<&str>) -> Option<String> {
    let character_id = character_id.filter(|s| !s.is_empty())?;
    Some(format!(
        "quilltap:char:{character_id}:v{PROMPT_CACHE_STRUCTURE_VERSION}"
    ))
}

/// The `ConnectionProfile` subset the cheap-LLM paths consume. Boolean flags
/// collapse v4's `=== true` strict checks (absent → `false`); `base_url` keeps
/// the raw stored value (the JS `|| undefined` / `|| localhost` coercions are
/// applied at the use sites).
#[derive(Clone, Debug, Default)]
pub struct CheapLlmProfile {
    pub id: String,
    pub provider: String,
    pub model_name: String,
    pub base_url: Option<String>,
    pub is_cheap: bool,
    pub is_dangerous_compatible: bool,
    /// The profile's provider parameters (open JSON).
    pub parameters: Option<Value>,
    /// Top-level maxTokens override (for `resolveMaxTokens`).
    pub max_tokens: Option<f64>,
    /// Top-level `maxContext` override — the profile's Max Context. Read by
    /// [`profile_params`] for the Ollama `num_ctx` injection (v4 `d9c5a1c7`
    /// widened `profileParams`'s parameter to
    /// `Pick<ConnectionProfile,'provider'|'parameters'> & {maxContext?}`).
    pub max_context: Option<f64>,
    pub model_class: Option<String>,
}

/// v4 `CheapLLMConfig`. `strategy` is the raw settings string
/// (`USER_DEFINED` / `PROVIDER_CHEAPEST` / `LOCAL_FIRST`).
#[derive(Clone, Debug, Default)]
pub struct CheapLlmConfig {
    pub strategy: String,
    pub user_defined_profile_id: Option<String>,
    pub default_cheap_profile_id: Option<String>,
    pub fallback_to_local: bool,
}

/// v4 `CheapLLMSelection` — the resolved provider choice every cheap-LLM call
/// carries.
#[derive(Clone, Debug, PartialEq)]
pub struct CheapLlmSelection {
    pub provider: String,
    pub model_name: String,
    pub base_url: Option<String>,
    pub connection_profile_id: Option<String>,
    pub is_local: bool,
    /// The chosen profile's provider parameters, forwarded so per-model
    /// settings (e.g. "reasoning off") take effect for cheap-LLM tasks.
    pub profile_parameters: Option<Value>,
}

/// The `DangerousContentSettings` subset the cheap-LLM paths consume.
#[derive(Clone, Debug, Default)]
pub struct DangerousContentSettings {
    /// `OFF` / `AUTO_ROUTE` / … (the raw settings string).
    pub mode: String,
    pub uncensored_text_profile_id: Option<String>,
}

/// Options for the uncensored-provider fallback on empty responses (v4
/// `UncensoredFallbackOptions`, `cheap-llm-tasks/types.ts`).
#[derive(Clone, Debug)]
pub struct UncensoredFallbackOptions<'a> {
    pub danger_settings: &'a DangerousContentSettings,
    pub available_profiles: &'a [CheapLlmProfile],
    pub is_dangerous_chat: Option<bool>,
}

/// The provider whose server allocates its own (usually far too small) context
/// window unless the request carries `options.num_ctx`.
const NUM_CTX_PROVIDER: &str = "OLLAMA";

/// v4 `profileParams` (`lib/llm/cheap-llm.ts`), widened by `d9c5a1c7`. THE
/// single chokepoint for turning a connection profile into the forwardable
/// `profileParameters` record — every construction site goes through here so
/// the injection below applies uniformly across the Salon and the utility
/// paths.
///
/// Two steps, in v4's order:
///
/// 1. **Base** — the JS `params && typeof params === 'object'` check. `null` is
///    falsy despite `typeof null === 'object'`, so it drops out; **arrays
///    pass**, because `typeof [] === 'object'`.
/// 2. **`num_ctx` injection** — for an Ollama profile with a positive numeric
///    `maxContext`, and only when the stored bag does not already pin
///    `num_ctx`, the profile's Max Context is injected. Ollama otherwise loads
///    its own default window while the budgeter sizes prompts against Max
///    Context, and the server silently truncates.
///
/// The faithful edges, each pinned by `profile_params_equivalence`:
///
/// - the provider test is `=== 'OLLAMA'`, **case-sensitive** — a stored
///   `'ollama'` gets no injection;
/// - `base?.num_ctx == null` is JS LOOSE equality, so an explicit `null` is
///   overwritten but `0` and `false` both win and suppress the injection;
/// - spreading an **array** base yields an object with index keys
///   (`{"0":…,"1":…,"num_ctx":…}`) — v5's `preserve_order` `Map` reproduces
///   that, and the insertion order with it;
/// - overwriting an existing `num_ctx: null` keeps the key at its ORIGINAL
///   position (JS object key order, and `IndexMap::insert` alike);
/// - `typeof maxContext === 'number'` admits `Infinity` (which then
///   stringifies to `null`, on both sides) and `NaN` (which fails `> 0`).
pub fn profile_params_parts(
    provider: &str,
    parameters: Option<&Value>,
    max_context: Option<f64>,
) -> Option<Value> {
    let base = match parameters {
        Some(v @ (Value::Object(_) | Value::Array(_))) => Some(v),
        _ => None,
    };

    // `base?.num_ctx == null` — absent, or present-and-null.
    let num_ctx_unset = base
        .and_then(|b| b.get("num_ctx"))
        .is_none_or(serde_json::Value::is_null);
    let inject =
        provider == NUM_CTX_PROVIDER && max_context.is_some_and(|n| n > 0.0) && num_ctx_unset;

    if !inject {
        return base.cloned();
    }

    // `{ ...(base ?? {}), num_ctx: maxContext }`.
    let mut out = serde_json::Map::new();
    match base {
        Some(Value::Object(o)) => {
            for (k, v) in o {
                out.insert(k.clone(), v.clone());
            }
        }
        Some(Value::Array(a)) => {
            for (i, v) in a.iter().enumerate() {
                out.insert(i.to_string(), v.clone());
            }
        }
        _ => {}
    }
    // `maxContext` is a REAL column, so an integral window arrives as `32768.0`
    // and must render as JS's `32768` — the same `js_number_to_json` collapse
    // every net read uses. It also maps a non-finite f64 to `null`, exactly as
    // `JSON.stringify` renders `Infinity`.
    out.insert(
        "num_ctx".to_string(),
        crate::db::js_number_to_json(max_context.unwrap_or_default()),
    );
    Some(Value::Object(out))
}

/// [`profile_params_parts`] over the cheap-LLM profile subset.
pub fn profile_params(profile: &CheapLlmProfile) -> Option<Value> {
    profile_params_parts(
        &profile.provider,
        profile.parameters.as_ref(),
        profile.max_context,
    )
}

/// [`profile_params_parts`] over a net-read connection-profile object (the
/// shape the service paths carry). `maxContext` is a nullable REAL column, so
/// an absent or NULL cell is `None`.
pub fn profile_params_value(profile: &Value) -> Option<Value> {
    profile_params_parts(
        profile
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or(""),
        profile.get("parameters"),
        profile.get("maxContext").and_then(Value::as_f64),
    )
}

/// JS `baseUrl || undefined`.
fn base_url_or_none(profile: &CheapLlmProfile) -> Option<String> {
    profile.base_url.clone().filter(|s| !s.is_empty())
}

/// JS `baseUrl || 'http://localhost:11434'`.
fn base_url_or_localhost(profile: &CheapLlmProfile) -> Option<String> {
    Some(base_url_or_none(profile).unwrap_or_else(|| "http://localhost:11434".to_string()))
}

fn selection_from_profile(profile: &CheapLlmProfile) -> CheapLlmSelection {
    CheapLlmSelection {
        provider: profile.provider.clone(),
        model_name: profile.model_name.clone(),
        base_url: base_url_or_none(profile),
        connection_profile_id: Some(profile.id.clone()),
        is_local: profile.provider == "OLLAMA",
        profile_parameters: profile_params(profile),
    }
}

/// v4 `getCheapLLMProvider`: select the cheap LLM by the documented priority
/// order — (1) global default cheap profile, (2) `USER_DEFINED` profile,
/// (3) any `isCheap` profile (local preferred), (4) local-first Ollama,
/// (5) fall back to the current profile (Ollama as-is, else the provider's
/// cheapest model). The `onNoCheapLLM` toast callback is host UI — omitted.
///
/// `registry_cheapest_for_current` is the plugin registry's
/// `defaultModel` for `current_profile.provider` (the injected
/// `getCheapModelConfig` seam), consulted only by the priority-5 fallback.
pub fn get_cheap_llm_provider(
    current_profile: &CheapLlmProfile,
    config: &CheapLlmConfig,
    available_profiles: &[CheapLlmProfile],
    ollama_available: bool,
    registry_cheapest_for_current: Option<&str>,
) -> CheapLlmSelection {
    // Priority 1: global default cheap profile.
    if let Some(default_id) = config
        .default_cheap_profile_id
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        if let Some(p) = available_profiles.iter().find(|p| p.id == default_id) {
            return selection_from_profile(p);
        }
        // Global default not found — fall through.
    }

    // Priority 2: user-defined profile (USER_DEFINED strategy).
    if config.strategy == "USER_DEFINED" {
        if let Some(user_id) = config
            .user_defined_profile_id
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            if let Some(p) = available_profiles.iter().find(|p| p.id == user_id) {
                return selection_from_profile(p);
            }
            // Profile not found — v4 warns and falls through.
        }
    }

    // Priority 3: any profile flagged isCheap, local (Ollama) preferred.
    let cheap_profiles: Vec<&CheapLlmProfile> =
        available_profiles.iter().filter(|p| p.is_cheap).collect();
    if !cheap_profiles.is_empty() {
        if let Some(local) = cheap_profiles.iter().find(|p| p.provider == "OLLAMA") {
            return CheapLlmSelection {
                provider: "OLLAMA".to_string(),
                model_name: local.model_name.clone(),
                base_url: base_url_or_localhost(local),
                connection_profile_id: Some(local.id.clone()),
                is_local: true,
                profile_parameters: profile_params(local),
            };
        }
        return selection_from_profile(cheap_profiles[0]);
    }

    // Priority 4: local first (prefer Ollama if available).
    if config.strategy == "LOCAL_FIRST" || (config.fallback_to_local && ollama_available) {
        if let Some(ollama) = available_profiles.iter().find(|p| p.provider == "OLLAMA") {
            return CheapLlmSelection {
                provider: "OLLAMA".to_string(),
                model_name: ollama.model_name.clone(),
                base_url: base_url_or_localhost(ollama),
                connection_profile_id: Some(ollama.id.clone()),
                is_local: true,
                profile_parameters: profile_params(ollama),
            };
        }
        // LOCAL_FIRST with no Ollama profile — fall through.
    }

    // Priority 5: no dedicated cheap LLM. For Ollama, use the current profile's
    // model as-is (all local models are "free"); note v4 sets no
    // profileParameters on either priority-5 branch.
    if current_profile.provider == "OLLAMA" {
        return CheapLlmSelection {
            provider: current_profile.provider.clone(),
            model_name: current_profile.model_name.clone(),
            base_url: base_url_or_none(current_profile),
            connection_profile_id: Some(current_profile.id.clone()),
            is_local: true,
            profile_parameters: None,
        };
    }

    // Map the current provider to its cheapest variant. v4's legacy map covers
    // every Provider enum value; an out-of-vocabulary provider would give JS
    // `undefined` (and fail downstream) — surfaced here as an empty model name.
    let cheap_model = get_cheapest_model(&current_profile.provider, registry_cheapest_for_current)
        .unwrap_or_default();
    CheapLlmSelection {
        provider: current_profile.provider.clone(),
        model_name: cheap_model,
        base_url: base_url_or_none(current_profile),
        connection_profile_id: Some(current_profile.id.clone()),
        is_local: false,
        profile_parameters: None,
    }
}

/// v4 `resolveUncensoredCheapLLMSelection`: swap in an uncensored-compatible
/// provider for dangerous chats so background tasks avoid content refusals —
/// the configured uncensored text profile first, then any
/// `isDangerousCompatible` profile, else fail open with the standard selection.
pub fn resolve_uncensored_cheap_llm_selection(
    standard_selection: CheapLlmSelection,
    is_dangerous_chat: bool,
    danger_settings: Option<&DangerousContentSettings>,
    available_profiles: &[CheapLlmProfile],
) -> CheapLlmSelection {
    let Some(danger) = danger_settings else {
        return standard_selection;
    };
    if !is_dangerous_chat || danger.mode == "OFF" {
        return standard_selection;
    }

    fn uncensored_selection(p: &CheapLlmProfile) -> CheapLlmSelection {
        let is_local = p.provider == "OLLAMA";
        CheapLlmSelection {
            provider: p.provider.clone(),
            model_name: p.model_name.clone(),
            base_url: if is_local {
                base_url_or_localhost(p)
            } else {
                base_url_or_none(p)
            },
            connection_profile_id: Some(p.id.clone()),
            is_local,
            profile_parameters: profile_params(p),
        }
    }

    if let Some(profile_id) = danger
        .uncensored_text_profile_id
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        if let Some(p) = available_profiles.iter().find(|p| p.id == profile_id) {
            return uncensored_selection(p);
        }
    }

    if let Some(p) = available_profiles
        .iter()
        .find(|p| p.is_dangerous_compatible)
    {
        return uncensored_selection(p);
    }

    standard_selection
}

// === P4.6BM ===

/// v4 `resolveCheapLLMProfileId` (`app/api/v1/chats/[id]/actions/memories.ts:36-58`)
/// — the connection-profile id the user has configured for memory extraction, or
/// `None` when none resolves.
///
/// Two branches, in order, each requiring the profile to EXIST **and** to belong
/// to the user:
///
///   1. `cheapLLMSettings.defaultCheapProfileId`;
///   2. `strategy === 'USER_DEFINED'` AND `cheapLLMSettings.userDefinedProfileId`.
///
/// This is deliberately NOT [`get_cheap_llm_provider`]'s five-tier priority
/// order: v4 factors a much narrower resolver for this one action, with no
/// `isCheap` sweep, no Ollama probe and no current-profile fallback, and a
/// `None` here is a 400 rather than a silent downgrade. Ported once here so the
/// two call sites cannot drift.
///
/// `chat_settings` is the user's row (v4 `chatSettings.findByUserId`), and
/// `lookup_profile` reads a connection profile by id (v4
/// `connections.findById`), returning its `userId` — the ownership check.
pub fn resolve_cheap_llm_profile_id(
    chat_settings: Option<&serde_json::Value>,
    user_id: &str,
    mut lookup_profile_user_id: impl FnMut(&str) -> Option<String>,
) -> Option<String> {
    let cheap = chat_settings.and_then(|s| s.get("cheapLLMSettings"));
    let owned = |id: &str, lookup: &mut dyn FnMut(&str) -> Option<String>| -> bool {
        lookup(id).as_deref() == Some(user_id)
    };

    if let Some(id) = cheap
        .and_then(|c| c.get("defaultCheapProfileId"))
        .and_then(serde_json::Value::as_str)
    {
        if owned(id, &mut lookup_profile_user_id) {
            return Some(id.to_string());
        }
    }

    let is_user_defined = cheap
        .and_then(|c| c.get("strategy"))
        .and_then(serde_json::Value::as_str)
        == Some("USER_DEFINED");
    if is_user_defined {
        if let Some(id) = cheap
            .and_then(|c| c.get("userDefinedProfileId"))
            .and_then(serde_json::Value::as_str)
        {
            if owned(id, &mut lookup_profile_user_id) {
                return Some(id.to_string());
            }
        }
    }

    None
}

// === end P4.6BM ===

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, provider: &str, model: &str) -> CheapLlmProfile {
        CheapLlmProfile {
            id: id.to_string(),
            provider: provider.to_string(),
            model_name: model.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn cache_key_shape() {
        assert_eq!(
            build_character_cache_key(Some("abc")).as_deref(),
            // v3 since P4.D50 (the Taboo section joined the cacheable prefix).
            Some("quilltap:char:abc:v3")
        );
        assert_eq!(build_character_cache_key(None), None);
        assert_eq!(build_character_cache_key(Some("")), None);
    }

    #[test]
    fn is_cheap_profile_prefers_local() {
        let mut cheap_remote = profile("p1", "OPENAI", "gpt-mini");
        cheap_remote.is_cheap = true;
        let mut cheap_local = profile("p2", "OLLAMA", "llama3:3b");
        cheap_local.is_cheap = true;
        let current = profile("cur", "ANTHROPIC", "claude-big");
        let sel = get_cheap_llm_provider(
            &current,
            &CheapLlmConfig {
                strategy: "PROVIDER_CHEAPEST".into(),
                ..Default::default()
            },
            &[cheap_remote, cheap_local],
            false,
            None,
        );
        assert_eq!(sel.provider, "OLLAMA");
        assert_eq!(sel.base_url.as_deref(), Some("http://localhost:11434"));
        assert!(sel.is_local);
    }

    #[test]
    fn dangerous_chat_swaps_to_uncensored_profile() {
        let mut unc = profile("u1", "OLLAMA", "dolphin");
        unc.is_dangerous_compatible = true;
        let standard = CheapLlmSelection {
            provider: "ANTHROPIC".into(),
            model_name: "safe".into(),
            base_url: None,
            connection_profile_id: Some("cur".into()),
            is_local: false,
            profile_parameters: None,
        };
        let danger = DangerousContentSettings {
            mode: "AUTO_ROUTE".into(),
            uncensored_text_profile_id: Some("u1".into()),
        };
        let sel = resolve_uncensored_cheap_llm_selection(
            standard.clone(),
            true,
            Some(&danger),
            std::slice::from_ref(&unc),
        );
        assert_eq!(sel.model_name, "dolphin");
        // Not dangerous → untouched.
        let sel2 =
            resolve_uncensored_cheap_llm_selection(standard.clone(), false, Some(&danger), &[unc]);
        assert_eq!(sel2, standard);
    }
}
