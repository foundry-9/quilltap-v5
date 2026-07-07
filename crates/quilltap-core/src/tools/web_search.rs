//! Port of v4's `search_web` tool (`lib/tools/handlers/web-search-handler.ts` +
//! `lib/tools/web-search-tool.ts`).
//!
//! The whole search boundary — the plugin `searchProviderRegistry`
//! (`getDefaultProvider` / `executeSearch` / provider `formatResults`), the
//! per-provider API-key lookup, and the legacy Serper HTTP fallback — is an
//! external boundary and becomes the injected [`WebSearchProvider`] seam (canned
//! outcome both sides — the settled decision). The portable half is the input
//! validation, the outcome → [`WebSearchOutput`] mapping (byte-exact error
//! strings), and the built-in result formatter.
//!
//! Deferrals: the provider's own `formatResults` (plugin surface — the built-in
//! formatter is what this port reproduces); the API-key acquisition (host-side,
//! the `cheap_llm_exec` precedent); a **date-only** `publishedDate` (the strict
//! `iso_to_ms` handles the full-ISO form JS `Date.parse` produces; a bare
//! `YYYY-MM-DD` would render `"Invalid Date"` — the corpus uses full-ISO dates).

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::format_time::format_date_short_us;
use crate::model::request_builder::BuiltRequest;
use crate::model::wire::SyncWireTransport;

/// v4 `WebSearchResult` — one search result. `publishedDate` optional.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    #[serde(rename = "publishedDate", skip_serializing_if = "Option::is_none")]
    pub published_date: Option<String>,
}

/// What the search boundary produced (the injected seam's return). Mirrors the
/// branches v4's handler maps to distinct outputs.
#[derive(Debug, Clone)]
pub enum WebSearchOutcome {
    /// A provider ran a search (native plugin or Serper). `success` false with an
    /// error is a provider-reported failure.
    ProviderResult {
        success: bool,
        results: Vec<WebSearchResult>,
        error: Option<String>,
    },
    /// The active provider requires an API key but none is configured for the user.
    MissingApiKey { display_name: String },
    /// No provider registered AND no Serper fallback key.
    NotConfigured,
}

/// The injected search boundary — v4's `searchProviderRegistry` + API-key lookup +
/// Serper fallback. Given the validated `(query, max_results, user_id)`, returns
/// what happened. Both differential sides inject the same canned outcome.
pub trait WebSearchProvider: Send + Sync {
    fn search(&self, query: &str, max_results: i64, user_id: &str) -> WebSearchOutcome;
}

/// The default boundary: no search provider configured (v4's behavior on an
/// instance with no search plugin + no Serper key → the "not configured" error).
/// Production wires a real provider (host-side, plugin surface).
#[derive(Debug, Clone, Copy, Default)]
pub struct NotConfiguredWebSearch;

impl WebSearchProvider for NotConfiguredWebSearch {
    fn search(&self, _query: &str, _max_results: i64, _user_id: &str) -> WebSearchOutcome {
        WebSearchOutcome::NotConfigured
    }
}

/// v4 `WebSearchToolOutput` — `{ success, results?, error?, totalFound, query }`.
/// Serialized in a fixed order (`success, results, error, totalFound, query`)
/// reproducing both branches' `JSON.stringify` (success omits `error`, failure
/// omits `results`).
#[derive(Debug, Clone)]
pub struct WebSearchOutput {
    pub success: bool,
    pub results: Option<Vec<WebSearchResult>>,
    pub error: Option<String>,
    pub total_found: usize,
    pub query: String,
}

impl Serialize for WebSearchOutput {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut len = 3; // success, totalFound, query
        if self.results.is_some() {
            len += 1;
        }
        if self.error.is_some() {
            len += 1;
        }
        let mut st = s.serialize_struct("WebSearchOutput", len)?;
        st.serialize_field("success", &self.success)?;
        if let Some(r) = &self.results {
            st.serialize_field("results", r)?;
        }
        if let Some(e) = &self.error {
            st.serialize_field("error", e)?;
        }
        // v4 `totalFound` is a JS number; corpus values are small integers.
        st.serialize_field("totalFound", &self.total_found)?;
        st.serialize_field("query", &self.query)?;
        st.end()
    }
}

