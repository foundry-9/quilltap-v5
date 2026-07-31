//! Model pricing fetcher + cost estimation — v4 `lib/llm/pricing-fetcher.ts` +
//! `lib/services/cost-estimation.service.ts` + `checkModelSupportsTools`
//! (`lib/tools/pseudo-tool-support.ts`). Closes the finalizer's cost-estimation
//! evidence seam and the injected `model_supports_native_tools` input.
//!
//! ## Sans-IO: the fetch is an injected seam
//!
//! v4 hits the network (`fetch` to openrouter.ai / an Ollama host, the
//! `@openrouter/sdk` client). The Rust core is sans-IO: the fetch is the injected
//! [`PricingFetch`] trait handing back the raw response JSON (or `None` on
//! failure), and every clock read is the explicit `now_ms` parameter. The 3 s
//! fetch timeout and Node socket behavior are host-timing (no timers in the core).
//! Live transport composes this in W4.7d, exactly as the request-builder /
//! stream-decoder units left transport to that unit.
//!
//! ## The two OpenRouter response casings (a faithful v4 divergence)
//!
//! The **public** models path (`fetchOpenRouterPublicPricing`) reads snake_case
//! (`supported_parameters` / `context_length`); the **authenticated SDK** path
//! (`fetchOpenRouterPricing`) reads camelCase (`supportedParameters` /
//! `contextLength`). These are ported as two separate parsers
//! ([`parse_openrouter_public`] / [`parse_openrouter_sdk`]) — never unified, never
//! "fixed".
//!
//! ## The caches are per-instance (v4's module-globals)
//!
//! v4 keeps `pricingCache` / `openRouterPublicCache` /
//! `openRouterPublicFetchFailureAt` as module-level mutable state. Carried here as
//! [`PricingFetcher`] instance state (`Mutex`-wrapped) — the host holds one
//! process-global fetcher; the differential creates a fresh one per scenario (the
//! `CheapLlmTaskExecutor` precedent). Storing `updatedAt` as a millisecond stamp
//! (rather than v4's ISO string re-parsed each read) is an internal detail; the
//! observable TTL behavior is identical under the injected `now_ms`.

mod fallback_data;

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

use crate::clock::iso_from_unix_ms;
use crate::provider_manifest::Registry;

pub use fallback_data::fallback_pricing;

/// Cache TTL: 24 hours (v4 `CACHE_TTL_MS`).
const CACHE_TTL_MS: i64 = 24 * 60 * 60 * 1000;
/// Negative-cache TTL: 5 minutes (v4 `OPENROUTER_NEGATIVE_CACHE_TTL_MS`).
const NEGATIVE_CACHE_TTL_MS: i64 = 5 * 60 * 1000;

/// v4 `ModelPricing` (the fetcher's richer row, distinct from the pure-cost
/// [`crate::pricing::ModelPricing`]). Serializes camelCase; `supportsVision` /
/// `supportsTools` are `Option<bool>` (omitted-when-`None` — v4's optional flags,
/// left undefined by the registry path, set by the parse/fallback paths).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchedModelPricing {
    pub model_id: String,
    pub provider: String,
    pub name: String,
    // serde camelCase renders `per_1m` -> `per1m`; v4's key is `promptCostPer1M`
    // (capital M), and an integer-valued rate renders bare (`10`, not `10.0`).
    #[serde(rename = "promptCostPer1M", serialize_with = "serialize_js_number")]
    pub prompt_cost_per_1m: f64,
    #[serde(rename = "completionCostPer1M", serialize_with = "serialize_js_number")]
    pub completion_cost_per_1m: f64,
    pub context_length: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_vision: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_tools: Option<bool>,
    pub fetched_at: String,
}

/// v4 `PriceSource` — the provenance tag on a cost estimate.
pub mod price_source {
    pub const OPENROUTER: &str = "openrouter";
    pub const REGISTRY: &str = "registry";
    pub const FALLBACK: &str = "fallback";
    pub const OPENROUTER_ESTIMATE: &str = "openrouter-estimate";
    pub const UNAVAILABLE: &str = "unavailable";
}

/// v4 `CostEstimateResult`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimateResult {
    /// `None` mirrors v4's `cost: null`. An integer-valued cost renders bare.
    #[serde(serialize_with = "serialize_opt_js_number")]
    pub cost: Option<f64>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_pricing: Option<FetchedModelPricing>,
}

