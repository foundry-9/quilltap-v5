//! The OpenAI moderation wire (W4.7f) — the real [`ModerationProvider`] closing
//! the W4.2 gatekeeper seam.
//!
//! v4 `plugins/dist/qtap-plugin-openai/moderation-provider.ts` (`moderate`): the
//! ONLY registered moderation provider. `POST {base}/v1/moderations` (default
//! `https://api.openai.com`, trailing slash stripped), `Authorization: Bearer`,
//! body `{ input: content }`; an HTTP error throws `OpenAI moderation API error
//! ({status}): {errorText}`; parse `results[0]` → `{ flagged, categories }` from
//! `Object.entries(categories)` with `category_scores[cat] ?? 0`; empty results →
//! `{ flagged: false, categories: [] }`.
//!
//! The sans-IO `build_moderation_request` + `parse_moderation_response` are
//! verified independently (`moderation_wire_equivalence`); [`RealModerationProvider`]
//! composes them over the injected [`WireTransport`] + the connection-profile
//! auto-detect (`autoDetectModerationApiKey`: first `provider === 'OPENAI'`
//! profile → `apiKeyId` → the [`ApiKeyResolver`] seam, W4.7d's `db::api_keys`).

use serde_json::{Map, Value};

use crate::db::connection_profiles;
use crate::db::runtime::Db;
use crate::model::request_builder::BuiltRequest;
use crate::model::wire::{WireResponse, WireTransport};

use super::gatekeeper::{
    ModerationCategoryScore, ModerationOutcome, ModerationProvider, ModerationResult,
};
use super::provider_routing::ApiKeyResolver;

/// One provider-side category (v4 `ModerationCategoryResult` — the FULL wire shape
/// incl. the per-category `flagged` the mapper drops). Kept here for byte-exact
/// differential fidelity.
#[derive(Clone, Debug, PartialEq)]
pub struct ModerationWireCategory {
    pub category: String,
    pub flagged: bool,
    pub score: f64,
}

/// v4 `ModerationResult` as the wire yields it (the mapper consumes only `flagged`
/// + `{category, score}`).
#[derive(Clone, Debug, PartialEq)]
pub struct ModerationWireResult {
    pub flagged: bool,
    pub categories: Vec<ModerationWireCategory>,
}

impl ModerationWireResult {
    /// Project to the gatekeeper's [`ModerationResult`]. The per-category `flagged`
    /// is CARRIED THROUGH (v4's `ModerationResult` keeps it): `map_moderation_result`
    /// still never reads it, but the moderation-path `logLLMCall` serializes the raw
    /// result byte-for-byte, so the field must survive the projection (W4.11c).
    pub fn into_gatekeeper(self) -> ModerationResult {
        ModerationResult {
            flagged: self.flagged,
            categories: self
                .categories
                .into_iter()
                .map(|c| ModerationCategoryScore {
                    category: c.category,
                    flagged: c.flagged,
                    score: c.score,
                })
                .collect(),
        }
    }
}

/// v4 `moderate`'s request: `POST {base}/v1/moderations`, body `{ input }`. The
/// `Authorization` header is appended by the caller (it holds the key).
pub fn build_moderation_request(content: &str, base_url: Option<&str>) -> BuiltRequest {
    let url = match base_url.filter(|s| !s.is_empty()) {
        Some(base) => format!("{}/v1/moderations", base.trim_end_matches('/')),
        None => "https://api.openai.com/v1/moderations".to_string(),
    };
    let mut body = Map::new();
    body.insert("input".to_string(), Value::String(content.to_string()));
    BuiltRequest {
        method: "POST".to_string(),
        url,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: Value::Object(body),
    }
}

/// Parse a moderation response into a [`ModerationWireResult`], or the exact
/// thrown error string (`Err`).
pub fn parse_moderation_response(resp: &WireResponse) -> Result<ModerationWireResult, String> {
    if !resp.ok() {
        // v4: `errorText = await response.text().catch(() => 'Unknown error')`.
        let error_text = if resp.body.is_empty() {
            "Unknown error"
        } else {
            resp.body.as_str()
        };
        return Err(format!(
            "OpenAI moderation API error ({}): {}",
            resp.status, error_text
        ));
    }
    let data: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    let results = data.get("results").and_then(Value::as_array);
    let Some(result) = results.filter(|r| !r.is_empty()).and_then(|r| r.first()) else {
        // v4: empty results → `{ flagged: false, categories: [] }`.
        return Ok(ModerationWireResult {
            flagged: false,
            categories: Vec::new(),
        });
    };

    let flagged = result
        .get("flagged")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let scores = result.get("category_scores");
    let mut categories = Vec::new();
    if let Some(cats) = result.get("categories").and_then(Value::as_object) {
        // Object.entries preserves insertion order (serde_json preserve_order).
        for (category, flagged_val) in cats {
            let cat_flagged = flagged_val.as_bool().unwrap_or(false);
            // `category_scores[cat] ?? 0`.
            let score = scores
                .and_then(|s| s.get(category))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            categories.push(ModerationWireCategory {
                category: category.clone(),
                flagged: cat_flagged,
                score,
            });
        }
    }
    Ok(ModerationWireResult {
        flagged,
        categories,
    })
}