/// v4 `validateWebSearchInput`: `query` a 1..=500-char string that is not
/// whitespace-only; `maxResults` an optional int 1..=10 (default 5).
fn validate(args: &Value) -> bool {
    let Some(obj) = args.as_object() else {
        return false;
    };
    match obj.get("query") {
        Some(Value::String(s)) => {
            // min(1).max(500) is UTF-16 length; the refine rejects whitespace-only.
            let len16 = s.encode_utf16().count();
            if !(1..=500).contains(&len16) || s.trim().is_empty() {
                return false;
            }
        }
        _ => return false,
    }
    match obj.get("maxResults") {
        None | Some(Value::Null) => true,
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => {
            let v = n.as_i64().unwrap_or(0);
            (1..=10).contains(&v)
        }
        _ => false,
    }
}

/// Execute the `search_web` tool (v4 `executeWebSearchTool`) over the injected
/// [`WebSearchProvider`] seam.
pub fn execute_web_search<P: WebSearchProvider + ?Sized>(
    provider: &P,
    user_id: &str,
    args: &Value,
) -> WebSearchOutput {
    if !validate(args) {
        return WebSearchOutput {
            success: false,
            results: None,
            error: Some(
                "Invalid input: query is required and must be a non-empty string".to_string(),
            ),
            total_found: 0,
            query: String::new(),
        };
    }
    let obj = args.as_object().expect("validated object");
    let query = obj
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // `maxResults = 5` default.
    let max_results = obj.get("maxResults").and_then(Value::as_i64).unwrap_or(5);

    match provider.search(&query, max_results, user_id) {
        WebSearchOutcome::ProviderResult {
            success,
            results,
            error,
        } => {
            if success {
                let total = results.len();
                WebSearchOutput {
                    success: true,
                    results: Some(results),
                    error: None,
                    total_found: total,
                    query,
                }
            } else {
                WebSearchOutput {
                    success: false,
                    results: None,
                    // v4: `providerResult.error ?? 'Search provider returned an error'`.
                    error: Some(
                        error.filter(|s| !s.is_empty()).unwrap_or_else(|| {
                            "Search provider returned an error".to_string()
                        }),
                    ),
                    total_found: 0,
                    query,
                }
            }
        }
        WebSearchOutcome::MissingApiKey { display_name } => WebSearchOutput {
            success: false,
            results: None,
            error: Some(format!(
                "No API key configured for {display_name}. Please add your API key in Settings > API Keys."
            )),
            total_found: 0,
            query,
        },
        WebSearchOutcome::NotConfigured => WebSearchOutput {
            success: false,
            results: None,
            error: Some(
                "Web search is not configured. Please add a search provider API key in Settings > API Keys."
                    .to_string(),
            ),
            total_found: 0,
            query,
        },
    }
}

/// v4 `formatWebSearchResults` — the BUILT-IN formatter (the provider's own
/// `formatResults` is a plugin seam not ported). A `publishedDate` renders via the
/// UTC-pinned `toLocaleDateString()`.
pub fn format_web_search_results(results: &[WebSearchResult]) -> String {
    if results.is_empty() {
        return "No search results found.".to_string();
    }
    let formatted: Vec<String> = results
        .iter()
        .enumerate()
        .map(|(index, r)| {
            let date_str = match &r.published_date {
                Some(d) if !d.is_empty() => format!(" (Published: {})", format_date_short_us(d)),
                _ => String::new(),
            };
            format!(
                "[Result {}]{date_str}\nTitle: {}\nURL: {}\nSummary: {}",
                index + 1,
                r.title,
                r.url,
                r.snippet
            )
        })
        .collect();
    format!(
        "Found {} search results:\n\n{}",
        results.len(),
        formatted.join("\n\n")
    )
}

// ===========================================================================
// The Serper wire (W4.7f) — the real WebSearchProvider closing the W4.1d5 seam
// ===========================================================================

/// v4 Serper `SERPER_API_URL`.
pub const SERPER_API_URL: &str = "https://google.serper.dev/search";

/// Build the Serper search request (v4 `executeSearch` / the legacy fallback —
/// identical body/url). The `X-API-KEY` header carries the key; only method / url
/// / body are differential-checked.
pub fn build_serper_request(query: &str, max_results: i64, api_key: &str) -> BuiltRequest {
    let mut body = Map::new();
    body.insert("q".to_string(), Value::String(query.to_string()));
    body.insert("num".to_string(), Value::from(max_results));
    BuiltRequest {
        method: "POST".to_string(),
        url: SERPER_API_URL.to_string(),
        headers: vec![
            ("X-API-KEY".to_string(), api_key.to_string()),
            ("content-type".to_string(), "application/json".to_string()),
        ],
        body: Value::Object(body),
    }
}

