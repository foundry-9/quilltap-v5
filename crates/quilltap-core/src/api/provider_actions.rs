//! The provider-actions driver (the P4.6 unification wire) — the live wire
//! behind the four Settings wire actions P4.6d engine-gated: connection test
//! (v4 `?action=test-connection`), test message (`?action=test-message`),
//! API-key test (`api-keys/[id]?action=test`), and the live models fetch
//! (`POST /models`).
//!
//! The HANDLERS live in [`super::settings`] and are differential-verified over
//! injected seams ([`settings::ConnectionValidator`](super::settings::ConnectionValidator)
//! / [`CompletionProvider`](crate::model::completion::CompletionProvider) /
//! [`settings::ModelsFetcher`](super::settings::ModelsFetcher)). This module
//! supplies (a) the dyn-erased [`ProviderActionsDriver`] the engine holds
//! (the [`ChatSendDriver`](super::chat_send::ChatSendDriver) precedent —
//! only the composing host can construct the wire, so the engine holds the
//! driver type-erased per assembly and refuses when absent), and (b) the LIVE
//! seam impls composed over the sans-IO provider layer (the W4.7f
//! `Real*Provider` precedent: the composition lives in core over the
//! [`SyncWireTransport`](crate::model::wire::SyncWireTransport) seam; the host
//! only plugs its reqwest transport).
//!
//! ## The live wire, surveyed from v4 at `a7b1398d`
//!
//! `validateApiKey` per plugin:
//!   - the OpenAI-SDK family (OPENAI_COMPATIBLE / GROK / DEEPSEEK / Z_AI /
//!     OPENROUTER): `client.models.list()` succeeds → true (the shared
//!     provider base class; `requireApiKey && !apiKey` → false first);
//!   - OPENAI: a `POST /v1/moderations {"input":"test"}` probe → `response.ok`;
//!   - ANTHROPIC: a minimal `messages.create` (claude-haiku-4-5-20251001,
//!     `max_tokens: 1`, "test") → 2xx;
//!   - GOOGLE: a minimal `generateContent` (gemini-2.5-flash, "test") → 2xx;
//!   - OLLAMA: `GET /api/tags` → `response.ok` (no key).
//!
//! Every plugin catches wire errors → `false` (never throws), so the
//! validator returns `Ok(false)` on failure, never `Err`.
//!
//! `getAvailableModels`: the models-list GET (the ported
//! [`models_list_request`](crate::model::provider_models_api::models_list_request)
//! and [`parse_models_list`](crate::model::provider_models_api::parse_models_list)).
//! Every plugin catches wire failures → `[]` EXCEPT anthropic, whose catch
//! returns a static fallback list (transcribed below verbatim). An UNKNOWN
//! provider is the one `Err` (v4 `createLLMProvider` throws → the route 500s).
//!
//! **Documented divergence (named):** v4's `modelsWithInfo` enriches each id
//! from the plugin's `getModelsWithMetadata` / `getModelMetadata` /
//! `getModelInfo` — per-plugin STATIC/dynamic metadata that is not manifest
//! data and is not ported. The live fetcher emits `{"id": <id>}` rows only
//! (optional fields absent, matching v4's `undefined`-dropped serialization
//! for providers without metadata getters); the SPA's ModelSelector consumes
//! the id list. Consequence: the fetch-time cache write (which requires
//! `displayName`) stays empty — the same net effect as v4's metadata-less
//! providers, whose `displayName: undefined` rows fail the cache upsert's
//! validation and are warn-swallowed.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::model::provider_auth::{apply_auth, declared_auth_extras_for};
use crate::model::provider_models_api::{models_list_request, parse_models_list};
use crate::model::request_builder::{build_request, RequestInput, StreamMessage};
use crate::model::transport::transport_headers;
use crate::model::wire::SyncWireTransport;
use crate::provider_manifest::Registry;

use super::settings::{self, ConnectionValidator, ModelsFetcher};
use super::types::{ErrorKind, Response};

/// The boxed future a [`ProviderActionsDriver`] method returns (dyn-compat —
/// the engine stores the driver type-erased).
pub type ProviderActionsFuture<'a> = Pin<Box<dyn Future<Output = Response> + Send + 'a>>;

