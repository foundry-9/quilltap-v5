//! Dangerous-content gatekeeper (v4
//! `lib/services/dangerous-content/gatekeeper.service.ts`) — classifies content
//! for dangerous/sensitive material via a dedicated moderation provider (a
//! seam) or, failing that, the cheap LLM.
//!
//! Fail-safe: any error returns `{ isDangerous: false }` — never blocks the
//! user.
//!
//! ## Seams (tracked deferrals)
//!
//!   - **Moderation provider** ([`ModerationProvider`]) — v4's plugin registry
//!     (`moderationProviderRegistry`) + its API-key auto-detect
//!     (`autoDetectModerationApiKey`, which scans `connection_profiles`) are the
//!     unported plugin boundary. The seam collapses "is a provider available,
//!     with a key" + the raw `provider.moderate(content)` call into one
//!     injected call returning a [`ModerationOutcome`]; the port still runs
//!     [`map_moderation_result`] over the raw result so the score/category math
//!     is verified. The default [`NoModerationProvider`] mirrors a no-plugin
//!     instance (always falls through to the LLM).
//!   - **Cheap-LLM API key** (`getApiKeyForCheapLLMSelection`) — host-side, as
//!     for [`crate::services::cheap_llm_exec`]; the boundary starts at the
//!     [`CompletionProvider`] call.
//!   - **`logLLMCall`** — fire-and-forget llm-logs write; host-side.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::clock::now_unix_ms;
use crate::db::chat_settings::DangerousContentSettings;
use crate::memory_tasks::strip_code_fences;
use crate::model::completion::{
    CompletionMessage, CompletionParams, CompletionProvider, CompletionUsage,
};

use super::prompt_text;
use crate::cheap_llm::CheapLlmSelection;

/// One classified category (v4 `{ category, score, label }`).
#[derive(Clone, Debug, PartialEq)]
pub struct DangerCategory {
    pub category: String,
    pub score: f64,
    pub label: String,
}

/// Result of content classification (v4 `DangerClassificationResult`).
#[derive(Clone, Debug, PartialEq)]
pub struct DangerClassificationResult {
    pub is_dangerous: bool,
    pub score: f64,
    pub categories: Vec<DangerCategory>,
    pub usage: Option<CompletionUsage>,
    /// `"moderation"` | `"llm"`.
    pub source: Option<String>,
    pub provider_name: Option<String>,
}

impl DangerClassificationResult {
    /// v4's `safeFallback` — the fail-safe not-dangerous result (no usage).
    pub fn safe_fallback() -> Self {
        Self {
            is_dangerous: false,
            score: 0.0,
            categories: Vec::new(),
            usage: None,
            source: None,
            provider_name: None,
        }
    }
}

/// One provider-side category score (v4 `ModerationCategoryResult`, `flagged`
/// unused by the mapper).
#[derive(Clone, Debug, PartialEq)]
pub struct ModerationCategoryScore {
    pub category: String,
    pub score: f64,
}

/// A raw moderation-provider result (v4 `ModerationResult`).
#[derive(Clone, Debug, PartialEq)]
pub struct ModerationResult {
    pub flagged: bool,
    pub categories: Vec<ModerationCategoryScore>,
}

/// The outcome of consulting the moderation-provider seam.
pub enum ModerationOutcome {
    /// No provider registered, or no API key auto-detected — fall through to the
    /// cheap-LLM classifier (v4's two `return null` branches).
    NotAvailable,
    /// The provider moderated the content: the raw result + the provider name
    /// (v4 `provider.metadata.providerName`).
    Moderated {
        result: ModerationResult,
        provider_name: String,
    },
    /// The provider call threw — v4 lets this propagate to `classifyContent`'s
    /// outer catch, which returns the safe fallback (NOT the LLM path).
    Failed(String),
}

/// The moderation-provider seam — v4's `classifyWithModerationProvider`
/// (registry lookup + API-key auto-detect + `provider.moderate`), collapsed to
/// one injected call. Given the content + context, return a [`ModerationOutcome`].
pub trait ModerationProvider {
    fn moderate(
        &self,
        content: &str,
        user_id: &str,
        settings: &DangerousContentSettings,
        chat_id: Option<&str>,
    ) -> impl std::future::Future<Output = ModerationOutcome> + Send;
}

/// A moderation provider that is never available — the faithful wiring for a
/// no-plugin instance (v4: `getDefaultProvider()` returns null).
pub struct NoModerationProvider;

impl ModerationProvider for NoModerationProvider {
    async fn moderate(
        &self,
        _content: &str,
        _user_id: &str,
        _settings: &DangerousContentSettings,
        _chat_id: Option<&str>,
    ) -> ModerationOutcome {
        ModerationOutcome::NotAvailable
    }
}