fn str_or_empty(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// v4's Serper success mapping: `organic[]` → results, then the `knowledgeGraph`
/// UNSHIFT when `kg.description && results.length < maxResults`.
pub fn map_serper_results(body: &Value, max_results: i64) -> Vec<WebSearchResult> {
    let mut results: Vec<WebSearchResult> = body
        .get("organic")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|r| WebSearchResult {
                    title: str_or_empty(r, "title"),
                    url: str_or_empty(r, "link"),
                    snippet: str_or_empty(r, "snippet"),
                    published_date: r.get("date").and_then(Value::as_str).map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default();

    if let Some(kg) = body.get("knowledgeGraph") {
        // `kg?.description &&` — a truthy (non-empty) description.
        let description = kg
            .get("description")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        if let Some(description) = description {
            if (results.len() as i64) < max_results {
                // `kg.title ?? 'Knowledge Graph'` (nullish default).
                let title = kg
                    .get("title")
                    .filter(|v| !v.is_null())
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| "Knowledge Graph".to_string());
                // `kg.source?.link ?? ''`.
                let url = kg
                    .get("source")
                    .and_then(|s| s.get("link"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                results.insert(
                    0,
                    WebSearchResult {
                        title,
                        url,
                        snippet: description.to_string(),
                        published_date: None,
                    },
                );
            }
        }
    }
    results
}

/// The Serper PLUGIN error strings (v4 `executeSearch` on `!ok`).
pub fn serper_plugin_error(status: u16, status_text: &str, error_text: &str) -> String {
    match status {
        401 | 403 => {
            "Invalid Serper API key. Please check your API key in Settings > API Keys.".to_string()
        }
        429 => "Serper API rate limit exceeded. Please try again later or upgrade your plan at serper.dev."
            .to_string(),
        _ => format!("Serper API error: {status} {status_text} - {error_text}"),
    }
}

/// The legacy env-var FALLBACK error strings (v4 `executeSerperFallback` on
/// `!ok` — deliberately DIFFERENT from the plugin set for 401/403 and the generic).
pub fn serper_fallback_error(status: u16, status_text: &str) -> String {
    match status {
        401 | 403 => {
            "Invalid Serper API key. Please check your SERPER_API_KEY environment variable or configure the API key in Settings > API Keys.".to_string()
        }
        429 => "Serper API rate limit exceeded. Please try again later or upgrade your plan at serper.dev."
            .to_string(),
        _ => format!("Search API error: {status} {status_text}"),
    }
}

/// The search API-key lookup seam (v4 `getAllApiKeys()` scan for `provider === X
/// && isActive` → `key_value`). DISTINCT from the moderation/routing
/// [`ApiKeyResolver`](crate::services::dangerous_content::provider_routing::ApiKeyResolver)
/// (by `apiKeyId`) — do not unify. Host-side (W4.7d's `db::api_keys`); canned in
/// the differential.
pub trait SearchApiKeyLookup: Send + Sync {
    fn find_active_key(&self, provider: &str, user_id: &str) -> Option<String>;
}

/// A lookup that finds no key (the faithful no-key wiring).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSearchApiKeys;
impl SearchApiKeyLookup for NoSearchApiKeys {
    fn find_active_key(&self, _provider: &str, _user_id: &str) -> Option<String> {
        None
    }
}

/// The real [`WebSearchProvider`] (closes the W4.1d5 seam). Reproduces v4
/// `executeWebSearchTool`'s provider path (the Serper plugin) + the legacy env-var
/// Serper fallback, over the injected [`SyncWireTransport`] + [`SearchApiKeyLookup`].
///
/// `serper_registered` mirrors `searchProviderRegistry.getDefaultProvider()`
/// returning the Serper plugin; `fallback_env_key` is the host `SERPER_API_KEY`
/// env var (injected — host config).
pub struct RealWebSearchProvider<T: SyncWireTransport, K: SearchApiKeyLookup> {
    transport: T,
    key_lookup: K,
    serper_registered: bool,
    fallback_env_key: Option<String>,
}

impl<T: SyncWireTransport, K: SearchApiKeyLookup> RealWebSearchProvider<T, K> {
    pub fn new(
        transport: T,
        key_lookup: K,
        serper_registered: bool,
        fallback_env_key: Option<String>,
    ) -> Self {
        Self {
            transport,
            key_lookup,
            serper_registered,
            fallback_env_key,
        }
    }

    /// Run the Serper request over the transport, mapping success + the error sets.
    /// `fallback` selects the legacy error strings + default catch message.
    fn run_serper(
        &self,
        query: &str,
        max_results: i64,
        api_key: &str,
        fallback: bool,
    ) -> WebSearchOutcome {
        let req = build_serper_request(query, max_results, api_key);
        let body = req.body_string();
        match self
            .transport
            .send(&req.method, &req.url, &req.headers, &body)
        {
            Ok(resp) if resp.ok() => {
                let data: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
                WebSearchOutcome::ProviderResult {
                    success: true,
                    results: map_serper_results(&data, max_results),
                    error: None,
                }
            }
            Ok(resp) => {
                let error = if fallback {
                    serper_fallback_error(resp.status, &resp.status_text)
                } else {
                    serper_plugin_error(resp.status, &resp.status_text, &resp.body)
                };
                WebSearchOutcome::ProviderResult {
                    success: false,
                    results: Vec::new(),
                    error: Some(error),
                }
            }
            Err(msg) => {
                // Plugin: `error.message || 'Unknown error during Serper web search'`.
                // Fallback: the outer catch → `... || 'Unknown error during web search'`.
                let default = if fallback {
                    "Unknown error during web search"
                } else {
                    "Unknown error during Serper web search"
                };
                let error = if msg.is_empty() {
                    default.to_string()
                } else {
                    msg
                };
                WebSearchOutcome::ProviderResult {
                    success: false,
                    results: Vec::new(),
                    error: Some(error),
                }
            }
        }
    }
}

impl<T: SyncWireTransport, K: SearchApiKeyLookup> WebSearchProvider
    for RealWebSearchProvider<T, K>
{
    fn search(&self, query: &str, max_results: i64, user_id: &str) -> WebSearchOutcome {
        if self.serper_registered {
            // Serper's `config.requiresApiKey` is true → look up the key.
            let Some(api_key) = self.key_lookup.find_active_key("SERPER", user_id) else {
                return WebSearchOutcome::MissingApiKey {
                    display_name: "Serper Web Search".to_string(),
                };
            };
            self.run_serper(query, max_results, &api_key, false)
        } else if let Some(env_key) = self.fallback_env_key.clone() {
            self.run_serper(query, max_results, &env_key, true)
        } else {
            WebSearchOutcome::NotConfigured
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Canned(WebSearchOutcome);
    impl WebSearchProvider for Canned {
        fn search(&self, _q: &str, _m: i64, _u: &str) -> WebSearchOutcome {
            self.0.clone()
        }
    }

    fn result(title: &str, date: Option<&str>) -> WebSearchResult {
        WebSearchResult {
            title: title.into(),
            url: "https://example.com".into(),
            snippet: "A snippet.".into(),
            published_date: date.map(str::to_string),
        }
    }

    #[test]
    fn validation_rejects_bad_input() {
        let p = Canned(WebSearchOutcome::NotConfigured);
        let out = execute_web_search(&p, "u", &json!({ "query": "   " }));
        assert!(!out.success);
        assert_eq!(out.query, "");
        let out2 = execute_web_search(&p, "u", &json!("nope"));
        assert!(!out2.success);
    }

    #[test]
    fn success_maps_and_formats() {
        let p = Canned(WebSearchOutcome::ProviderResult {
            success: true,
            results: vec![
                result("AI news", Some("2026-06-15T00:00:00.000Z")),
                result("More", None),
            ],
            error: None,
        });
        let out = execute_web_search(&p, "u", &json!({ "query": "ai" }));
        assert!(out.success);
        assert_eq!(out.total_found, 2);
        let fmt = format_web_search_results(out.results.as_deref().unwrap());
        assert!(fmt.contains("(Published: 6/15/2026)"));
        assert!(fmt.starts_with("Found 2 search results:"));
    }

    #[test]
    fn error_branches() {
        let missing = Canned(WebSearchOutcome::MissingApiKey {
            display_name: "Serper".into(),
        });
        let out = execute_web_search(&missing, "u", &json!({ "query": "x" }));
        assert_eq!(
            out.error.as_deref(),
            Some(
                "No API key configured for Serper. Please add your API key in Settings > API Keys."
            )
        );
        let notcfg = Canned(WebSearchOutcome::NotConfigured);
        let out2 = execute_web_search(&notcfg, "u", &json!({ "query": "x" }));
        assert!(out2
            .error
            .as_deref()
            .unwrap()
            .starts_with("Web search is not configured"));
        let fail = Canned(WebSearchOutcome::ProviderResult {
            success: false,
            results: vec![],
            error: None,
        });
        let out3 = execute_web_search(&fail, "u", &json!({ "query": "x" }));
        assert_eq!(
            out3.error.as_deref(),
            Some("Search provider returned an error")
        );
    }

    #[test]
    fn serper_request_and_kg_unshift() {
        let req = build_serper_request("cats", 3, "k");
        assert_eq!(req.url, SERPER_API_URL);
        assert_eq!(req.body_string(), r#"{"q":"cats","num":3}"#);

        // kg unshifts only when results.len() < maxResults.
        let body = json!({
            "organic": [{ "title": "A", "link": "https://a", "snippet": "sa" }],
            "knowledgeGraph": { "title": "KG", "description": "kd", "source": { "link": "https://kg" } }
        });
        let under = map_serper_results(&body, 3);
        assert_eq!(under.len(), 2);
        assert_eq!(under[0].title, "KG"); // unshifted to front
        assert_eq!(under[0].url, "https://kg");
        // At capacity → NOT unshifted.
        let at_cap = map_serper_results(&body, 1);
        assert_eq!(at_cap.len(), 1);
        assert_eq!(at_cap[0].title, "A");
    }

    #[test]
    fn serper_error_string_sets_differ() {
        assert_eq!(
            serper_plugin_error(401, "Unauthorized", "bad"),
            "Invalid Serper API key. Please check your API key in Settings > API Keys."
        );
        assert_eq!(
            serper_fallback_error(403, "Forbidden"),
            "Invalid Serper API key. Please check your SERPER_API_KEY environment variable or configure the API key in Settings > API Keys."
        );
        assert_eq!(
            serper_plugin_error(500, "Internal Server Error", "boom"),
            "Serper API error: 500 Internal Server Error - boom"
        );
        assert_eq!(
            serper_fallback_error(500, "Internal Server Error"),
            "Search API error: 500 Internal Server Error"
        );
    }

    #[test]
    fn real_provider_over_canned_transport() {
        use crate::model::wire::{wire_key, CannedSyncWireTransport, WireResponse};

        struct OneKey(&'static str);
        impl SearchApiKeyLookup for OneKey {
            fn find_active_key(&self, _p: &str, _u: &str) -> Option<String> {
                Some(self.0.to_string())
            }
        }

        let req = build_serper_request("cats", 5, "sk");
        let key = wire_key(&req.method, &req.url, &req.body_string());
        let transport = CannedSyncWireTransport::new().with_raw_response(
            key,
            WireResponse::new(
                200,
                r#"{"organic":[{"title":"A","link":"https://a","snippet":"s"}]}"#,
            ),
        );
        let provider = RealWebSearchProvider::new(transport, OneKey("sk"), true, None);
        let out = provider.search("cats", 5, "u");
        match out {
            WebSearchOutcome::ProviderResult {
                success, results, ..
            } => {
                assert!(success);
                assert_eq!(results.len(), 1);
            }
            _ => panic!("expected provider result"),
        }

        // No key + registered → MissingApiKey (Serper Web Search display name).
        let provider2 =
            RealWebSearchProvider::new(CannedSyncWireTransport::new(), NoSearchApiKeys, true, None);
        match provider2.search("cats", 5, "u") {
            WebSearchOutcome::MissingApiKey { display_name } => {
                assert_eq!(display_name, "Serper Web Search")
            }
            _ => panic!("expected missing key"),
        }

        // Not registered + no env key → NotConfigured.
        let provider3 = RealWebSearchProvider::new(
            CannedSyncWireTransport::new(),
            NoSearchApiKeys,
            false,
            None,
        );
        assert!(matches!(
            provider3.search("cats", 5, "u"),
            WebSearchOutcome::NotConfigured
        ));
    }
}