/// The dyn-erased seam the engine holds per assembly. Each method delegates to
/// the differential-verified [`super::settings`] handler with the driver's own
/// live seams; the `profile` bags are v4's request bodies verbatim
/// (`{provider, apiKeyId?, baseUrl?}` / `+ {modelName, parameters?}`).
pub trait ProviderActionsDriver: Send + Sync {
    fn connection_test(&self, profile: Value) -> ProviderActionsFuture<'_>;
    fn connection_test_message(&self, profile: Value) -> ProviderActionsFuture<'_>;
    fn api_key_test(
        &self,
        user_id: String,
        api_key_id: String,
        base_url: Option<String>,
    ) -> ProviderActionsFuture<'_>;
    fn model_fetch(
        &self,
        user_id: String,
        provider: String,
        api_key_id: Option<String>,
        base_url: Option<String>,
    ) -> ProviderActionsFuture<'_>;
}

/// v4 anthropic `getAvailableModels`'s catch-branch static fallback list
/// (`AnthropicProvider.getAvailableModels`, transcribed verbatim at `a7b1398d`).
const ANTHROPIC_FALLBACK_MODELS: [&str; 11] = [
    "claude-opus-4-6",
    "claude-sonnet-4-6",
    "claude-opus-4-5-20251101",
    "claude-sonnet-4-5-20250929",
    "claude-haiku-4-5-20251001",
    "claude-opus-4-1-20250805",
    "claude-sonnet-4-20250514",
    "claude-opus-4-20250514",
    "claude-3-7-sonnet-20250219",
    "claude-3-5-haiku-20241022",
    "claude-3-haiku-20240307",
];

/// Join a base url and an endpoint path without doubling the slash.
fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

/// The effective base for a provider probe: a non-empty override wins, else the
/// manifest `baseUrl`. `None` when the provider is unknown to the registry.
fn effective_base(registry: &Registry, provider: &str, base_url: Option<&str>) -> Option<String> {
    match base_url.filter(|b| !b.is_empty()) {
        Some(b) => Some(b.to_string()),
        None => registry.get_provider(provider).map(|m| m.base_url.clone()),
    }
}

/// A GET/POST probe returning whether the wire answered 2xx. Any transport
/// error is `false` (v4's plugin catch → `false`).
fn probe_ok<T: SyncWireTransport>(
    transport: &T,
    method: &str,
    url: String,
    headers: Vec<(String, String)>,
    body: &str,
) -> bool {
    matches!(
        transport.send(method, &url, &headers, body),
        Ok(resp) if (200..300).contains(&resp.status)
    )
}

/// The live [`ConnectionValidator`] over a [`SyncWireTransport`] — v4's
/// per-plugin `validateApiKey` matrix (module docs).
pub struct WireConnectionValidator<'a, T> {
    pub transport: &'a T,
    pub user_agent: &'a str,
    pub base_url_env: Option<&'a str>,
}

impl<T: SyncWireTransport> ConnectionValidator for WireConnectionValidator<'_, T> {
    fn validate(
        &self,
        provider: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<bool, String> {
        let registry = Registry::built_in();
        Ok(match provider {
            "OLLAMA" => {
                let Some(base) = effective_base(registry, provider, base_url) else {
                    return Ok(false);
                };
                let headers = transport_headers(provider, &[], self.user_agent, self.base_url_env);
                probe_ok(
                    self.transport,
                    "GET",
                    join_url(&base, "/api/tags"),
                    headers,
                    "",
                )
            }
            "OPENAI" => {
                // v4: `baseUrl ? baseUrl.replace(/\/$/, '') + '/v1/moderations'
                // : 'https://api.openai.com/v1/moderations'` — note the RAW
                // override is used (no manifest fallback) and the path carries
                // its own `/v1`.
                let url = match base_url.filter(|b| !b.is_empty()) {
                    Some(b) => format!("{}/v1/moderations", b.trim_end_matches('/')),
                    None => "https://api.openai.com/v1/moderations".to_string(),
                };
                let mut headers =
                    vec![("Content-Type".to_string(), "application/json".to_string())];
                headers.push(("Authorization".to_string(), format!("Bearer {api_key}")));
                headers.push(("User-Agent".to_string(), self.user_agent.to_string()));
                probe_ok(self.transport, "POST", url, headers, "{\"input\":\"test\"}")
            }
            "ANTHROPIC" => self.minimal_completion_probe(
                registry,
                provider,
                "claude-haiku-4-5-20251001",
                Some(1),
                api_key,
                base_url,
            ),
            "GOOGLE" => self.minimal_completion_probe(
                registry,
                provider,
                "gemini-2.5-flash",
                None,
                api_key,
                base_url,
            ),
            // The OpenAI-SDK family + any other manifest provider: the shared
            // base-class `validateApiKey` — the requireApiKey guard, then
            // `client.models.list()` success.
            _ => {
                if settings::requires_api_key(provider) && api_key.is_empty() {
                    return Ok(false);
                }
                let Some(base) = effective_base(registry, provider, base_url) else {
                    return Ok(false);
                };
                let req = models_list_request(provider);
                let mut url = join_url(&base, req.path);
                // The manifest's fixed auth extras, which a bare wire call has
                // no request builder to supply (see the models fetcher below).
                let extras = declared_auth_extras_for(registry, provider);
                let mut headers =
                    transport_headers(provider, &extras, self.user_agent, self.base_url_env);
                apply_auth(registry, provider, api_key, &mut headers, &mut url);
                probe_ok(self.transport, req.method, url, headers, "")
            }
        })
    }
}

