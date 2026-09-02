//! The provider-IO constructor bundle + the live pricing fetch (P4.1a
//! deliverables 3 + 5a): the one place a host assembles the model-boundary
//! drivers — the streaming composer, the non-streaming completion transport,
//! the W4.7f wire providers (image / moderation / web search), and the
//! [`PricingFetch`] HTTP impl behind the ported
//! [`PricingFetcher`](quilltap_core::services::pricing_fetcher::PricingFetcher).
//!
//! ## The pricing fetch (v4 `lib/llm/pricing-fetcher.ts`)
//!
//! Three raw HTTP calls, each returning the raw response JSON or `None` on any
//! failure (the trait contract — the ported fetcher owns the negative cache):
//!
//! - `openrouter_public_models` — v4 `fetchOpenRouterPublicPricing`: `GET
//!   https://openrouter.ai/api/v1/models` with `Content-Type: application/json`
//!   and the 3 s `AbortSignal.timeout` (`OPENROUTER_FETCH_TIMEOUT_MS = 3000`).
//! - `openrouter_sdk_models` — v4 `fetchOpenRouterPricing` via the
//!   `@openrouter/sdk` `client.models.list()`, which issues (re-surveyed at
//!   `@openrouter/sdk` **1.2.2** `funcs/modelsList.js`, P4.D33) `GET
//!   https://openrouter.ai/api/v1/models` with `Accept: application/json`,
//!   `Authorization: Bearer <key>`, `HTTP-Referer` (`BASE_URL` ||
//!   `http://localhost:3000`) and `X-OpenRouter-Title: Quilltap` — reproduced
//!   as that HTTP call. (1.2 adds an `X-OpenRouter-Categories` header, but it is
//!   `compactMap`-dropped unless the caller sets `appCategories`, which v4 does
//!   not; the first page carries no query string because v4 passes no request.
//!   The SDK's own backoff-retry config is NOT reproduced; a failure returns
//!   `None`, host policy.)
//!
//!   Two things the SDK does between that GET and v4's parse are reproduced here
//!   rather than left to the wire, because v4's parse consumes the SDK's output,
//!   not the endpoint's (see [`quilltap_core::services::pricing_fetcher`]):
//!   the **page loop** (`?limit=500&offset=N` until a short page — v4 accumulates
//!   every page) and the model-level **key remap** (snake_case wire →
//!   camelCase). Both are pure decisions in the core; this seam only supplies the
//!   HTTP.
//! - `ollama_tags` — v4 `fetchOllamaModels`: `GET {base_url}/api/tags`.
//!
//! The 3 s pricing timeout applies to all three in this host impl (v4 pins it
//! on the public fetch; the phase-4 timer inventory carries "pricing 3 s" as
//! the one knob — the SDK/ollama calls had NO timeout in v4, and unbounded
//! waits on the message-finalization path are exactly what the 3 s guard
//! exists to prevent).
//!
//! Every call runs `reqwest::blocking` on a dedicated thread (the
//! [`PricingFetch`] trait is sync and its callers sit on the async spine — see
//! [`crate::wire`] for the runtime-thread guard).

use std::time::Duration;

use quilltap_core::model::streaming_provider::{ProviderKeySource, WireStreamingProvider};
use quilltap_core::model::transport::{quilltap_user_agent, ReqwestTransport, TransportPolicy};
use quilltap_core::services::pricing_fetcher::{
    openrouter_next_page_offset, remap_openrouter_sdk_models, PricingFetch, PricingFetcher,
    OPENROUTER_PAGE_LIMIT,
};
use serde_json::Value;

use crate::wire::{
    run_off_runtime, BlockingWireTransport, ReqwestImageBytes, ReqwestWireTransport,
};

/// v4 `OPENROUTER_FETCH_TIMEOUT_MS` — the fail-fast pricing-fetch timeout
/// (cost estimation sits on the message-finalization critical path).
pub const PRICING_FETCH_TIMEOUT: Duration = Duration::from_millis(3000);

