//! The production [`StreamingCompletionProvider`] composition (P4.1a) — the
//! streaming twin of [`completion_provider`](super::completion_provider),
//! composing the frozen sans-IO surfaces end to end:
//!
//! [`request_builder`](crate::model::request_builder) (the wire body, `stream:
//! true`) → [`transport`](crate::model::transport) (headers + auth + the byte
//! stream) → [`decoders`](crate::model::decoders) (the manifest-selected push
//! state machine) → the normalized [`StreamChunk`] sequence on a
//! [`tokio::sync::mpsc::Receiver`] — exactly the shape the primary stream /
//! failover / tool loops already consume from the canned provider.
//!
//! ## Decoder selection
//!
//! The manifest's `streamDecoder` picks the decoder; the
//! [`ChatCompletionsFlavor`] split the manifest does NOT carry (an internal
//! selector, W4.7b) is applied here: `DEEPSEEK → DeepSeek`, `Z_AI → ZAi`,
//! `OPENROUTER → OpenRouterRaw`, everything else on `chat-completions-sse` →
//! `OpenAiCompatible`. Google's decoder takes the thinking-model predicate
//! ([`google::is_thinking_model`](crate::model::request_builder::google::is_thinking_model)
//! over the call's model, matching v4 `isThinkingModel(params.model)`); ollama's
//! takes the model as its default-model echo.
//!
//! ## The pump is a plain OS thread (the core stays scheduler-free)
//!
//! No `tokio::spawn` in always-compiled code: the byte-receiver → decoder →
//! chunk-sender pump runs on a `std::thread` using `blocking_recv` /
//! `blocking_send` (the writer-thread precedent), so this module compiles and
//! unit-tests without a runtime. A transport error or a [`DecodeError`] becomes
//! an `Err(StreamError)` item on the channel — content already emitted stays
//! emitted (v4 streams can fail after yielding). At transport EOF the decoder's
//! idempotent `finish()` flushes the terminal chunk.
//!
//! ## API keys
//!
//! v4 resolves the key before the provider call (`streamMessage(params,
//! apiKey)`); the v5 seam signature carries no key, and the failover path
//! re-calls the SAME injected provider with a *different* provider id (the
//! uncensored reroute). So the composer holds an injected [`ProviderKeySource`]
//! (provider id → plaintext key) the host populates from the resolved
//! connection profiles; a missing key injects the empty string (the provider
//! rejects it upstream, surfacing as a stream error — v4's behavior with an
//! empty key). Auth injection itself is the shared
//! [`provider_auth::apply_auth`](crate::model::provider_auth) per the manifest
//! scheme.
//!
//! ## Deliberate divergence: OpenRouter always streams the raw chat-completions
//! ## wire
//!
//! v4 uses the OpenRouter SDK's **OpenResponses** protocol when no tools are
//! present — a distinct, undocumented wire W4.7b explicitly left out of scope.
//! v5 routes OPENROUTER through the raw chat-completions wire ALWAYS (the
//! differential-verified `streamViaChatCompletions` path). Documented as a
//! deliberate divergence; do not attempt the OpenResponses wire.

use std::collections::HashMap;
use std::future::Future;

use crate::model::decoders::{
    AnthropicSseDecoder, ChatCompletionsFlavor, ChatCompletionsSseDecoder, GooglePartsDecoder,
    OllamaNdjsonDecoder, ResponsesApiSseDecoder, StreamDecoder,
};
use crate::model::provider_auth::apply_auth;
use crate::model::request_builder::{build_request, google, RequestInput};
use crate::model::stream::{
    StreamChunkResult, StreamError, StreamParams, StreamingCompletionProvider,
};
use crate::model::transport::{
    transport_headers, ProviderTransport, TransportPolicy, TransportRequest,
};
use crate::provider_manifest::{rewrite_localhost_url, Registry, StreamDecoder as ManifestDecoder};

/// Provider id → plaintext api key. The host builds this from the resolved
/// connection profiles (the primary profile + any failover/reroute targets);
/// the differential injects a fixed map. A provider whose manifest auth is
/// `none` (ollama) never consults it.
pub trait ProviderKeySource: Send + Sync {
    fn key_for(&self, provider: &str) -> Option<String>;
}