impl<T: SyncWireTransport> WireConnectionValidator<'_, T> {
    /// The anthropic/google validate probe: the ported request builder emits
    /// the byte-exact minimal wire body (v4's SDK `messages.create` /
    /// `generateContent` with a one-word "test"), sent non-streaming; 2xx →
    /// valid. A build failure (unknown provider) or wire error → `false`.
    fn minimal_completion_probe(
        &self,
        registry: &Registry,
        provider: &str,
        model: &str,
        max_tokens: Option<i64>,
        api_key: &str,
        base_url: Option<&str>,
    ) -> bool {
        let input = RequestInput {
            model: model.to_string(),
            messages: vec![StreamMessage::user("test")],
            max_tokens,
            stream: false,
            ..Default::default()
        };
        let Ok(built) = build_request(provider, &input) else {
            return false;
        };
        // A profile baseUrl override re-roots the built url on the override
        // (the completion provider's swap, simplified — the manifest base is
        // the built url's prefix).
        let mut url = match base_url.filter(|b| !b.is_empty()) {
            Some(base) => match registry
                .get_provider(provider)
                .and_then(|m| built.url.strip_prefix(m.base_url.as_str()))
            {
                Some(rest) => format!("{}{rest}", base.trim_end_matches('/')),
                None => built.url.clone(),
            },
            None => built.url.clone(),
        };
        let mut headers =
            transport_headers(provider, &built.headers, self.user_agent, self.base_url_env);
        apply_auth(registry, provider, api_key, &mut headers, &mut url);
        let body = built.body_string();
        probe_ok(self.transport, &built.method, url, headers, &body)
    }
}

/// The live [`ModelsFetcher`] over a [`SyncWireTransport`] — v4's per-plugin
/// `getAvailableModels` (module docs; wire failures → `[]`, anthropic's → the
/// static fallback, unknown provider → `Err`).
pub struct WireModelsFetcher<'a, T> {
    pub transport: &'a T,
    pub user_agent: &'a str,
    pub base_url_env: Option<&'a str>,
}

impl<T: SyncWireTransport> ModelsFetcher for WireModelsFetcher<'_, T> {
    fn fetch(
        &self,
        provider: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<(Vec<String>, Vec<Value>), String> {
        let registry = Registry::built_in();
        if !registry.has_provider(provider) {
            // v4 `createLLMProvider(provider, ...)` throws on an unknown
            // provider → the route 500s.
            return Err(format!("Unknown provider: {provider}"));
        }
        let base = effective_base(registry, provider, base_url).unwrap_or_default();
        let req = models_list_request(provider);
        let mut url = join_url(&base, req.path);
        // v4 lists models through the vendor SDK, which carries the provider's
        // fixed headers on every call; this path builds the request by hand, so
        // the manifest's `auth.extra` has to come from the registry. Without
        // anthropic's `anthropic-version` the GET 400s, the catch below answers
        // the static fallback, and Fetch Models silently reports 11 stale ids
        // instead of the account's real catalogue (dogfood #89).
        let extras = declared_auth_extras_for(registry, provider);
        let mut headers = transport_headers(provider, &extras, self.user_agent, self.base_url_env);
        apply_auth(registry, provider, api_key, &mut headers, &mut url);

        let models: Vec<String> = match self.transport.send(req.method, &url, &headers, "") {
            Ok(resp) if (200..300).contains(&resp.status) => {
                match serde_json::from_str::<Value>(&resp.body) {
                    Ok(body) => parse_models_list(provider, &body),
                    Err(_) => Vec::new(),
                }
            }
            // Wire/HTTP failure: v4's plugin catch — anthropic falls back to
            // its static list, everyone else returns [].
            _ if provider == "ANTHROPIC" => ANTHROPIC_FALLBACK_MODELS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            _ => Vec::new(),
        };
        let models_with_info: Vec<Value> = models.iter().map(|id| json!({ "id": id })).collect();
        Ok((models, models_with_info))
    }
}