/// The real [`ModerationProvider`] (closes the W4.2 seam). Holds the [`Db`] handle
/// (the connection-profile auto-detect reads off the read pool), the
/// [`ApiKeyResolver`] seam, and the injected [`WireTransport`].
pub struct RealModerationProvider<T: WireTransport, A: ApiKeyResolver> {
    db: Db,
    api_keys: A,
    transport: T,
}

impl<T: WireTransport, A: ApiKeyResolver> RealModerationProvider<T, A> {
    pub fn new(db: Db, api_keys: A, transport: T) -> Self {
        Self {
            db,
            api_keys,
            transport,
        }
    }

    /// v4 `autoDetectModerationApiKey('OPENAI', userId)`: first `provider ===
    /// 'OPENAI'` connection profile → `apiKeyId` → the [`ApiKeyResolver`]. Any DB
    /// error / missing key → `None` (→ `NotAvailable`, falling to the LLM).
    fn detect_api_key(&self, user_id: &str) -> Option<String> {
        let api_key_id = self
            .db
            .read_main(|conn| {
                let profiles = connection_profiles::find_by_user_id(conn, user_id)?;
                Ok(profiles
                    .iter()
                    .find(|p| p.get("provider").and_then(Value::as_str) == Some("OPENAI"))
                    .and_then(|p| p.get("apiKeyId").and_then(Value::as_str))
                    .filter(|s| !s.is_empty())
                    .map(str::to_string))
            })
            .ok()
            .flatten()?;
        self.api_keys.resolve(&api_key_id, user_id)
    }
}

impl<T, A> ModerationProvider for RealModerationProvider<T, A>
where
    T: WireTransport,
    A: ApiKeyResolver + Send + Sync,
{
    async fn moderate(
        &self,
        content: &str,
        user_id: &str,
        _settings: &crate::db::chat_settings::DangerousContentSettings,
        _chat_id: Option<&str>,
    ) -> ModerationOutcome {
        // No key auto-detected → fall through to the LLM (v4's `return null`).
        let Some(api_key) = self.detect_api_key(user_id) else {
            return ModerationOutcome::NotAvailable;
        };

        let req = build_moderation_request(content, None);
        let mut headers = req.headers.clone();
        headers.push(("Authorization".to_string(), format!("Bearer {api_key}")));
        let body = req.body_string();

        match self
            .transport
            .send(&req.method, &req.url, &headers, &body)
            .await
        {
            Ok(resp) => match parse_moderation_response(&resp) {
                Ok(wire) => ModerationOutcome::Moderated {
                    result: wire.into_gatekeeper(),
                    provider_name: "OPENAI".to_string(),
                },
                // The wire throw (HTTP error) propagates to the safe fallback.
                Err(msg) => ModerationOutcome::Failed(msg),
            },
            // A network throw likewise propagates.
            Err(msg) => ModerationOutcome::Failed(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_url_default_and_base_override() {
        let r = build_moderation_request("hi", None);
        assert_eq!(r.url, "https://api.openai.com/v1/moderations");
        assert_eq!(r.body_string(), r#"{"input":"hi"}"#);
        let r2 = build_moderation_request("hi", Some("https://proxy.test/"));
        assert_eq!(r2.url, "https://proxy.test/v1/moderations");
    }

    #[test]
    fn parse_flagged_with_scores() {
        let resp = WireResponse::new(
            200,
            r#"{"id":"m","model":"x","results":[{"flagged":true,"categories":{"sexual":true,"hate":false},"category_scores":{"sexual":0.92,"hate":0.001}}]}"#,
        );
        let r = parse_moderation_response(&resp).unwrap();
        assert!(r.flagged);
        assert_eq!(r.categories.len(), 2);
        assert_eq!(r.categories[0].category, "sexual");
        assert!(r.categories[0].flagged);
        assert_eq!(r.categories[0].score, 0.92);
        assert_eq!(r.categories[1].score, 0.001);
    }

    #[test]
    fn parse_empty_results_is_clean() {
        let resp = WireResponse::new(200, r#"{"id":"m","model":"x","results":[]}"#);
        let r = parse_moderation_response(&resp).unwrap();
        assert!(!r.flagged);
        assert!(r.categories.is_empty());
    }

    #[test]
    fn parse_http_error_throws() {
        let resp = WireResponse::new(429, "rate limited");
        let err = parse_moderation_response(&resp).unwrap_err();
        assert_eq!(err, "OpenAI moderation API error (429): rate limited");
    }
}