impl ProviderKeySource for HashMap<String, String> {
    fn key_for(&self, provider: &str) -> Option<String> {
        self.get(provider).cloned()
    }
}

/// A single key served for every provider (the common one-profile case).
#[derive(Clone, Debug)]
pub struct SingleKey(pub String);

impl ProviderKeySource for SingleKey {
    fn key_for(&self, _provider: &str) -> Option<String> {
        Some(self.0.clone())
    }
}

/// The decoder choice for a provider — separated from construction so the
/// per-provider mapping (incl. the flavor split) is unit-testable.
#[derive(Clone, Debug, PartialEq)]
pub enum DecoderSelection {
    ChatCompletions(ChatCompletionsFlavor),
    ResponsesApi,
    Anthropic,
    /// `thinking` = v4 `isThinkingModel(params.model)`.
    Google {
        thinking: bool,
    },
    /// Carries the call's model as the decoder's default-model echo.
    Ollama {
        model: String,
    },
}

/// Select the decoder for `(provider, model)` off the manifest registry.
/// `None` for an unregistered provider.
pub fn decoder_selection(provider: &str, model: &str) -> Option<DecoderSelection> {
    let manifest = Registry::built_in().get_provider(provider)?;
    Some(match manifest.stream_decoder {
        ManifestDecoder::ChatCompletionsSse => {
            // The flavor split the manifest does not carry (W4.7b — an internal
            // selector over ONE shared parser), keyed off the one dispatch
            // table (P4.13 unit 5).
            use crate::model::provider_io::ProviderKind;
            let flavor = match ProviderKind::of(provider) {
                Some(ProviderKind::DeepSeek) => ChatCompletionsFlavor::DeepSeek,
                Some(ProviderKind::ZAi) => ChatCompletionsFlavor::ZAi,
                Some(ProviderKind::OpenRouter) => ChatCompletionsFlavor::OpenRouterRaw,
                _ => ChatCompletionsFlavor::OpenAiCompatible,
            };
            DecoderSelection::ChatCompletions(flavor)
        }
        ManifestDecoder::ResponsesApiSse => DecoderSelection::ResponsesApi,
        ManifestDecoder::AnthropicSse => DecoderSelection::Anthropic,
        ManifestDecoder::GoogleParts => DecoderSelection::Google {
            thinking: google::is_thinking_model(model),
        },
        ManifestDecoder::OllamaNdjson => DecoderSelection::Ollama {
            model: model.to_string(),
        },
    })
}

/// Construct the concrete decoder for a selection.
fn build_decoder(selection: DecoderSelection) -> Box<dyn StreamDecoder + Send> {
    match selection {
        DecoderSelection::ChatCompletions(flavor) => {
            Box::new(ChatCompletionsSseDecoder::new(flavor))
        }
        DecoderSelection::ResponsesApi => Box::new(ResponsesApiSseDecoder::new()),
        DecoderSelection::Anthropic => Box::new(AnthropicSseDecoder::new()),
        DecoderSelection::Google { thinking } => Box::new(GooglePartsDecoder::new(thinking)),
        DecoderSelection::Ollama { model } => Box::new(OllamaNdjsonDecoder::new(model)),
    }
}

/// Build the provider-agnostic [`RequestInput`] from the streaming call's
/// [`StreamParams`] (`stream: true`; the streaming path sets more fields than
/// the completion twin's `request_input_from_params` — tools / top_p / stop /
/// web search / cache key / previous_response_id all flow through).
fn request_input_from_stream_params(params: &StreamParams) -> RequestInput {
    RequestInput {
        model: params.model.clone(),
        // The carrying enum flows through UNCONVERTED (P4.13 unit 5) — there is
        // no boundary left at which a field could be dropped (finding #25's
        // structural cause was exactly a conversion here).
        messages: params.messages.clone(),
        temperature: params.temperature,
        max_tokens: params.max_tokens,
        top_p: params.top_p,
        stop: if params.stop.is_empty() {
            None
        } else {
            Some(params.stop.clone())
        },
        // v4 `tools?: unknown[]` — carried on StreamParams as an opaque JSON
        // array value; `None`/non-array → absent (v4 sends `undefined`).
        tools: params.tools.as_ref().and_then(|v| v.as_array().cloned()),
        tool_choice: None,
        response_format: None,
        web_search_enabled: params.web_search_enabled,
        profile_parameters: params.profile_parameters.clone(),
        cache_key: params.cache_key.clone(),
        previous_response_id: params.previous_response_id.clone(),
        strict_max_tokens: false,
        stream: true,
    }
}

