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
use serde_json::Value;

use crate::format_time::format_date_short_us;

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
}