/// v4 `MODERATION_CATEGORY_MAP` — OpenAI moderation category → Concierge
/// category. An unmapped provider category passes through unchanged.
fn map_moderation_category(provider_category: &str) -> &str {
    match provider_category {
        "sexual" => "nsfw",
        "sexual/minors" => "nsfw",
        "violence" => "violence",
        "violence/graphic" => "violence",
        "hate" => "hate_speech",
        "hate/threatening" => "hate_speech",
        "harassment" => "hate_speech",
        "harassment/threatening" => "hate_speech",
        "self-harm" => "self_harm",
        "self-harm/intent" => "self_harm",
        "self-harm/instructions" => "self_harm",
        "illicit" => "illegal_activity",
        "illicit/violent" => "illegal_activity",
        other => other,
    }
}

/// v4 `CATEGORY_LABELS` — human-readable labels for Concierge categories. An
/// unknown category falls back to itself.
pub fn category_label(category: &str) -> &str {
    match category {
        "nsfw" => "Sexual/NSFW content",
        "violence" => "Violence or graphic content",
        "hate_speech" => "Hate speech or harassment",
        "self_harm" => "Self-harm content",
        "illegal_activity" => "Illegal activity",
        "disturbing" => "Disturbing content",
        other => other,
    }
}

/// v4 `mapModerationResult`: aggregate provider categories into Concierge
/// categories (max score per mapped category), keep those above the relevance
/// floor, and derive the overall score. `isDangerous` is the provider's
/// `flagged` OR the max mapped score meeting the threshold.
pub fn map_moderation_result(
    moderation_result: &ModerationResult,
    threshold: f64,
) -> DangerClassificationResult {
    // Aggregate scores by Concierge category (take max score per category).
    // Insertion order of first appearance is preserved so the resulting
    // category array order matches v4's `Map` iteration.
    let mut order: Vec<String> = Vec::new();
    let mut category_scores: HashMap<String, f64> = HashMap::new();
    for cat in &moderation_result.categories {
        let mapped = map_moderation_category(&cat.category).to_string();
        let existing = category_scores.get(&mapped).copied().unwrap_or(0.0);
        if cat.score > existing {
            category_scores.insert(mapped.clone(), cat.score);
        }
        if !order.contains(&mapped) {
            order.push(mapped);
        }
    }

    // OpenAI returns tiny nonzero scores for irrelevant categories; filter to a
    // minimum relevance floor.
    const RELEVANCE_FLOOR: f64 = 0.01;
    let mut categories: Vec<DangerCategory> = Vec::new();
    for category in &order {
        let score = category_scores[category];
        if score >= RELEVANCE_FLOOR {
            categories.push(DangerCategory {
                category: category.clone(),
                score,
                label: category_label(category).to_string(),
            });
        }
    }

    let max_score = categories
        .iter()
        .map(|c| c.score)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_score = if categories.is_empty() {
        0.0
    } else {
        max_score
    };

    DangerClassificationResult {
        is_dangerous: moderation_result.flagged || max_score >= threshold,
        score: max_score,
        categories,
        usage: None,
        source: None,
        provider_name: None,
    }
}

/// v4 `parseClassificationResponse`: strip code fences, parse JSON, derive the
/// effective score/danger from the overall score, the max category score, and
/// the explicit `isDangerous` flag. Any parse failure → safe fallback.
pub fn parse_classification_response(
    response_content: &str,
    threshold: f64,
) -> DangerClassificationResult {
    let clean = strip_code_fences(response_content);
    let parsed: serde_json::Value = match serde_json::from_str(&clean) {
        Ok(v) => v,
        Err(_) => return DangerClassificationResult::safe_fallback(),
    };

    // `typeof parsed.score === 'number' ? parsed.score : 0`
    let overall_score = parsed.get("score").and_then(js_number).unwrap_or(0.0);

    let categories: Vec<DangerCategory> = match parsed.get("categories") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .map(|c| DangerCategory {
                // `String(c.category || 'unknown')`
                category: js_string_or(c.get("category"), "unknown"),
                // `typeof c.score === 'number' ? c.score : 0`
                score: c.get("score").and_then(js_number).unwrap_or(0.0),
                // `String(c.label || '')`
                label: js_string_or(c.get("label"), ""),
            })
            .collect(),
        _ => Vec::new(),
    };

    let max_category_score = categories
        .iter()
        .map(|c| c.score)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_category_score = if categories.is_empty() {
        0.0
    } else {
        max_category_score
    };
    let effective_score = overall_score.max(max_category_score);
    let llm_says_dangerous = parsed.get("isDangerous") == Some(&serde_json::Value::Bool(true));

    DangerClassificationResult {
        is_dangerous: effective_score >= threshold || llm_says_dangerous,
        score: effective_score,
        categories,
        usage: None,
        source: None,
        provider_name: None,
    }
}