/// v4's `BASE_URL` fallback for openrouter's `HTTP-Referer`.
const DEFAULT_BASE_URL: &str = "http://localhost:3000";

/// The catalogue endpoint both openrouter pricing legs read.
const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// Issue one blocking GET (off-runtime) and parse the body as JSON when 2xx.
/// Any transport failure, timeout, non-2xx status, or unparsable body → `None`
/// (v4's `catch` → the fetch helpers return `[]` / the negative cache).
fn get_json(url: String, headers: Vec<(String, String)>) -> Option<Value> {
    let result = run_off_runtime(move || -> Option<Value> {
        let client = reqwest::blocking::Client::new();
        let mut rb = client.get(&url).timeout(PRICING_FETCH_TIMEOUT);
        for (k, v) in &headers {
            rb = rb.header(k.as_str(), v.as_str());
        }
        let resp = rb.send().ok()?;
        if !resp.status().is_success() {
            return None;
        }
        serde_json::from_str::<Value>(&resp.text().ok()?).ok()
    });
    result.ok().flatten()
}

/// The live [`PricingFetch`]: real HTTP against openrouter.ai / an Ollama host.
pub struct LivePricingFetch {
    /// v4 `process.env.BASE_URL` (the SDK path's `HTTP-Referer`).
    base_url_env: Option<String>,
}

impl LivePricingFetch {
    pub fn new(base_url_env: Option<String>) -> Self {
        Self { base_url_env }
    }

    /// Walk the SDK's model pages from `base` and hand back ONE body shaped like
    /// what v4's parse consumes: every page's models concatenated, model keys
    /// remapped snake_case → camelCase.
    ///
    /// The loop and the remap both come from the core (they are v4 behavior, not
    /// host policy); this only issues the GETs. A page that fails mid-walk ends
    /// the walk and keeps what was already collected — the same shape as a
    /// truncated catalogue rather than a total loss, and `None` only when the
    /// FIRST page fails (v4's `catch` → `[]` → the caller's fallback).
    fn openrouter_models_pages(&self, base: &str, headers: &[(String, String)]) -> Option<Value> {
        let mut collected: Vec<Value> = Vec::new();
        let mut offset: usize = 0;
        let mut url = base.to_string();
        loop {
            let page = match get_json(url, headers.to_vec()) {
                Some(page) => page,
                None if collected.is_empty() => return None,
                None => break,
            };
            if let Some(models) = page.get("data").and_then(Value::as_array) {
                collected.extend(models.iter().cloned());
            }
            match openrouter_next_page_offset(&page, offset) {
                Some(next) => {
                    // The follow-up request carries the SDK's materialized
                    // `limit` default alongside the offset (recorded at 1.2.2:
                    // `?limit=500&offset=500`); the first page carries neither.
                    url = format!("{base}?limit={OPENROUTER_PAGE_LIMIT}&offset={next}");
                    offset = next;
                }
                None => break,
            }
        }
        Some(remap_openrouter_sdk_models(
            &serde_json::json!({ "data": collected }),
        ))
    }
}

impl PricingFetch for LivePricingFetch {
    fn openrouter_public_models(&self) -> Option<Value> {
        // The PUBLIC leg is a bare `fetch` in v4 too — one page, no SDK, and the
        // parse reads the wire's own snake_case. Deliberately NOT page-looped or
        // remapped; the two legs are different code paths in v4.
        get_json(
            OPENROUTER_MODELS_URL.to_string(),
            vec![("Content-Type".to_string(), "application/json".to_string())],
        )
    }

    fn openrouter_sdk_models(&self, api_key: &str) -> Option<Value> {
        let referer = self
            .base_url_env
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("Authorization".to_string(), format!("Bearer {api_key}")),
            ("HTTP-Referer".to_string(), referer),
            ("X-OpenRouter-Title".to_string(), "Quilltap".to_string()),
        ];
        self.openrouter_models_pages(OPENROUTER_MODELS_URL, &headers)
    }

    fn ollama_tags(&self, base_url: &str) -> Option<Value> {
        get_json(format!("{base_url}/api/tags"), Vec::new())
    }
}

