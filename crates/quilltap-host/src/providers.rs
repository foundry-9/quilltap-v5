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
//!   `@openrouter/sdk` `client.models.list()`, which issues (surveyed at
//!   `@openrouter/sdk` `funcs/modelsList.js`) `GET
//!   https://openrouter.ai/api/v1/models` with `Accept: application/json`,
//!   `Authorization: Bearer <key>`, `HTTP-Referer` (`BASE_URL` ||
//!   `http://localhost:3000`) and `X-OpenRouter-Title: Quilltap` — reproduced
//!   as that HTTP call. (The SDK's own backoff-retry config is NOT reproduced;
//!   a failure returns `None`, host policy.)
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
use quilltap_core::services::pricing_fetcher::{PricingFetch, PricingFetcher};
use serde_json::Value;

use crate::wire::{run_off_runtime, BlockingWireTransport, ReqwestWireTransport};

/// v4 `OPENROUTER_FETCH_TIMEOUT_MS` — the fail-fast pricing-fetch timeout
/// (cost estimation sits on the message-finalization critical path).
pub const PRICING_FETCH_TIMEOUT: Duration = Duration::from_millis(3000);

/// v4's `BASE_URL` fallback for openrouter's `HTTP-Referer`.
const DEFAULT_BASE_URL: &str = "http://localhost:3000";

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
}

impl PricingFetch for LivePricingFetch {
    fn openrouter_public_models(&self) -> Option<Value> {
        get_json(
            "https://openrouter.ai/api/v1/models".to_string(),
            vec![("Content-Type".to_string(), "application/json".to_string())],
        )
    }

    fn openrouter_sdk_models(&self, api_key: &str) -> Option<Value> {
        let referer = self
            .base_url_env
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        get_json(
            "https://openrouter.ai/api/v1/models".to_string(),
            vec![
                ("Accept".to_string(), "application/json".to_string()),
                ("Authorization".to_string(), format!("Bearer {api_key}")),
                ("HTTP-Referer".to_string(), referer),
                ("X-OpenRouter-Title".to_string(), "Quilltap".to_string()),
            ],
        )
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
        }
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
    }

    /// The async wire for the W4.7f dialects — feed it to
    /// `RealImageProvider::new` / `RealModerationProvider::new` (their
    /// constructors take the transport plus their own DB/key seams).
    pub fn wire_transport(&self) -> ReqwestWireTransport {
        ReqwestWireTransport::new()
    }

    /// The sync wire for the Serper web-search boundary — feed it to
    /// `RealWebSearchProvider::new`.
    pub fn sync_wire_transport(&self) -> BlockingWireTransport {
        BlockingWireTransport::new()
    }

    /// The live image provider (the W4.7f dialect over the async wire).
    pub fn image_provider(
        &self,
    ) -> quilltap_core::model::image_dialects::RealImageProvider<ReqwestWireTransport> {
        quilltap_core::model::image_dialects::RealImageProvider::new(self.wire_transport())
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
    pub fn web_search_provider<K: quilltap_core::tools::web_search::SearchApiKeyLookup>(
        &self,
        key_lookup: K,
        serper_registered: bool,
        fallback_env_key: Option<String>,
    ) -> quilltap_core::tools::web_search::RealWebSearchProvider<BlockingWireTransport, K> {
        quilltap_core::tools::web_search::RealWebSearchProvider::new(
            self.sync_wire_transport(),
            key_lookup,
            serper_registered,
            fallback_env_key,
        )
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