/// JS `typeof x === 'number' ? x : <default>` for a `serde_json` value: only a
/// real JSON number counts (booleans/strings don't).
fn js_number(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
}

/// JS `String(x || fallback)`: a falsy value (absent / null / `false` / `0` /
/// `""`) yields the fallback; otherwise the JS string coercion. The classifier
/// fields are strings in practice, so `String(x)` on a present truthy string is
/// the string itself.
fn js_string_or(v: Option<&serde_json::Value>, fallback: &str) -> String {
    match v {
        None | Some(serde_json::Value::Null) => fallback.to_string(),
        Some(serde_json::Value::Bool(false)) => fallback.to_string(),
        Some(serde_json::Value::String(s)) if s.is_empty() => fallback.to_string(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => {
            // `0 || fallback` → fallback; otherwise `String(n)`. Category/label
            // are strings in practice — this branch is a defensive edge.
            if n.as_f64() == Some(0.0) {
                fallback.to_string()
            } else {
                n.to_string()
            }
        }
        Some(other) => other.to_string(),
    }
}

// --- classification cache (v4's module-global LRU by content hash) ---

struct CacheEntry {
    result: DangerClassificationResult,
    timestamp_ms: i64,
}

const CACHE_MAX_SIZE: usize = 200;
const CACHE_TTL_MS: i64 = 5 * 60 * 1000;

fn cache() -> &'static Mutex<Vec<(String, CacheEntry)>> {
    // A `Vec` preserves insertion order so the oldest-first eviction matches v4's
    // `Map.keys().next()`.
    static CACHE: OnceLock<Mutex<Vec<(String, CacheEntry)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

fn content_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(content.as_bytes());
    let hex = hex::encode(digest);
    hex[..16].to_string()
}

fn get_cached_result(content_hash: &str) -> Option<DangerClassificationResult> {
    let mut guard = cache().lock().expect("classification cache lock");
    let idx = guard.iter().position(|(k, _)| k == content_hash)?;
    if now_unix_ms() - guard[idx].1.timestamp_ms > CACHE_TTL_MS {
        guard.remove(idx);
        return None;
    }
    Some(guard[idx].1.result.clone())
}

fn cache_result(content_hash: String, result: DangerClassificationResult) {
    let mut guard = cache().lock().expect("classification cache lock");
    if guard.len() >= CACHE_MAX_SIZE {
        guard.remove(0);
    }
    // Replace an existing entry in place (keeps a single key), else push.
    if let Some(idx) = guard.iter().position(|(k, _)| *k == content_hash) {
        guard[idx].1 = CacheEntry {
            result,
            timestamp_ms: now_unix_ms(),
        };
    } else {
        guard.push((
            content_hash,
            CacheEntry {
                result,
                timestamp_ms: now_unix_ms(),
            },
        ));
    }
}

/// v4 `classifyContent`. Tries the moderation provider first (free,
/// purpose-built); on `NotAvailable` falls back to the cheap-LLM classifier. Any
/// error is fail-safe (`safeFallback`).
///
/// The cheap-LLM API key acquisition (`getApiKeyForCheapLLMSelection`) is the
/// host-side seam; this port's boundary is the [`CompletionProvider`] call, so it
/// does not model the null-key early return (which only happens when no key is
/// available — a host concern).
pub async fn classify_content<M, C>(
    moderation: &M,
    completion: &C,
    content: &str,
    cheap_llm_selection: &CheapLlmSelection,
    user_id: &str,
    settings: &DangerousContentSettings,
    chat_id: Option<&str>,
) -> DangerClassificationResult
where
    M: ModerationProvider,
    C: CompletionProvider,
{
    match classify_inner(
        moderation,
        completion,
        content,
        cheap_llm_selection,
        user_id,
        settings,
        chat_id,
    )
    .await
    {
        Ok(result) => result,
        // v4's outer catch → fail safe.
        Err(_) => DangerClassificationResult::safe_fallback(),
    }
}