/// The production driver: the live seams over an owned transport + completion
/// provider, holding its assembly's [`Db`] (the
/// [`ChatSpine`](super::chat_send::ChatSendDriver)-holds-its-own-Db precedent).
pub struct RealProviderActions<T, C> {
    pub db: Db,
    pub transport: T,
    pub completion: C,
    pub user_agent: String,
    /// v4 `process.env.BASE_URL` (the openrouter Referer header).
    pub base_url_env: Option<String>,
}

/// Pull the shared `{provider, apiKeyId?, baseUrl?}` fields off a v4 test bag.
/// A missing/empty `provider` mirrors the Zod `min(1)` refusal.
fn parse_test_bag(profile: &Value) -> Result<(String, Option<String>, Option<String>), Response> {
    let provider = profile
        .get("provider")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| Response::error(ErrorKind::BadRequest, "Provider is required"))?;
    let api_key_id = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let base_url = profile
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok((provider, api_key_id, base_url))
}

impl<T, C> ProviderActionsDriver for RealProviderActions<T, C>
where
    T: SyncWireTransport + Send + Sync + 'static,
    C: crate::model::completion::CompletionProvider + Send + Sync + 'static,
{
    fn connection_test(&self, profile: Value) -> ProviderActionsFuture<'_> {
        Box::pin(async move {
            let (provider, api_key_id, base_url) = match parse_test_bag(&profile) {
                Ok(v) => v,
                Err(r) => return r,
            };
            let validator = WireConnectionValidator {
                transport: &self.transport,
                user_agent: &self.user_agent,
                base_url_env: self.base_url_env.as_deref(),
            };
            settings::connection_test(
                &self.db,
                &provider,
                api_key_id.as_deref(),
                base_url.as_deref(),
                &validator,
            )
        })
    }

    fn connection_test_message(&self, profile: Value) -> ProviderActionsFuture<'_> {
        Box::pin(async move {
            let (provider, api_key_id, base_url) = match parse_test_bag(&profile) {
                Ok(v) => v,
                Err(r) => return r,
            };
            let Some(model_name) = profile
                .get("modelName")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|m| !m.is_empty())
            else {
                return Response::error(ErrorKind::BadRequest, "Model name is required");
            };
            let parameters = profile.get("parameters").cloned().unwrap_or(json!({}));
            settings::connection_test_message(
                &self.db,
                &provider,
                api_key_id.as_deref(),
                base_url.as_deref(),
                &model_name,
                &parameters,
                &self.completion,
            )
            .await
        })
    }

    fn api_key_test(
        &self,
        user_id: String,
        api_key_id: String,
        base_url: Option<String>,
    ) -> ProviderActionsFuture<'_> {
        Box::pin(async move {
            let validator = WireConnectionValidator {
                transport: &self.transport,
                user_agent: &self.user_agent,
                base_url_env: self.base_url_env.as_deref(),
            };
            settings::api_key_test(
                &self.db,
                &user_id,
                &api_key_id,
                base_url.as_deref(),
                &validator,
            )
            .await
        })
    }

    fn model_fetch(
        &self,
        user_id: String,
        provider: String,
        api_key_id: Option<String>,
        base_url: Option<String>,
    ) -> ProviderActionsFuture<'_> {
        Box::pin(async move {
            let fetcher = WireModelsFetcher {
                transport: &self.transport,
                user_agent: &self.user_agent,
                base_url_env: self.base_url_env.as_deref(),
            };
            settings::model_fetch(
                &self.db,
                &user_id,
                &provider,
                api_key_id.as_deref(),
                base_url.as_deref(),
                &fetcher,
            )
            .await
        })
    }
}