/// The provider-IO bundle: the shared policy / user-agent / `BASE_URL` knobs
/// plus constructors for every model-boundary driver. One per host process;
/// each constructor hands back an independent driver over a fresh pooled
/// client (cheap — `reqwest` pools per client).
pub struct ProviderIo {
    policy: TransportPolicy,
    user_agent: String,
    base_url_env: Option<String>,
    /// The container host gateway (P4.71 — v4's `resolveHostGateway()`),
    /// resolved ONCE here and injected into every provider this bundle builds.
    /// `None` on bare metal, which is the pure rewrite's no-op arm.
    localhost_gateway: Option<String>,
}

impl ProviderIo {
    /// Bundle for a host running `version`, reading `BASE_URL` from the
    /// process environment (v4's `process.env.BASE_URL`).
    pub fn new(version: &str) -> Self {
        Self::with_base_url_env(version, std::env::var("BASE_URL").ok())
    }

    /// Bundle with an explicit `BASE_URL` (tests / embedded hosts).
    pub fn with_base_url_env(version: &str, base_url_env: Option<String>) -> Self {
        Self {
            policy: TransportPolicy::default(),
            user_agent: quilltap_user_agent(version),
            base_url_env,
            // v4 resolves lazily inside each `rewriteLocalhostUrl`; v5 resolves
            // once per process and injects. `resolve_injected_gateway` keeps
            // v4's gate-then-resolve order, so a bare-metal host resolves
            // nothing and logs nothing (see `host_gateway`).
            localhost_gateway: crate::host_gateway::resolve_injected_gateway(),
        }
    }

    /// Force the injected gateway (the wiring pins; an embedded host that
    /// resolves its own). Production reads the process environment via
    /// [`with_base_url_env`](Self::with_base_url_env).
    pub fn with_localhost_gateway(mut self, gateway: Option<String>) -> Self {
        self.localhost_gateway = gateway;
        self
    }

    /// The resolved container host gateway, for the construction sites that
    /// build their providers outside this bundle (the spine's).
    pub fn localhost_gateway(&self) -> Option<String> {
        self.localhost_gateway.clone()
    }