async fn classify_inner<M, C>(
    moderation: &M,
    completion: &C,
    content: &str,
    cheap_llm_selection: &CheapLlmSelection,
    user_id: &str,
    settings: &DangerousContentSettings,
    chat_id: Option<&str>,
) -> Result<DangerClassificationResult, ()>
where
    M: ModerationProvider,
    C: CompletionProvider,
{
    let hash = content_hash(content);
    if let Some(cached) = get_cached_result(&hash) {
        return Ok(cached);
    }

    // Try the moderation provider first.
    match moderation
        .moderate(content, user_id, settings, chat_id)
        .await
    {
        ModerationOutcome::Moderated {
            result,
            provider_name,
        } => {
            let mut mapped = map_moderation_result(&result, settings.threshold);
            mapped.source = Some("moderation".to_string());
            mapped.provider_name = Some(provider_name);
            cache_result(hash, mapped.clone());
            return Ok(mapped);
        }
        // v4: `provider.moderate` throwing propagates to the outer catch → safe.
        ModerationOutcome::Failed(_) => return Err(()),
        ModerationOutcome::NotAvailable => {}
    }

    // Fall back to cheap-LLM classification. API-key acquisition is host-side.
    let mut system_prompt = prompt_text::CLASSIFICATION_SYSTEM_PROMPT.to_string();
    if let Some(custom) = settings
        .custom_classification_prompt
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        system_prompt.push_str(&format!("\n\nADDITIONAL INSTRUCTIONS:\n{custom}"));
    }

    let messages = vec![
        CompletionMessage::system(system_prompt),
        CompletionMessage::user(format!("Classify the following content:\n\n{content}")),
    ];

    let params = CompletionParams {
        messages,
        model: cheap_llm_selection.model_name.clone(),
        temperature: Some(0.1),
        max_tokens: 500,
        // v4 sets no strictMaxTokens / cacheKey on this call; neither is in the
        // canned-completion key, so they don't affect the differential.
        strict_max_tokens: false,
        cache_key: None,
        profile_parameters: cheap_llm_selection.profile_parameters.clone(),
    };

    let response = completion
        .send_message(
            &cheap_llm_selection.provider,
            cheap_llm_selection.base_url.as_deref(),
            &params,
        )
        .await
        .map_err(|_| ())?;

    let mut result = parse_classification_response(&response.content, settings.threshold);
    result.usage = response.usage;
    result.source = Some("llm".to_string());
    result.provider_name = Some(cheap_llm_selection.provider.clone());

    cache_result(hash, result.clone());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_is_byte_exact() {
        assert_eq!(
            prompt_text::CLASSIFICATION_SYSTEM_PROMPT.chars().count(),
            1114
        );
        assert!(prompt_text::CLASSIFICATION_SYSTEM_PROMPT.ends_with("to handle."));
    }

    #[test]
    fn parse_safe_json() {
        let r = parse_classification_response(
            r#"{ "isDangerous": false, "score": 0.0, "categories": [] }"#,
            0.7,
        );
        assert!(!r.is_dangerous);
        assert_eq!(r.score, 0.0);
        assert!(r.categories.is_empty());
    }

    #[test]
    fn parse_dangerous_by_category_score() {
        let r = parse_classification_response(
            r#"```json
{ "isDangerous": false, "score": 0.2, "categories": [{ "category": "violence", "score": 0.9, "label": "gore" }] }
```"#,
            0.7,
        );
        // effectiveScore = max(0.2, 0.9) = 0.9 >= 0.7
        assert!(r.is_dangerous);
        assert_eq!(r.score, 0.9);
        assert_eq!(r.categories[0].category, "violence");
    }

    #[test]
    fn parse_dangerous_by_explicit_flag() {
        let r = parse_classification_response(
            r#"{ "isDangerous": true, "score": 0.1, "categories": [] }"#,
            0.7,
        );
        assert!(r.is_dangerous);
        assert_eq!(r.score, 0.1);
    }

    #[test]
    fn parse_failure_is_safe() {
        let r = parse_classification_response("not json at all", 0.7);
        assert!(!r.is_dangerous);
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn map_moderation_takes_max_and_floor() {
        let mr = ModerationResult {
            flagged: false,
            categories: vec![
                ModerationCategoryScore {
                    category: "sexual".into(),
                    score: 0.3,
                },
                ModerationCategoryScore {
                    category: "sexual/minors".into(),
                    score: 0.8,
                },
                ModerationCategoryScore {
                    category: "hate".into(),
                    score: 0.001, // below floor → dropped
                },
            ],
        };
        let r = map_moderation_result(&mr, 0.7);
        // sexual + sexual/minors both map to nsfw → max 0.8; hate dropped.
        assert_eq!(r.categories.len(), 1);
        assert_eq!(r.categories[0].category, "nsfw");
        assert_eq!(r.categories[0].score, 0.8);
        assert_eq!(r.categories[0].label, "Sexual/NSFW content");
        assert_eq!(r.score, 0.8);
        assert!(r.is_dangerous); // 0.8 >= 0.7
    }

    #[test]
    fn map_moderation_flagged_forces_dangerous() {
        let mr = ModerationResult {
            flagged: true,
            categories: vec![],
        };
        let r = map_moderation_result(&mr, 0.7);
        assert!(r.is_dangerous);
        assert_eq!(r.score, 0.0);
        assert!(r.categories.is_empty());
    }
}