/// The production streaming provider: request builder → transport → decoder →
/// [`StreamChunk`](crate::model::stream::StreamChunk) channel. Generic over the
/// transport (the feature-gated
/// [`ReqwestTransport`](crate::model::transport::ReqwestTransport) in
/// production; a fake in tests) and the key source.
pub struct WireStreamingProvider<T: ProviderTransport, K: ProviderKeySource> {
    transport: T,
    keys: K,
    policy: TransportPolicy,
    user_agent: String,
    /// v4 `process.env.BASE_URL` (openrouter's `HTTP-Referer`).
    base_url_env: Option<String>,
    /// The host's container/VM gateway for `rewrite_localhost_url` (W4.7a — v4
    /// resolves it host-side; `None` on a bare-metal host).
    localhost_gateway: Option<String>,
}

impl<T: ProviderTransport, K: ProviderKeySource> WireStreamingProvider<T, K> {
    pub fn new(transport: T, keys: K, policy: TransportPolicy, user_agent: String) -> Self {
        Self {
            transport,
            keys,
            policy,
            user_agent,
            base_url_env: None,
            localhost_gateway: None,
        }
    }

    /// Set v4's `BASE_URL` env (openrouter `HTTP-Referer`).
    pub fn with_base_url_env(mut self, base_url_env: Option<String>) -> Self {
        self.base_url_env = base_url_env;
        self
    }

    /// Set the localhost-rewrite gateway (container hosts).
    pub fn with_localhost_gateway(mut self, gateway: Option<String>) -> Self {
        self.localhost_gateway = gateway;
        self
    }

    /// The composed transport (the tool-wire call-site pin reads a recording
    /// fake back out after the loop has run).
    pub fn transport_ref(&self) -> &T {
        &self.transport
    }

    /// Build the finalized [`TransportRequest`] + the decoder for one call.
    /// Split out so the request construction is unit-testable without a
    /// transport round-trip.
    fn prepare(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &StreamParams,
    ) -> Result<(TransportRequest, Box<dyn StreamDecoder + Send>), StreamError> {
        let registry = Registry::built_in();
        let manifest = registry
            .get_provider(provider)
            .ok_or_else(|| StreamError::new(format!("unknown provider: {provider}")))?;

        let input = request_input_from_stream_params(params);
        let built = build_request(provider, &input)
            .map_err(|e| StreamError::new(format!("request build: {e}")))?;

        // A profile baseUrl overrides the manifest base (v4
        // `createLLMProvider(provider, baseUrl)` → the SDK client's baseURL),
        // localhost-rewritten per the registry's `resolveBaseUrl`.
        let mut url = match base_url.filter(|b| !b.is_empty()) {
            Some(base) => {
                let base = rewrite_localhost_url(base, self.localhost_gateway.as_deref());
                match built.url.strip_prefix(manifest.base_url.as_str()) {
                    Some(rest) => format!("{base}{rest}"),
                    None => built.url.clone(),
                }
            }
            None => built.url.clone(),
        };

        let api_key = self.keys.key_for(provider).unwrap_or_default();
        let mut headers = transport_headers(
            provider,
            &built.headers,
            &self.user_agent,
            self.base_url_env.as_deref(),
        );
        apply_auth(registry, provider, &api_key, &mut headers, &mut url);

        let selection = decoder_selection(provider, &params.model)
            .ok_or_else(|| StreamError::new(format!("unknown provider: {provider}")))?;

        Ok((
            TransportRequest {
                provider: provider.to_string(),
                method: built.method.clone(),
                url,
                headers,
                body: built.body_string().into_bytes(),
                api_key,
            },
            build_decoder(selection),
        ))
    }
}

/// A receiver pre-loaded with a single error item (a failure before the first
/// chunk — v4's generator throwing before any yield).
fn single_error(message: String) -> tokio::sync::mpsc::Receiver<StreamChunkResult> {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    // The receiver is live (just created); try_send cannot fail on capacity 1.
    let _ = tx.try_send(Err(StreamError::new(message)));
    rx
}