/// Serialize an `f64` the way `JSON.stringify` renders it (integer-valued → bare
/// integer). Mirrors [`crate::db::js_number_to_json`].
fn serialize_js_number<S: serde::Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    crate::db::js_number_to_json(*v).serialize(s)
}

/// Optional variant: `None` → JSON `null`, `Some` → JS-number rendering.
fn serialize_opt_js_number<S: serde::Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(n) => crate::db::js_number_to_json(*n).serialize(s),
        None => s.serialize_none(),
    }
}

/// A connection profile, as far as pricing needs it (v4 `ConnectionProfile`
/// subset). The `profiles` + `api_keys` a caller supplies stand in for v4's
/// `repos.connections.findAll()` + `findApiKeyByIdAndUserId` (host-side).
#[derive(Clone, Debug)]
pub struct PricingProfile {
    pub provider: String,
    pub api_key_id: Option<String>,
    pub base_url: Option<String>,
}

/// The per-user pricing context the spine supplies (v4 reads it from the repos
/// inside `refreshPricingCache` / `getApiKeyForProvider`).
#[derive(Clone, Debug, Default)]
pub struct PricingContext {
    pub profiles: Vec<PricingProfile>,
    /// `apiKeyId` -> key value (v4 `findApiKeyByIdAndUserId(...).key_value`).
    pub api_keys: HashMap<String, String>,
}

/// The injected fetch seam. Each method returns the raw JSON response body, or
/// `None` on a network / non-2xx error (v4's `catch`).
pub trait PricingFetch {
    /// `GET https://openrouter.ai/api/v1/models`.
    fn openrouter_public_models(&self) -> Option<Value>;
    /// `@openrouter/sdk` `client.models.list()`.
    fn openrouter_sdk_models(&self, api_key: &str) -> Option<Value>;
    /// `GET {base_url}/api/tags`.
    fn ollama_tags(&self, base_url: &str) -> Option<Value>;
}

/// v4 `PROVIDER_TO_OPENROUTER_SLUG` — the OpenRouter slug for a Quilltap provider
/// (only the four with public-pricing coverage).
fn provider_to_openrouter_slug(provider: &str) -> Option<&'static str> {
    match provider {
        "OPENAI" => Some("openai"),
        "ANTHROPIC" => Some("anthropic"),
        "GOOGLE" => Some("google"),
        "GROK" => Some("x-ai"),
        _ => None,
    }
}

// ============================================================================
// JS parseFloat + falsy-string coercion
// ============================================================================

/// v4 `String(x || '0')` for a pricing field: a falsy value (absent / null /
/// false / 0 / "") becomes `"0"`; a truthy string is itself; a number is its JS
/// string. (Arrays/objects don't occur in a pricing field.)
fn price_string(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "0".to_string(),
        Some(Value::Bool(true)) => "true".to_string(),
        Some(Value::Number(n)) => {
            if n.as_f64() == Some(0.0) {
                "0".to_string()
            } else {
                n.to_string()
            }
        }
        Some(Value::String(s)) => {
            if s.is_empty() {
                "0".to_string()
            } else {
                s.clone()
            }
        }
        Some(other) => other.to_string(),
    }
}

/// JS `parseFloat`: parse the leading numeric prefix (after skipping leading
/// whitespace); `NaN` when no numeric prefix exists (garbage propagates, per the
/// work order). Reproduces JS semantics — a trailing non-numeric suffix is
/// ignored (`"1.5x"` → `1.5`), `"Infinity"` is honored, and a bare exponent
/// marker is dropped (`"1e"` → `1`).
pub fn js_parse_float(input: &str) -> f64 {
    let t = input.trim_start();
    let bytes = t.as_bytes();
    let mut i = 0usize;

    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    if t[i..].starts_with("Infinity") {
        return if bytes.first() == Some(&b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }

    let mut seen_digit = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        seen_digit = true;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            seen_digit = true;
        }
    }
    if !seen_digit {
        return f64::NAN;
    }
    // Optional exponent — only consumed when it has at least one digit.
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }

    t[..i].parse::<f64>().unwrap_or(f64::NAN)
}

// ============================================================================
// Parsers (the two casings + ollama)
// ============================================================================

