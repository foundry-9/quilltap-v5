//! The cheap-LLM route an extraction unit of work runs on — port of v4
//! `lib/memory/extraction/route.ts` (`ac6348c5f`, bug 213 from `c2795733f`;
//! pin `83334b9a5`), resolved once per unit of work through the chat's
//! Concierge state.
//!
//! v4's why, carried:
//!  - The selection comes from the user's cheap-LLM settings
//!    ([`build_cheap_llm_config`], the global default cheap profile first)
//!    against the unit of work's connection profile, then through
//!    `resolve_uncensored_cheap_llm_selection`: only an Unmoderated chat whose
//!    policy routes direct goes to the uncensored desk. (Pre-refactor the
//!    extraction path dropped `defaultCheapProfileId`.)
//!  - The uncensored fallback is the empty-answer retry every pass carries —
//!    the EPISODE pass included (bug 213). Its `is_dangerous_chat` is the
//!    question "is this chat on the uncensored route?", answered by
//!    `should_use_uncensored_route` — never the classifier's
//!    `chats.isDangerousChat` telemetry. A Locked chat's policy is inert, so
//!    it neither reroutes nor fails over.
//!  - The output budget is the connection profile's (`resolve_max_tokens`):
//!    it bounds every pass's cap (`pass_cap`).
//!
//! v4 wraps the Concierge read in a `try` whose catch WARNs `[Memory] Could
//! not resolve the Concierge policy for extraction; running without the
//! uncensored fallback` and leaves the fallback off. v5's
//! `resolve_concierge_settings` is infallible (it degrades on any stored
//! shape), so that arm has no v5 home; the route always carries a fallback.

use serde_json::Value;

use crate::cheap_llm::{
    get_cheap_llm_provider, resolve_uncensored_cheap_llm_selection, CheapLlmConfig,
    CheapLlmProfile, CheapLlmSelection, UncensoredFallbackOptions,
};
use crate::context_budget::resolve_max_tokens;
use crate::services::dangerous_content::chat_override::should_use_uncensored_route;
use crate::services::dangerous_content::resolver::{
    resolve_concierge_settings, ResolvedConciergePolicy,
};

/// v4 `buildCheapLLMConfig(chatSettings)` (`lib/llm/cheap-llm.ts:142-154`):
/// `DEFAULT_CHEAP_LLM_CONFIG` (`PROVIDER_CHEAPEST`, `fallbackToLocal: true`)
/// when the settings carry no `cheapLLMSettings`, else the four keys —
/// `userDefinedProfileId ?? undefined` and `defaultCheapProfileId ??
/// undefined` KEEP an empty string (the selector's truthy check then skips
/// it, as v4's `if (config.defaultCheapProfileId)` does).
pub fn build_cheap_llm_config(chat_settings: Option<&Value>) -> CheapLlmConfig {
    let Some(s) = chat_settings
        .and_then(|c| c.get("cheapLLMSettings"))
        .filter(|s| s.is_object())
    else {
        return CheapLlmConfig {
            strategy: "PROVIDER_CHEAPEST".to_string(),
            user_defined_profile_id: None,
            default_cheap_profile_id: None,
            fallback_to_local: true,
        };
    };
    let str_of = |k: &str| s.get(k).and_then(Value::as_str).map(str::to_string);
    CheapLlmConfig {
        // The schema defaults `strategy`; a row without one reads as v4's
        // `undefined`, which no strategy branch matches — PROVIDER_CHEAPEST's.
        strategy: str_of("strategy").unwrap_or_else(|| "PROVIDER_CHEAPEST".to_string()),
        user_defined_profile_id: str_of("userDefinedProfileId"),
        default_cheap_profile_id: str_of("defaultCheapProfileId"),
        fallback_to_local: s.get("fallbackToLocal").and_then(Value::as_bool) == Some(true),
    }
}

/// The owned half of v4 `UncensoredFallbackOptions` (the executor borrows it
/// per call through [`ExtractionRoute::fallback_options`]).
#[derive(Clone, Debug)]
pub struct UncensoredFallback {
    pub concierge_policy: ResolvedConciergePolicy,
    pub available_profiles: Vec<CheapLlmProfile>,
    pub is_dangerous_chat: bool,
}

/// v4 `ExtractionRoute`.
#[derive(Clone, Debug)]
pub struct ExtractionRoute {
    pub cheap_llm: CheapLlmSelection,
    /// `None` only when the Concierge policy could not be resolved (no v5
    /// arm — see the module doc).
    pub uncensored_fallback: Option<UncensoredFallback>,
    /// The output budget the pass caps are clamped by.
    pub max_tokens: f64,
    /// The profile the unit of work runs against (cost events are booked to
    /// it).
    pub connection_profile: Value,
}

impl ExtractionRoute {
    /// The borrowed fallback options the executor takes.
    pub fn fallback_options(&self) -> Option<UncensoredFallbackOptions<'_>> {
        self.uncensored_fallback
            .as_ref()
            .map(|f| UncensoredFallbackOptions {
                concierge_policy: &f.concierge_policy,
                available_profiles: &f.available_profiles,
                is_dangerous_chat: Some(f.is_dangerous_chat),
            })
    }
}

/// v4 `resolveExtractionRoute({ chat, connectionProfile, chatSettings,
/// availableProfiles })` — the route, resolved once; never fails.
pub fn resolve_extraction_route(
    chat: &Value,
    connection_profile: &Value,
    chat_settings: Option<&Value>,
    available_profiles: &[Value],
) -> ExtractionRoute {
    let profile = crate::cheap_llm::cheap_llm_profile_from_value(connection_profile);
    let available: Vec<CheapLlmProfile> = available_profiles
        .iter()
        .map(crate::cheap_llm::cheap_llm_profile_from_value)
        .collect();
    let base = get_cheap_llm_provider(
        &profile,
        &build_cheap_llm_config(chat_settings),
        &available,
        false,
        None,
    );
    let uncensored_route = should_use_uncensored_route(Some(chat));
    let concierge_policy = resolve_concierge_settings(chat_settings, Some(chat));
    let cheap_llm = resolve_uncensored_cheap_llm_selection(
        base,
        uncensored_route,
        Some(&concierge_policy),
        &available,
    );
    let route = ExtractionRoute {
        cheap_llm,
        uncensored_fallback: Some(UncensoredFallback {
            concierge_policy,
            available_profiles: available,
            is_dangerous_chat: uncensored_route,
        }),
        max_tokens: resolve_max_tokens(
            profile.max_tokens.map(|v| v as i64),
            profile.model_class.as_deref(),
        ) as f64,
        connection_profile: connection_profile.clone(),
    };
    let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
    tracing::debug!(
        chatId = %chat_id,
        provider = %route.cheap_llm.provider,
        model = %route.cheap_llm.model_name,
        uncensoredRoute = uncensored_route,
        hasUncensoredFallback = route.uncensored_fallback.is_some(),
        maxTokens = route.max_tokens as i64,
        "[Memory] Extraction route resolved"
    );
    route
}