impl<T: ProviderTransport, K: ProviderKeySource> StreamingCompletionProvider
    for WireStreamingProvider<T, K>
{
    fn stream_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        // Prepare synchronously; the async part is only the transport call.
        let prepared = self.prepare(provider, base_url, params);
        async move {
            let (request, mut decoder) = match prepared {
                Ok(p) => p,
                Err(e) => return single_error(e.message),
            };

            let bytes_rx = match self.transport.execute_stream(&request, &self.policy).await {
                Ok(rx) => rx,
                Err(e) => return single_error(e.message),
            };

            let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunkResult>(32);

            // The pump: a plain OS thread (no scheduler in the core). It owns
            // the byte receiver + decoder; a dropped consumer ends it via the
            // failed blocking_send.
            let mut bytes_rx = bytes_rx;
            std::thread::spawn(move || {
                loop {
                    match bytes_rx.blocking_recv() {
                        Some(Ok(bytes)) => match decoder.push(&bytes) {
                            Ok(chunks) => {
                                for chunk in chunks {
                                    if tx.blocking_send(Ok(chunk)).is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(e) => {
                                // v4's generator throws here — no finish().
                                let _ = tx.blocking_send(Err(StreamError::new(e.message)));
                                return;
                            }
                        },
                        Some(Err(te)) => {
                            let _ = tx.blocking_send(Err(StreamError::new(te.message)));
                            return;
                        }
                        // Transport EOF.
                        None => break,
                    }
                }
                match decoder.finish() {
                    Ok(chunks) => {
                        for chunk in chunks {
                            if tx.blocking_send(Ok(chunk)).is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.blocking_send(Err(StreamError::new(e.message)));
                    }
                }
            });

            rx
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::transport::{BoxFuture, StreamBytes, TransportError, TransportResponse};
    use std::sync::Mutex;

    /// A transport replaying scripted byte frames (or a pre-stream failure),
    /// recording the request it saw.
    struct FakeStreamTransport {
        frames: Vec<StreamBytes>,
        fail_before_stream: Option<String>,
        seen: Mutex<Option<TransportRequest>>,
    }

    impl FakeStreamTransport {
        fn new(frames: Vec<StreamBytes>) -> Self {
            Self {
                frames,
                fail_before_stream: None,
                seen: Mutex::new(None),
            }
        }
    }

    impl ProviderTransport for FakeStreamTransport {
        fn execute<'a>(
            &'a self,
            _request: &'a TransportRequest,
            _policy: &'a TransportPolicy,
        ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
            Box::pin(async move {
                Err(TransportError {
                    message: "non-streaming not scripted".to_string(),
                    status: None,
                })
            })
        }
        fn execute_stream<'a>(
            &'a self,
            request: &'a TransportRequest,
            _policy: &'a TransportPolicy,
        ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>>
        {
            *self.seen.lock().unwrap() = Some(request.clone());
            let frames = self.frames.clone();
            let fail = self.fail_before_stream.clone();
            Box::pin(async move {
                if let Some(message) = fail {
                    return Err(TransportError {
                        message,
                        status: None,
                    });
                }
                let (tx, rx) = tokio::sync::mpsc::channel(frames.len().max(1));
                for f in frames {
                    let _ = tx.send(f).await;
                }
                Ok(rx)
            })
        }
    }

    fn params(model: &str) -> StreamParams {
        StreamParams {
            messages: vec![crate::model::stream::StreamMessage::user("hi")],
            model: model.to_string(),
            temperature: Some(0.7),
            max_tokens: Some(1024),
            top_p: None,
            tools: None,
            web_search_enabled: false,
            profile_parameters: None,
            cache_key: None,
            previous_response_id: None,
            stop: Vec::new(),
        }
    }

    async fn drain(
        mut rx: tokio::sync::mpsc::Receiver<StreamChunkResult>,
    ) -> Vec<StreamChunkResult> {
        let mut out = Vec::new();
        while let Some(item) = rx.recv().await {
            out.push(item);
        }
        out
    }

    fn keys() -> HashMap<String, String> {
        let mut m = HashMap::new();
        for p in [
            "ANTHROPIC",
            "OPENAI",
            "GOOGLE",
            "GROK",
            "DEEPSEEK",
            "Z_AI",
            "OPENROUTER",
            "OPENAI_COMPATIBLE",
        ] {
            m.insert(p.to_string(), format!("synthetic-{}", p.to_lowercase()));
        }
        m
    }

    /// Decoder/flavor selection for all nine providers.
    #[test]
    fn decoder_selection_all_nine() {
        use ChatCompletionsFlavor as F;
        assert_eq!(
            decoder_selection("DEEPSEEK", "deepseek-chat"),
            Some(DecoderSelection::ChatCompletions(F::DeepSeek))
        );
        assert_eq!(
            decoder_selection("Z_AI", "glm-4.6"),
            Some(DecoderSelection::ChatCompletions(F::ZAi))
        );
        assert_eq!(
            decoder_selection("OPENROUTER", "anthropic/claude-sonnet-4"),
            Some(DecoderSelection::ChatCompletions(F::OpenRouterRaw))
        );
        assert_eq!(
            decoder_selection("OPENAI_COMPATIBLE", "m"),
            Some(DecoderSelection::ChatCompletions(F::OpenAiCompatible))
        );
        assert_eq!(
            decoder_selection("OPENAI", "gpt-5.2"),
            Some(DecoderSelection::ResponsesApi)
        );
        assert_eq!(
            decoder_selection("GROK", "grok-4"),
            Some(DecoderSelection::ResponsesApi)
        );
        assert_eq!(
            decoder_selection("ANTHROPIC", "claude-sonnet-4-5"),
            Some(DecoderSelection::Anthropic)
        );
        // Google carries the thinking-model predicate over the call's model.
        assert_eq!(
            decoder_selection("GOOGLE", "gemini-3-pro-preview"),
            Some(DecoderSelection::Google { thinking: true })
        );
        assert_eq!(
            decoder_selection("GOOGLE", "gemini-2.0-flash"),
            Some(DecoderSelection::Google { thinking: false })
        );
        assert_eq!(
            decoder_selection("OLLAMA", "llama3.2"),
            Some(DecoderSelection::Ollama {
                model: "llama3.2".to_string()
            })
        );
        assert_eq!(decoder_selection("NOPE", "m"), None);
    }

    /// Auth injection per manifest scheme through the streaming path: bearer
    /// (deepseek), named header (anthropic), query param (google), none
    /// (ollama). The User-Agent rides every request.
    #[tokio::test]
    async fn auth_injection_per_manifest_scheme() {
        // Bearer.
        let t = FakeStreamTransport::new(vec![]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let _ = drain(
            p.stream_message("DEEPSEEK", None, &params("deepseek-chat"))
                .await,
        )
        .await;
        let seen = p.transport.seen.lock().unwrap().clone().unwrap();
        assert!(seen
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer synthetic-deepseek"));
        assert!(seen
            .headers
            .iter()
            .any(|(k, v)| k == "User-Agent" && v == "Quilltap/test"));

        // Named header (anthropic x-api-key).
        let t = FakeStreamTransport::new(vec![]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let _ = drain(
            p.stream_message("ANTHROPIC", None, &params("claude-sonnet-4-5"))
                .await,
        )
        .await;
        let seen = p.transport.seen.lock().unwrap().clone().unwrap();
        assert!(seen
            .headers
            .iter()
            .any(|(k, v)| k == "x-api-key" && v == "synthetic-anthropic"));

        // Query param (google).
        let t = FakeStreamTransport::new(vec![]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let _ = drain(
            p.stream_message("GOOGLE", None, &params("gemini-2.0-flash"))
                .await,
        )
        .await;
        let seen = p.transport.seen.lock().unwrap().clone().unwrap();
        assert!(
            seen.url.contains("key=synthetic-google"),
            "url was {}",
            seen.url
        );
        assert!(seen.url.contains(":streamGenerateContent?alt=sse"));

        // None (ollama): no auth header even without a registered key.
        let t = FakeStreamTransport::new(vec![]);
        let p = WireStreamingProvider::new(
            t,
            HashMap::new(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let _ = drain(p.stream_message("OLLAMA", None, &params("llama3.2")).await).await;
        let seen = p.transport.seen.lock().unwrap().clone().unwrap();
        assert!(!seen
            .headers
            .iter()
            .any(|(k, _)| k == "Authorization" || k == "x-api-key"));
    }

    /// A profile baseUrl override swaps the manifest base prefix.
    #[tokio::test]
    async fn base_url_override_swaps_manifest_base() {
        let t = FakeStreamTransport::new(vec![]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let _ = drain(
            p.stream_message(
                "OPENAI_COMPATIBLE",
                Some("https://my-proxy.example/v1"),
                &params("m"),
            )
            .await,
        )
        .await;
        let seen = p.transport.seen.lock().unwrap().clone().unwrap();
        assert!(
            seen.url.starts_with("https://my-proxy.example/v1/"),
            "url was {}",
            seen.url
        );
    }

    /// A complete SSE stream decodes through the composer: content deltas + the
    /// terminal done chunk, and the request body carried `stream: true`.
    #[tokio::test]
    async fn decodes_a_chat_completions_stream_end_to_end() {
        let wire = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hel\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
            "data: [DONE]\n\n",
        );
        let t = FakeStreamTransport::new(vec![Ok(wire.as_bytes().to_vec())]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(
            p.stream_message("OPENAI_COMPATIBLE", None, &params("m"))
                .await,
        )
        .await;
        let seen = p.transport.seen.lock().unwrap().clone().unwrap();
        let body: serde_json::Value = serde_json::from_slice(&seen.body).unwrap();
        assert_eq!(body["stream"], serde_json::json!(true));

        let chunks: Vec<_> = items.into_iter().map(|r| r.unwrap()).collect();
        let text: String = chunks.iter().map(|c| c.content.as_str()).collect();
        assert_eq!(text, "Hello");
        let last = chunks.last().unwrap();
        assert!(last.done);
        assert_eq!(last.usage.unwrap().total_tokens, 5);
    }

    /// A mid-stream transport error becomes an `Err` item AFTER the chunks
    /// already emitted.
    #[tokio::test]
    async fn mid_stream_transport_error_is_an_err_item() {
        let frame = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"}}]}\n\n";
        let t = FakeStreamTransport::new(vec![
            Ok(frame.as_bytes().to_vec()),
            Err(TransportError {
                message: "connection reset".to_string(),
                status: None,
            }),
        ]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(
            p.stream_message("OPENAI_COMPATIBLE", None, &params("m"))
                .await,
        )
        .await;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_ref().unwrap().content, "partial");
        assert_eq!(items[1].as_ref().unwrap_err().message, "connection reset");
    }

    /// A pre-stream transport failure is a single `Err` item.
    #[tokio::test]
    async fn pre_stream_failure_is_a_single_error() {
        let mut t = FakeStreamTransport::new(vec![]);
        t.fail_before_stream = Some("HTTP 401: unauthorized".to_string());
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(
            p.stream_message("OPENAI_COMPATIBLE", None, &params("m"))
                .await,
        )
        .await;
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].as_ref().unwrap_err().message,
            "HTTP 401: unauthorized"
        );
    }

    /// An unknown provider fails loud (a single error item, no transport call).
    #[tokio::test]
    async fn unknown_provider_is_a_single_error() {
        let t = FakeStreamTransport::new(vec![]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(p.stream_message("NOPE", None, &params("m")).await).await;
        assert_eq!(items.len(), 1);
        assert!(items[0]
            .as_ref()
            .unwrap_err()
            .message
            .contains("unknown provider"));
        assert!(p.transport.seen.lock().unwrap().is_none());
    }

    /// Transport EOF drives the decoder's `finish()` exactly once: a
    /// responses-API stream with no `response.completed` frame still yields
    /// exactly ONE terminal chunk from `finish()` (the decoder's idempotence,
    /// proven through the full compose path — a double flush would duplicate
    /// the done chunk and fail the count).
    #[tokio::test]
    async fn eof_flushes_finish_exactly_once() {
        let wire = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n",
        );
        let t = FakeStreamTransport::new(vec![Ok(wire.as_bytes().to_vec())]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(p.stream_message("OPENAI", None, &params("gpt-5.2")).await).await;
        let done_count = items
            .iter()
            .filter(|r| r.as_ref().map(|c| c.done).unwrap_or(false))
            .count();
        assert_eq!(done_count, 1, "exactly one terminal chunk");
        assert_eq!(items[0].as_ref().unwrap().content, "hi");
    }
}