    /// Override the transport policy (timeout / retry knobs).
    pub fn with_policy(mut self, policy: TransportPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn policy(&self) -> TransportPolicy {
        self.policy
    }

    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    pub fn base_url_env(&self) -> Option<&str> {
        self.base_url_env.as_deref()
    }

    /// The `ProviderTransport` for the non-streaming
    /// [`execute_completion`](quilltap_core::model::completion_provider::execute_completion)
    /// composition (pass `self.policy()` / `self.user_agent()` /
    /// `self.base_url_env()` alongside).
    pub fn completion_transport(&self) -> ReqwestTransport {
        ReqwestTransport::new()
    }

    /// The production streaming composer over the reqwest transport (P4.1a
    /// deliverable 1), keyed by the host-resolved provider→key source.
    pub fn streaming_provider<K: ProviderKeySource>(
        &self,
        keys: K,
    ) -> WireStreamingProvider<ReqwestTransport, K> {
        WireStreamingProvider::new(
            ReqwestTransport::new(),
            keys,
            self.policy,
            self.user_agent.clone(),
        )
        .with_base_url_env(self.base_url_env.clone())
        // P4.71: v4's `createLLMProvider(name, baseUrl)` runs the profile's
        // base URL through `rewriteLocalhostUrl` in the registry
        // (`provider-registry.ts:89`, used at `:103`). This is that site.
        .with_localhost_gateway(self.localhost_gateway.clone())
    }

    /// The async wire for the W4.7f dialects — feed it to
    /// `RealImageProvider::with_bytes_fetch` / `RealModerationProvider::new` (their
    /// constructors take the transport plus their own DB/key seams).
    pub fn wire_transport(&self) -> ReqwestWireTransport {
        ReqwestWireTransport::new()
    }

    /// The sync wire for the Serper web-search boundary — feed it to
    /// `RealWebSearchProvider::new`.
    pub fn sync_wire_transport(&self) -> BlockingWireTransport {
        BlockingWireTransport::new()
    }

    /// The image-download seam for the `ca22ec45` Z.AI URL→base64 conversion —
    /// a bare GET, no headers (see [`ReqwestImageBytes`]).
    pub fn image_bytes_fetch(&self) -> ReqwestImageBytes {
        ReqwestImageBytes::new()
    }

    /// The live image provider (the W4.7f dialect over the async wire), with the
    /// `ca22ec45` download seam wired so a Z.AI URL-only answer becomes usable
    /// base64 rather than an empty image.
    pub fn image_provider(
        &self,
    ) -> quilltap_core::model::image_dialects::RealImageProvider<
        ReqwestWireTransport,
        ReqwestImageBytes,
    > {
        quilltap_core::model::image_dialects::RealImageProvider::with_bytes_fetch(
            self.wire_transport(),
            self.image_bytes_fetch(),
        )
    }

    /// The live OpenAI moderation provider (the W4.2/W4.7f gatekeeper seam),
    /// over the caller's `Db` + api-key resolver (spine-owned seams).
    pub fn moderation_provider<
        A: quilltap_core::services::dangerous_content::provider_routing::ApiKeyResolver,
    >(
        &self,
        db: quilltap_core::db::runtime::Db,
        api_keys: A,
    ) -> quilltap_core::services::dangerous_content::moderation_wire::RealModerationProvider<
        ReqwestWireTransport,
        A,
    > {
        quilltap_core::services::dangerous_content::moderation_wire::RealModerationProvider::new(
            db,
            api_keys,
            self.wire_transport(),
        )
    }

    /// The live Serper web-search provider (the W4.1d5/W4.7f seam), over the
    /// caller's key lookup. `fallback_env_key` is the host `SERPER_API_KEY`.
    ///
    /// The `QUILLTAP_SERPER_BASE_URL` env override (P4.42) is read HERE — the one
    /// host place — so the core provider stays pure: absent (the normal case) the
    /// request goes to v4's hard-coded `SERPER_API_URL` byte-for-byte; the e2e
    /// beat sets it to point the blocking transport at an in-process mock.
    pub fn web_search_provider<K: quilltap_core::tools::web_search::SearchApiKeyLookup>(
        &self,
        key_lookup: K,
        serper_registered: bool,
        fallback_env_key: Option<String>,
    ) -> quilltap_core::tools::web_search::RealWebSearchProvider<BlockingWireTransport, K> {
        let base_url = std::env::var("QUILLTAP_SERPER_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty());
        quilltap_core::tools::web_search::RealWebSearchProvider::new(
            self.sync_wire_transport(),
            key_lookup,
            serper_registered,
            fallback_env_key,
            self.user_agent.clone(),
        )
        .with_base_url(base_url)
    }

    /// The ported pricing fetcher over the live HTTP fetch. Hold ONE per host
    /// process — the 24 h cache + 5 min negative cache are per-instance state.
    pub fn pricing_fetcher(&self) -> PricingFetcher<LivePricingFetch> {
        PricingFetcher::new(LivePricingFetch::new(self.base_url_env.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// A loopback canned responder proving the ollama-tags GET shape + JSON
    /// parse (the only [`PricingFetch`] leg with a caller-supplied host).
    #[tokio::test]
    async fn ollama_tags_hits_api_tags_and_parses() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = r#"{"models":[{"name":"llama3.2"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let seen = String::from_utf8_lossy(&buf[..n]).to_string();
            stream.write_all(response.as_bytes()).unwrap();
            seen
        });

        let fetch = LivePricingFetch::new(None);
        // Called from a runtime thread — the off-runtime guard is under test too.
        let got = fetch.ollama_tags(&format!("http://{addr}"));
        let seen = handle.join().unwrap();
        assert!(seen.starts_with("GET /api/tags"));
        assert_eq!(got.unwrap()["models"][0]["name"], "llama3.2");
    }

    /// A tiny loopback catalogue server: answers each GET with the next canned
    /// body and records the request lines it saw.
    fn serve_pages(
        bodies: Vec<String>,
    ) -> (std::net::SocketAddr, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for body in bodies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                seen.push(text.lines().next().unwrap_or_default().to_string());
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
            seen
        });
        (addr, handle)
    }