fn as_array(v: &Value, key: &str) -> Vec<Value> {
    v.get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// v4 `fetchOpenRouterPublicPricing`'s per-model parse (snake_case). String prices
/// via [`js_parse_float`] × 1e6; `context_length ?? null`; `architecture.modality`
/// includes "image" → vision; `supported_parameters` includes tools/tool_choice →
/// tools. `fetched_at` is derived from `now_ms` (v4's `new Date().toISOString()`).
pub fn parse_openrouter_public(data: &Value, now_ms: i64) -> Vec<FetchedModelPricing> {
    let fetched_at = iso_from_unix_ms(now_ms);
    as_array(data, "data")
        .iter()
        .map(|model| {
            let pricing = model.get("pricing");
            let prompt = js_parse_float(&price_string(pricing.and_then(|p| p.get("prompt"))));
            let completion =
                js_parse_float(&price_string(pricing.and_then(|p| p.get("completion"))));
            let modality_has_image = model
                .get("architecture")
                .and_then(|a| a.get("modality"))
                .map(modality_includes_image)
                .unwrap_or(false);
            let supports_tools = model
                .get("supported_parameters")
                .and_then(Value::as_array)
                .map(|params| {
                    params
                        .iter()
                        .any(|p| matches!(p.as_str(), Some("tools") | Some("tool_choice")))
                })
                .unwrap_or(false);
            FetchedModelPricing {
                model_id: str_field(model, "id"),
                provider: "OPENROUTER".to_string(),
                name: model
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| str_field(model, "id")),
                prompt_cost_per_1m: prompt * 1_000_000.0,
                completion_cost_per_1m: completion * 1_000_000.0,
                context_length: model.get("context_length").and_then(Value::as_i64),
                supports_vision: Some(modality_has_image),
                supports_tools: Some(supports_tools),
                fetched_at: fetched_at.clone(),
            }
        })
        .collect()
}

/// v4 `fetchOpenRouterPricing`'s per-model parse (camelCase SDK shape). Same price
/// math; `contextLength` / `supportedParameters` / `architecture.modality`.
/// v4 returns `sortByCost(models)` — applied by the caller.
pub fn parse_openrouter_sdk(data: &Value, now_ms: i64) -> Vec<FetchedModelPricing> {
    let fetched_at = iso_from_unix_ms(now_ms);
    as_array(data, "data")
        .iter()
        .map(|model| {
            let pricing = model.get("pricing");
            let prompt = js_parse_float(&price_string(pricing.and_then(|p| p.get("prompt"))));
            let completion =
                js_parse_float(&price_string(pricing.and_then(|p| p.get("completion"))));
            let modality_has_image = model
                .get("architecture")
                .and_then(|a| a.get("modality"))
                .map(modality_includes_image)
                .unwrap_or(false);
            let supports_tools = model
                .get("supportedParameters")
                .and_then(Value::as_array)
                .map(|params| {
                    params
                        .iter()
                        .any(|p| matches!(p.as_str(), Some("tools") | Some("tool_choice")))
                })
                .unwrap_or(false);
            FetchedModelPricing {
                model_id: str_field(model, "id"),
                // v4 uses `model.name` directly (no `?? id`) on the SDK path.
                name: model
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_default(),
                provider: "OPENROUTER".to_string(),
                prompt_cost_per_1m: prompt * 1_000_000.0,
                completion_cost_per_1m: completion * 1_000_000.0,
                context_length: model.get("contextLength").and_then(Value::as_i64),
                supports_vision: Some(modality_has_image),
                supports_tools: Some(supports_tools),
                fetched_at: fetched_at.clone(),
            }
        })
        .collect()
}

/// v4 `fetchOllamaModels`: free (0 cost), null context; vision when the name
/// includes "llava"/"vision"; tools always false.
pub fn parse_ollama_models(data: &Value, now_ms: i64) -> Vec<FetchedModelPricing> {
    let fetched_at = iso_from_unix_ms(now_ms);
    as_array(data, "models")
        .iter()
        .map(|model| {
            let name = str_field(model, "name");
            let vision = name.contains("llava") || name.contains("vision");
            FetchedModelPricing {
                model_id: name.clone(),
                provider: "OLLAMA".to_string(),
                name,
                prompt_cost_per_1m: 0.0,
                completion_cost_per_1m: 0.0,
                context_length: None,
                supports_vision: Some(vision),
                supports_tools: Some(false),
                fetched_at: fetched_at.clone(),
            }
        })
        .collect()
}