/// Convenience alias for the engine's per-assembly slot.
pub type SharedProviderActions = Arc<dyn ProviderActionsDriver>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::wire::WireResponse;
    use std::sync::Mutex;

    /// Records requests; answers from a scripted (status, body) queue.
    /// `(method, url, body, headers)`. The headers slot was `_headers` until
    /// dogfood #89 — which is precisely why a models GET that had never carried
    /// `anthropic-version` passed every test in this module: the fake wire threw
    /// away the one field the bug lived in.
    type SeenCall = (String, String, String, Vec<(String, String)>);

    struct ScriptedWire {
        seen: Mutex<Vec<SeenCall>>,
        replies: Mutex<Vec<Result<WireResponse, String>>>,
    }

    impl ScriptedWire {
        fn new(replies: Vec<Result<WireResponse, String>>) -> Self {
            Self {
                seen: Mutex::new(Vec::new()),
                replies: Mutex::new(replies),
            }
        }
    }

    impl SyncWireTransport for ScriptedWire {
        fn send(
            &self,
            method: &str,
            url: &str,
            headers: &[(String, String)],
            body: &str,
        ) -> Result<WireResponse, String> {
            self.seen.lock().unwrap().push((
                method.to_string(),
                url.to_string(),
                body.to_string(),
                headers.to_vec(),
            ));
            let mut replies = self.replies.lock().unwrap();
            if replies.is_empty() {
                Err("no scripted reply".to_string())
            } else {
                replies.remove(0)
            }
        }
    }

    #[test]
    fn sdk_family_validate_is_a_models_list_get() {
        let wire = ScriptedWire::new(vec![Ok(WireResponse::new(200, "{\"data\":[]}"))]);
        let v = WireConnectionValidator {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        assert_eq!(
            v.validate("OPENAI_COMPATIBLE", "sk-x", Some("http://127.0.0.1:9/v1")),
            Ok(true)
        );
        let seen = wire.seen.lock().unwrap();
        assert_eq!(seen[0].0, "GET");
        assert_eq!(seen[0].1, "http://127.0.0.1:9/v1/models");
    }

    #[test]
    fn sdk_family_requires_key_guard_short_circuits() {
        let wire = ScriptedWire::new(vec![]);
        let v = WireConnectionValidator {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        // GROK requires a key; empty key → false with NO wire call.
        assert_eq!(v.validate("GROK", "", None), Ok(false));
        assert!(wire.seen.lock().unwrap().is_empty());
        // OPENAI_COMPATIBLE does not require one → the wire IS consulted.
        assert_eq!(v.validate("OPENAI_COMPATIBLE", "", None), Ok(false));
        assert_eq!(wire.seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn openai_validate_is_the_moderations_probe() {
        let wire = ScriptedWire::new(vec![Ok(WireResponse::new(200, "{}"))]);
        let v = WireConnectionValidator {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        assert_eq!(v.validate("OPENAI", "sk-x", None), Ok(true));
        let seen = wire.seen.lock().unwrap();
        assert_eq!(seen[0].0, "POST");
        assert_eq!(seen[0].1, "https://api.openai.com/v1/moderations");
        assert_eq!(seen[0].2, "{\"input\":\"test\"}");
    }

    #[test]
    fn wire_failure_is_valid_false_not_err() {
        let wire = ScriptedWire::new(vec![Err("connection refused".to_string())]);
        let v = WireConnectionValidator {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        assert_eq!(v.validate("OPENAI_COMPATIBLE", "k", None), Ok(false));
    }

    #[test]
    fn models_fetch_parses_and_shapes_info_rows() {
        let wire = ScriptedWire::new(vec![Ok(WireResponse::new(
            200,
            "{\"data\":[{\"id\":\"mock-model\"},{\"id\":\"other\"}]}",
        ))]);
        let f = WireModelsFetcher {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        let (models, info) = f
            .fetch("OPENAI_COMPATIBLE", "", Some("http://127.0.0.1:9/v1"))
            .unwrap();
        assert_eq!(models, vec!["mock-model".to_string(), "other".to_string()]);
        assert_eq!(info[0], json!({"id": "mock-model"}));
    }

    #[test]
    fn anthropic_fetch_failure_falls_back_to_the_static_list() {
        let wire = ScriptedWire::new(vec![Err("refused".to_string())]);
        let f = WireModelsFetcher {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        let (models, _) = f.fetch("ANTHROPIC", "sk-ant", None).unwrap();
        assert_eq!(models.len(), 11);
        assert_eq!(models[0], "claude-opus-4-6");
    }

    /// Dogfood #89. v4 lists anthropic's models through the SDK, which sends
    /// `anthropic-version` on every call; v5 builds the GET by hand, and without
    /// the manifest's `auth.extra` the API answers 400 — so the live catalogue
    /// was unreachable and the catch-branch fallback above answered every time.
    /// Both halves are asserted: the header goes out ONCE, and a 200 body is
    /// actually parsed (a green fallback list is what the bug looked like).
    #[test]
    fn anthropic_models_fetch_carries_the_version_header() {
        let wire = ScriptedWire::new(vec![Ok(WireResponse::new(
            200,
            "{\"data\":[{\"id\":\"claude-live-1\"}]}",
        ))]);
        let f = WireModelsFetcher {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        let (models, _) = f.fetch("ANTHROPIC", "sk-ant", None).unwrap();
        assert_eq!(models, vec!["claude-live-1".to_string()]);

        let seen = wire.seen.lock().unwrap();
        assert_eq!(seen[0].0, "GET");
        assert_eq!(seen[0].1, "https://api.anthropic.com/v1/models");
        let versions: Vec<&String> = seen[0]
            .3
            .iter()
            .filter(|(k, _)| k == "anthropic-version")
            .map(|(_, v)| v)
            .collect();
        assert_eq!(versions, vec!["2023-06-01"], "sent once, with v4's value");
        assert!(
            seen[0]
                .3
                .iter()
                .any(|(k, v)| k == "x-api-key" && v == "sk-ant"),
            "the key header still rides alongside the extras"
        );
    }

    /// The guard on the fix: extras are the manifest's, not a blanket anthropic
    /// header bolted onto every provider.
    #[test]
    fn a_provider_declaring_no_extras_gets_none() {
        let wire = ScriptedWire::new(vec![Ok(WireResponse::new(200, "{\"data\":[]}"))]);
        let f = WireModelsFetcher {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        f.fetch("OPENAI_COMPATIBLE", "k", Some("http://127.0.0.1:9/v1"))
            .unwrap();
        let seen = wire.seen.lock().unwrap();
        assert!(!seen[0].3.iter().any(|(k, _)| k == "anthropic-version"));
    }

    #[test]
    fn unknown_provider_fetch_is_the_one_err() {
        let wire = ScriptedWire::new(vec![]);
        let f = WireModelsFetcher {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        assert!(f.fetch("NO_SUCH", "", None).is_err());
    }

    #[test]
    fn ollama_validate_hits_api_tags_without_auth() {
        let wire = ScriptedWire::new(vec![Ok(WireResponse::new(200, "{\"models\":[]}"))]);
        let v = WireConnectionValidator {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        assert_eq!(v.validate("OLLAMA", "", None), Ok(true));
        let seen = wire.seen.lock().unwrap();
        assert_eq!(seen[0].1, "http://localhost:11434/api/tags");
    }

    #[test]
    fn anthropic_validate_is_a_minimal_messages_create() {
        let wire = ScriptedWire::new(vec![Ok(WireResponse::new(200, "{}"))]);
        let v = WireConnectionValidator {
            transport: &wire,
            user_agent: "ua",
            base_url_env: None,
        };
        assert_eq!(v.validate("ANTHROPIC", "sk-ant", None), Ok(true));
        let seen = wire.seen.lock().unwrap();
        assert_eq!(seen[0].0, "POST");
        assert!(seen[0].1.starts_with("https://api.anthropic.com/v1"));
        let body: Value = serde_json::from_str(&seen[0].2).unwrap();
        assert_eq!(body["model"], "claude-haiku-4-5-20251001");
        assert_eq!(body["max_tokens"], 1);
    }
}