    /// The SDK page walk + key remap, end to end over real HTTP: a full page
    /// (500 rows) is followed at `?limit=500&offset=500`, a short page ends the
    /// walk, the models concatenate in order, and the snake_case wire keys land
    /// camelCase — which is what `parse_openrouter_sdk` reads. Without the walk
    /// this returns 500 of 503 models; without the remap all 503 lose their
    /// `contextLength` and `supportsTools`.
    #[test]
    fn openrouter_models_walk_pages_and_remap_keys() {
        fn page(ids: impl Iterator<Item = String>) -> String {
            let models: Vec<Value> = ids
                .map(|id| {
                    serde_json::json!({
                        "id": id,
                        "name": "m",
                        "pricing": { "prompt": "0", "completion": "0" },
                        "context_length": 4096,
                        "supported_parameters": ["tools"],
                        "architecture": { "modality": "text->text" },
                    })
                })
                .collect();
            serde_json::json!({ "data": models }).to_string()
        }
        let full = page((0..OPENROUTER_PAGE_LIMIT).map(|i| format!("full/{i}")));
        let short = page((0..3).map(|i| format!("short/{i}")));
        let (addr, handle) = serve_pages(vec![full, short]);

        let fetch = LivePricingFetch::new(None);
        let body = fetch
            .openrouter_models_pages(&format!("http://{addr}/api/v1/models"), &[])
            .expect("first page succeeded");

        let seen = handle.join().unwrap();
        assert_eq!(seen.len(), 2, "expected two requests, saw {seen:?}");
        assert_eq!(seen[0], "GET /api/v1/models HTTP/1.1");
        assert_eq!(
            seen[1], "GET /api/v1/models?limit=500&offset=500 HTTP/1.1",
            "the follow-up carries the SDK's materialized limit + offset"
        );

        let models = body["data"].as_array().unwrap();
        assert_eq!(models.len(), OPENROUTER_PAGE_LIMIT + 3);
        assert_eq!(models[0]["id"], "full/0");
        assert_eq!(models[OPENROUTER_PAGE_LIMIT]["id"], "short/0");
        // The remap is what makes the camelCase parser see the wire's values.
        assert_eq!(models[0]["contextLength"], 4096);
        assert!(models[0].get("context_length").is_none());
        assert_eq!(models[0]["supportedParameters"][0], "tools");
        assert!(models[0].get("supported_parameters").is_none());
        // Nested keys the parse reaches are passed through unrenamed.
        assert_eq!(models[0]["architecture"]["modality"], "text->text");
    }

    /// A non-2xx / refused-connection pricing fetch is `None`, not an error.
    #[test]
    fn pricing_fetch_failure_is_none() {
        let fetch = LivePricingFetch::new(None);
        // Port 9 (discard) — connection refused.
        assert!(fetch.ollama_tags("http://127.0.0.1:9").is_none());
    }

    /// Bundle construction wires the version + BASE_URL knobs through.
    #[test]
    fn bundle_constructs_drivers() {
        let io = ProviderIo::with_base_url_env("0.0.0-test", Some("https://app.example".into()));
        assert_eq!(io.user_agent(), "Quilltap/0.0.0-test");
        assert_eq!(io.base_url_env(), Some("https://app.example"));
        assert_eq!(io.policy(), TransportPolicy::default());
        // Constructors exist and typecheck.
        let _ = io.completion_transport();
        let _ = io.wire_transport();
        let _ = io.sync_wire_transport();
        let _ = io.image_provider();
        let _ = io.pricing_fetcher();
        let _ = io.streaming_provider(std::collections::HashMap::<String, String>::new());
    }
}