fn modality_includes_image(modality: &Value) -> bool {
    match modality {
        // v4: `architecture?.modality?.includes('image')`. `modality` is a string
        // like "text+image->text"; `.includes` is substring on a string (and
        // element-membership on an array — handle both).
        Value::String(s) => s.contains("image"),
        Value::Array(arr) => arr.iter().any(|m| m.as_str() == Some("image")),
        _ => false,
    }
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default()
}

/// Average cost per 1M (v4 `getAverageCostPer1M`).
fn average_cost(m: &FetchedModelPricing) -> f64 {
    (m.prompt_cost_per_1m + m.completion_cost_per_1m) / 2.0
}

/// v4 `sortByCost` — stable sort by average cost ascending.
fn sort_by_cost(mut models: Vec<FetchedModelPricing>) -> Vec<FetchedModelPricing> {
    models.sort_by(|a, b| {
        average_cost(a)
            .partial_cmp(&average_cost(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    models
}

// ============================================================================
// The stateful fetcher
// ============================================================================

#[derive(Default)]
struct CacheState {
    /// (models, fetchedAt ms).
    openrouter_public: Option<(Vec<FetchedModelPricing>, i64)>,
    openrouter_public_failure_at: Option<i64>,
    /// (providers -> models, updatedAt ms).
    pricing_cache: Option<(HashMap<String, Vec<FetchedModelPricing>>, i64)>,
}

/// The stateful pricing fetcher (v4's module-global caches, per-instance here).
pub struct PricingFetcher<F: PricingFetch> {
    fetch: F,
    state: Mutex<CacheState>,
}

impl<F: PricingFetch> PricingFetcher<F> {
    pub fn new(fetch: F) -> Self {
        Self {
            fetch,
            state: Mutex::new(CacheState::default()),
        }
    }

    /// v4 `getOpenRouterPublicPricing`: TTL-fresh cache → return; else within the
    /// negative-cache window → `[]`; else fetch, storing only when non-empty.
    fn get_openrouter_public_pricing(&self, now_ms: i64) -> Vec<FetchedModelPricing> {
        {
            let state = self.state.lock().expect("pricing lock");
            if let Some((models, fetched_at)) = &state.openrouter_public {
                if now_ms - fetched_at < CACHE_TTL_MS {
                    return models.clone();
                }
            }
            if let Some(failure_at) = state.openrouter_public_failure_at {
                if now_ms - failure_at < NEGATIVE_CACHE_TTL_MS {
                    return Vec::new();
                }
            }
        }

        // Fetch (outside the lock, matching v4's async fetch outside cache reads).
        let models = match self.fetch.openrouter_public_models() {
            Some(body) => {
                let models = parse_openrouter_public(&body, now_ms);
                let mut state = self.state.lock().expect("pricing lock");
                state.openrouter_public_failure_at = None;
                if !models.is_empty() {
                    state.openrouter_public = Some((models.clone(), now_ms));
                }
                models
            }
            None => {
                let mut state = self.state.lock().expect("pricing lock");
                state.openrouter_public_failure_at = Some(now_ms);
                Vec::new()
            }
        };
        models
    }

    /// v4 `getOpenRouterPricingForModel`: exact `slug/modelId`, then fuzzy
    /// (substring either direction within the slug namespace).
    pub fn get_openrouter_pricing_for_model(
        &self,
        provider: &str,
        model_id: &str,
        now_ms: i64,
    ) -> Option<FetchedModelPricing> {
        let slug = provider_to_openrouter_slug(provider)?;
        let models = self.get_openrouter_public_pricing(now_ms);
        if models.is_empty() {
            return None;
        }

        let expected = format!("{slug}/{model_id}");
        if let Some(m) = models.iter().find(|m| m.model_id == expected) {
            return Some(m.clone());
        }
        let prefix = format!("{slug}/");
        models
            .iter()
            .find(|m| {
                if !m.model_id.starts_with(&prefix) {
                    return false;
                }
                let name = &m.model_id[prefix.len()..];
                model_id.contains(name) || name.contains(model_id)
            })
            .cloned()
    }

    /// v4 `fetchProviderPricing`: OLLAMA (baseUrl fetch), OPENROUTER (SDK fetch
    /// with api key), else `FALLBACK_PRICING[provider]`.
    fn fetch_provider_pricing(
        &self,
        provider: &str,
        now_ms: i64,
        ctx: &PricingContext,
    ) -> Vec<FetchedModelPricing> {
        match provider {
            "OLLAMA" => {
                let base = ctx
                    .profiles
                    .iter()
                    .find(|p| p.provider == "OLLAMA")
                    .and_then(|p| p.base_url.clone());
                match base {
                    Some(base_url) if !base_url.is_empty() => self
                        .fetch
                        .ollama_tags(&base_url)
                        .map(|body| parse_ollama_models(&body, now_ms))
                        .unwrap_or_default(),
                    _ => Vec::new(),
                }
            }
            "OPENROUTER" => {
                let key = ctx
                    .profiles
                    .iter()
                    .find(|p| p.provider == "OPENROUTER" && p.api_key_id.is_some())
                    .and_then(|p| p.api_key_id.as_ref())
                    .and_then(|id| ctx.api_keys.get(id).cloned());
                match key {
                    Some(api_key) if !api_key.is_empty() => self
                        .fetch
                        .openrouter_sdk_models(&api_key)
                        .map(|body| sort_by_cost(parse_openrouter_sdk(&body, now_ms)))
                        .unwrap_or_default(),
                    _ => Vec::new(),
                }
            }
            other => fallback_pricing(other),
        }
    }

    /// v4 `refreshPricingCache`: per unique provider in the profiles, fetch (or
    /// fall back), store non-empty results (else FALLBACK if it has rows).
    fn refresh_pricing_cache(
        &self,
        now_ms: i64,
        ctx: &PricingContext,
    ) -> HashMap<String, Vec<FetchedModelPricing>> {
        let mut seen = HashSet::new();
        let mut providers: Vec<String> = Vec::new();
        for p in &ctx.profiles {
            if seen.insert(p.provider.clone()) {
                providers.push(p.provider.clone());
            }
        }

        let mut cache: HashMap<String, Vec<FetchedModelPricing>> = HashMap::new();
        for provider in providers {
            let models = self.fetch_provider_pricing(&provider, now_ms, ctx);
            if !models.is_empty() {
                cache.insert(provider, models);
            } else {
                let fb = fallback_pricing(&provider);
                if !fb.is_empty() {
                    cache.insert(provider, fb);
                }
            }
        }

        let mut state = self.state.lock().expect("pricing lock");
        state.pricing_cache = Some((cache.clone(), now_ms));
        cache
    }

    /// v4 `getPricingCache`: TTL-fresh → return; else refresh.
    fn get_pricing_cache(
        &self,
        now_ms: i64,
        ctx: &PricingContext,
    ) -> HashMap<String, Vec<FetchedModelPricing>> {
        {
            let state = self.state.lock().expect("pricing lock");
            if let Some((cache, updated_at)) = &state.pricing_cache {
                if now_ms - updated_at < CACHE_TTL_MS {
                    return cache.clone();
                }
            }
        }
        self.refresh_pricing_cache(now_ms, ctx)
    }

    /// v4 `getProviderPricing`: cache for the provider, else `FALLBACK_PRICING`.
    fn get_provider_pricing(
        &self,
        provider: &str,
        now_ms: i64,
        ctx: &PricingContext,
    ) -> Vec<FetchedModelPricing> {
        let cache = self.get_pricing_cache(now_ms, ctx);
        cache
            .get(provider)
            .cloned()
            .unwrap_or_else(|| fallback_pricing(provider))
    }

    /// v4 `getModelPricing` (pricing-fetcher): exact `modelId` match in the
    /// provider's cached/fallback list.
    fn get_model_pricing(
        &self,
        provider: &str,
        model_id: &str,
        now_ms: i64,
        ctx: &PricingContext,
    ) -> Option<FetchedModelPricing> {
        self.get_provider_pricing(provider, now_ms, ctx)
            .into_iter()
            .find(|m| m.model_id == model_id)
    }

    /// v4 `checkModelSupportsTools`: OPENROUTER → live cache exact match →
    /// `supportsTools ?? true`, model-missing → true; other providers →
    /// `FALLBACK_PRICING` exact → `supportsTools ?? true`; unknown → true. Never
    /// breaks toward pseudo-tools.
    pub fn check_model_supports_tools(
        &self,
        provider: &str,
        model_name: &str,
        now_ms: i64,
        ctx: &PricingContext,
    ) -> bool {
        if provider == "OPENROUTER" {
            let cache = self.get_pricing_cache(now_ms, ctx);
            if let Some(models) = cache.get("OPENROUTER") {
                if let Some(model) = models.iter().find(|m| m.model_id == model_name) {
                    return model.supports_tools.unwrap_or(true);
                }
            }
            // Model not found in cache -> default true.
            return true;
        }

        let fallback = fallback_pricing(provider);
        if let Some(model) = fallback.iter().find(|m| m.model_id == model_name) {
            return model.supports_tools.unwrap_or(true);
        }
        true
    }

    /// v4 `getModelPricingFromRegistry`: the plugin registry first (manifest
    /// `getModelPricing` → `{input, output}`), then `FALLBACK_PRICING` (exact OR
    /// substring either direction).
    fn get_model_pricing_from_registry(
        registry: &Registry,
        provider: &str,
        model_id: &str,
        now_ms: i64,
    ) -> Option<FetchedModelPricing> {
        if let Some(pricing) = registry.model_pricing(provider, model_id) {
            return Some(FetchedModelPricing {
                model_id: model_id.to_string(),
                provider: provider.to_string(),
                name: model_id.to_string(),
                prompt_cost_per_1m: pricing.input,
                completion_cost_per_1m: pricing.output,
                context_length: None,
                supports_vision: None,
                supports_tools: None,
                fetched_at: iso_from_unix_ms(now_ms),
            });
        }

        fallback_pricing(provider).into_iter().find(|m| {
            m.model_id == model_id
                || m.model_id.contains(model_id)
                || model_id.contains(&m.model_id)
        })
    }

    /// v4 `estimateMessageCost`: the OpenRouter-cache → registry → pricing-cache →
    /// public-per-model cascade, each with its source tag. Any internal failure
    /// yields `{ cost: None, source: 'unavailable' }`.
    #[allow(clippy::too_many_arguments)]
    pub fn estimate_message_cost(
        &self,
        registry: &Registry,
        provider: &str,
        model_name: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
        now_ms: i64,
        ctx: &PricingContext,
    ) -> CostEstimateResult {
        // 1. OpenRouter cache (only when the provider is OpenRouter).
        if provider == "OPENROUTER" {
            if let Some(pricing) = self.get_model_pricing("OPENROUTER", model_name, now_ms, ctx) {
                let cost = estimate_cost(&pricing, prompt_tokens, completion_tokens);
                return CostEstimateResult {
                    cost: Some(cost),
                    source: price_source::OPENROUTER.to_string(),
                    model_pricing: Some(pricing),
                };
            }
        }

        // 2. Provider registry (plugin pricing / FALLBACK substring).
        if let Some(pricing) =
            Self::get_model_pricing_from_registry(registry, provider, model_name, now_ms)
        {
            let cost = estimate_cost(&pricing, prompt_tokens, completion_tokens);
            return CostEstimateResult {
                cost: Some(cost),
                source: price_source::REGISTRY.to_string(),
                model_pricing: Some(pricing),
            };
        }

        // 3. Pricing cache for the provider (exact match).
        if let Some(pricing) = self.get_model_pricing(provider, model_name, now_ms, ctx) {
            let cost = estimate_cost(&pricing, prompt_tokens, completion_tokens);
            return CostEstimateResult {
                cost: Some(cost),
                source: price_source::FALLBACK.to_string(),
                model_pricing: Some(pricing),
            };
        }

        // 4. OpenRouter public estimate (non-OpenRouter providers only).
        if provider != "OPENROUTER" {
            if let Some(pricing) =
                self.get_openrouter_pricing_for_model(provider, model_name, now_ms)
            {
                let cost = estimate_cost(&pricing, prompt_tokens, completion_tokens);
                return CostEstimateResult {
                    cost: Some(cost),
                    source: price_source::OPENROUTER_ESTIMATE.to_string(),
                    model_pricing: Some(pricing),
                };
            }
        }

        CostEstimateResult {
            cost: None,
            source: price_source::UNAVAILABLE.to_string(),
            model_pricing: None,
        }
    }
}

/// v4 `estimateCost`: `(tokens/1e6)*rate` per bucket, summed (divide-then-multiply,
/// then add — bit-identical to v4).
fn estimate_cost(pricing: &FetchedModelPricing, prompt_tokens: i64, completion_tokens: i64) -> f64 {
    let prompt_cost = (prompt_tokens as f64 / 1_000_000.0) * pricing.prompt_cost_per_1m;
    let completion_cost = (completion_tokens as f64 / 1_000_000.0) * pricing.completion_cost_per_1m;
    prompt_cost + completion_cost
}

#[cfg(test)]
mod tests;
