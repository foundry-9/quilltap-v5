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
                Some(ProviderKind::NanoGpt) => ChatCompletionsFlavor::NanoGpt,
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
    /// The host's container gateway for `rewrite_localhost_url` (W4.7a — v4
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
    #[allow(clippy::type_complexity)]
    fn prepare(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &StreamParams,
    ) -> Result<
        (
            TransportRequest,
            Box<dyn StreamDecoder + Send>,
            crate::model::stream::StreamAttachmentResults,
        ),
        StreamError,
    > {
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
            built.attachment_results,
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
        // P4.D83 (v4 `d89babc4`): this call's wall-clock budget. A provider that
        // offers the setting (Ollama) takes its number from the PROFILE — a
        // better default, so the retry count stands — and a caller-supplied
        // ceiling still wins and forbids retrying past itself. On the streaming
        // path the budget bounds time-to-headers only (module header), which is
        // v4's `openStream()` verbatim: the `AbortController` is armed INSIDE it,
        // so each attempt (including the think-retry below) re-arms the timer.
        let policy = self
            .policy
            .with_provider_default_timeout(
                crate::model::request_builder::provider_profile_timeout_ms(
                    provider,
                    params.profile_parameters.as_ref(),
                ),
            )
            .with_request_budget(params.request_timeout_ms);
        // The conversation-chaining fallback (dogfood #69). v4's OpenAI provider
        // wraps the chained `responses.create` in a try/catch: when the request
        // carried `previous_response_id` and the create fails BEFORE the first
        // chunk (`previous_response_not_found` is routine on the `store: false`
        // responses both apps send), it logs and re-issues ONCE with the full
        // conversation and no chaining ("conversation chaining failed, falling
        // back to full input", `provider.ts:463-533`). Restore that best-effort
        // fallback: build the full-input request up front (so the Err arm can
        // retry without a second borrow of the call args). Rebuilding with the id
        // set to `None` is byte-identical to a never-chained request — only
        // OpenAI's builder reads the id, so this prep is inert for every other
        // provider and is only ever CONSUMED on a pre-stream failure of a chained
        // request.
        let fallback_prepared = if params.previous_response_id.is_some() {
            let mut fallback_params = params.clone();
            fallback_params.previous_response_id = None;
            Some(self.prepare(provider, base_url, &fallback_params))
        } else {
            None
        };
        async move {
            let (request, mut decoder, mut attachment_results) = match prepared {
                Ok(p) => p,
                Err(e) => return single_error(e.message),
            };

            let bytes_rx = match self.transport.execute_stream(&request, &policy).await {
                Ok(rx) => rx,
                Err(e) => {
                    // P4.D78 (v4 `d9c5a1c7`): an Ollama model that refuses the
                    // `think` parameter gets ONE retry with the key deleted.
                    // Re-calling `execute_stream` re-arms the first-byte timer
                    // per attempt, which is v4's `openStream()` refactor
                    // verbatim (the `AbortController` is built inside it). This
                    // arm is provider-disjoint from the chaining fallback below
                    // — only OPENAI chains, only OLLAMA carries `think`.
                    if let Some(retry) = crate::model::ollama_think_retry::think_retry_request(
                        provider, &request, &e,
                    ) {
                        tracing::warn!(
                            target: "quilltap::model::streaming_provider",
                            provider = %request.provider,
                            error = %e.message,
                            "Ollama rejected the think parameter; retrying without it"
                        );
                        match self.transport.execute_stream(&retry, &policy).await {
                            Ok(rx) => rx,
                            // v4 surfaces the SECOND failure (the retry's error).
                            Err(retry_err) => return single_error(retry_err.message),
                        }
                    } else {
                        // Pre-stream failure. If this was a chained request, retry once
                        // with the full input (the fallback); a plain pre-stream failure
                        // with no chaining keeps today's single-error behavior
                        // byte-for-byte.
                        let fallback = match fallback_prepared {
                            Some(fb) => fb,
                            None => return single_error(e.message),
                        };
                        let (fallback_request, fallback_decoder, fallback_attachment_results) =
                            match fallback {
                                Ok(p) => p,
                                // The full-input build can only fail where the chained
                                // build already did (and that returned above), but stay
                                // loud rather than swallow.
                                Err(pe) => return single_error(pe.message),
                            };
                        tracing::warn!(
                            target: "quilltap::model::streaming_provider",
                            provider = %request.provider,
                            "Conversation chaining failed, falling back to full input"
                        );
                        match self
                            .transport
                            .execute_stream(&fallback_request, &policy)
                            .await
                        {
                            Ok(rx) => {
                                // Swap in the full-input build's decoder + attachment
                                // report for the pump below.
                                decoder = fallback_decoder;
                                attachment_results = fallback_attachment_results;
                                rx
                            }
                            // v4 surfaces the SECOND failure (the retry's error).
                            Err(retry_err) => return single_error(retry_err.message),
                        }
                    }
                }
            };

            let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunkResult>(32);

            // The pump: a plain OS thread (no scheduler in the core). It owns
            // the byte receiver + decoder; a dropped consumer ends it via the
            // failed blocking_send.
            let mut bytes_rx = bytes_rx;
            std::thread::spawn(move || {
                // v4's plugins attach the format-time `attachmentResults` to the
                // chunks they yield; the provider-agnostic decoders stamp an
                // EMPTY `Some(..)` on the final chunk, so the builder's real
                // report replaces exactly those (P4.21).
                let stamp = move |mut chunk: crate::model::stream::StreamChunk| {
                    if chunk.attachment_results.is_some() {
                        chunk.attachment_results = Some(attachment_results.clone());
                    }
                    chunk
                };
                loop {
                    match bytes_rx.blocking_recv() {
                        Some(Ok(bytes)) => match decoder.push(&bytes) {
                            Ok(chunks) => {
                                for chunk in chunks {
                                    if tx.blocking_send(Ok(stamp(chunk))).is_err() {
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
                            if tx.blocking_send(Ok(stamp(chunk))).is_err() {
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
    use std::time::Duration;

    /// A transport replaying scripted byte frames (or a pre-stream failure),
    /// recording the request it saw.
    struct FakeStreamTransport {
        frames: Vec<StreamBytes>,
        fail_before_stream: Option<String>,
        seen: Mutex<Option<TransportRequest>>,
        /// P4.D83: the policy `execute_stream` was handed — the composed per-call
        /// budget, recorded so the resolution is proven without racing a clock.
        seen_policy: Mutex<Option<TransportPolicy>>,
    }

    impl FakeStreamTransport {
        fn new(frames: Vec<StreamBytes>) -> Self {
            Self {
                frames,
                fail_before_stream: None,
                seen: Mutex::new(None),
                seen_policy: Mutex::new(None),
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
            policy: &'a TransportPolicy,
        ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>>
        {
            *self.seen.lock().unwrap() = Some(request.clone());
            *self.seen_policy.lock().unwrap() = Some(*policy);
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

    /// A transport that scripts a per-call outcome (a pre-stream failure or a
    /// scripted stream) and records EVERY request it sees — the chaining-fallback
    /// tests need two calls with different outcomes and both bodies read back
    /// (the single-shot [`FakeStreamTransport`] keeps only the last).
    struct ScriptedStreamTransport {
        // Front-popped per call: `Err(message)` = pre-stream failure; `Ok(frames)`
        // = a scripted byte stream.
        outcomes: Mutex<std::collections::VecDeque<Result<Vec<StreamBytes>, TransportError>>>,
        seen: Mutex<Vec<TransportRequest>>,
    }

    impl ScriptedStreamTransport {
        fn new(outcomes: Vec<Result<Vec<StreamBytes>, String>>) -> Self {
            Self::typed(
                outcomes
                    .into_iter()
                    .map(|o| {
                        o.map_err(|message| TransportError {
                            message,
                            status: None,
                        })
                    })
                    .collect(),
            )
        }

        /// Outcomes carrying a real [`TransportError`] — the think-retry tests
        /// need `status: Some(..)` (v4's `!response.ok`), which a bare message
        /// cannot express.
        fn typed(outcomes: Vec<Result<Vec<StreamBytes>, TransportError>>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes.into()),
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    impl ProviderTransport for ScriptedStreamTransport {
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
            self.seen.lock().unwrap().push(request.clone());
            let outcome = self.outcomes.lock().unwrap().pop_front();
            Box::pin(async move {
                match outcome {
                    Some(Err(e)) => Err(e),
                    Some(Ok(frames)) => {
                        let (tx, rx) = tokio::sync::mpsc::channel(frames.len().max(1));
                        for f in frames {
                            let _ = tx.send(f).await;
                        }
                        Ok(rx)
                    }
                    None => Err(TransportError {
                        message: "scripted transport: no outcome queued".to_string(),
                        status: None,
                    }),
                }
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
            request_timeout_ms: None,
        }
    }

    /// [`params`] for OpenAI's responses API with a conversation-chaining id set.
    fn openai_params_with_prev(prev: Option<&str>) -> StreamParams {
        let mut p = params("gpt-4o");
        p.previous_response_id = prev.map(|s| s.to_string());
        p
    }

    /// One responses-API content delta; EOF drives the decoder's terminal chunk.
    fn responses_api_stream(text: &str) -> Vec<StreamBytes> {
        let frame = format!(
            "event: response.output_text.delta\ndata: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\n",
            serde_json::Value::String(text.to_string())
        );
        vec![Ok(frame.into_bytes())]
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

        // Header (google `x-goog-api-key`). This arm asserted the `?key=` query
        // param until P4.47 (B) proved v4's genai SDK sends a header and leaves
        // the url alone (module header of `provider_auth`); the url assertion
        // below is now the WHOLE url v4 records, key-free.
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
        assert!(seen
            .headers
            .iter()
            .any(|(k, v)| k == "x-goog-api-key" && v == "synthetic-google"));
        assert!(
            !seen.url.contains("key=synthetic-google"),
            "the key must not ALSO ride in the url; it was {}",
            seen.url
        );
        assert!(seen.url.ends_with(":streamGenerateContent?alt=sse"));

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

    /// P4.21: the builder's format-time `attachmentResults` replace the
    /// decoder's empty stamp on the FINAL chunk — a DeepSeek stream with an
    /// attachment reports the drop-and-report failure, exactly what v4's
    /// plugin attaches to the chunks it yields.
    #[tokio::test]
    async fn final_chunk_carries_the_builders_attachment_results() {
        let wire = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let t = FakeStreamTransport::new(vec![Ok(wire.as_bytes().to_vec())]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let mut prms = params("deepseek-chat");
        prms.messages = vec![crate::model::stream::StreamMessage::User {
            content: "What is in this image?".to_string(),
            cache_control: None,
            attachments: vec![serde_json::json!({
                "id": "att-1",
                "filename": "photo.png",
                "mimeType": "image/png",
                "size": 8,
                "data": "aGVsbG8h",
            })],
        }];
        let items = drain(p.stream_message("DEEPSEEK", None, &prms).await).await;
        let chunks: Vec<_> = items.into_iter().map(|r| r.unwrap()).collect();
        let last = chunks.last().unwrap();
        assert!(last.done);
        let results = last
            .attachment_results
            .as_ref()
            .expect("final chunk must carry attachment results");
        assert!(results.sent.is_empty());
        assert_eq!(results.failed.len(), 1);
        assert_eq!(results.failed[0].id, "att-1");
        assert_eq!(
            results.failed[0].error,
            "DeepSeek models do not accept file attachments. Send text-only messages."
        );
        // And the body stripped the attachment (plain string content).
        let seen = p.transport.seen.lock().unwrap().clone().unwrap();
        let body: serde_json::Value = serde_json::from_slice(&seen.body).unwrap();
        assert_eq!(body["messages"][0]["content"], "What is in this image?");
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

    /// The chaining fallback (finding #69): a chained OpenAI request whose stream
    /// fails pre-stream retries ONCE with the full input and no
    /// `previous_response_id`, and the retry's stream proceeds.
    #[tokio::test]
    async fn chaining_fallback_retries_with_full_input() {
        let mut prms = openai_params_with_prev(Some("resp_dead"));
        // Two user turns: the chained request sends only the LAST; the full-input
        // retry sends both — so the retry's `input` array is strictly larger.
        prms.messages = vec![
            crate::model::stream::StreamMessage::system("You are Byron."),
            crate::model::stream::StreamMessage::user("first"),
            crate::model::stream::StreamMessage::user("second"),
        ];
        let t = ScriptedStreamTransport::new(vec![
            Err("HTTP 400: previous_response_not_found".to_string()),
            Ok(responses_api_stream("Hi")),
        ]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(p.stream_message("OPENAI", None, &prms).await).await;

        // The stream proceeds: content emitted + a terminal done chunk.
        let chunks: Vec<_> = items.iter().map(|r| r.as_ref().unwrap()).collect();
        let text: String = chunks.iter().map(|c| c.content.as_str()).collect();
        assert_eq!(text, "Hi");
        assert!(chunks.last().unwrap().done, "the retry stream terminates");

        // Exactly two transport calls: the chained attempt, then the full-input retry.
        let seen = p.transport.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2, "expected chained + full-input retry");

        let chained: serde_json::Value = serde_json::from_slice(&seen[0].body).unwrap();
        assert_eq!(
            chained["previous_response_id"], "resp_dead",
            "the first attempt chains"
        );
        let chained_input_len = chained["input"].as_array().unwrap().len();

        let retry: serde_json::Value = serde_json::from_slice(&seen[1].body).unwrap();
        assert!(
            retry.get("previous_response_id").is_none(),
            "the retry drops the chaining id"
        );
        let retry_input = retry["input"].as_array().unwrap();
        assert!(
            retry_input.len() > chained_input_len,
            "the retry carries the FULL input (both turns), not just the last user message"
        );
    }

    /// Both attempts fail → a single error carrying the RETRY's message (v4
    /// surfaces the second failure), exactly two transport calls.
    #[tokio::test]
    async fn chaining_fallback_both_fail_is_single_error() {
        let prms = openai_params_with_prev(Some("resp_dead"));
        let t = ScriptedStreamTransport::new(vec![
            Err("HTTP 400: previous_response_not_found".to_string()),
            Err("HTTP 500: server error".to_string()),
        ]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(p.stream_message("OPENAI", None, &prms).await).await;
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].as_ref().unwrap_err().message,
            "HTTP 500: server error"
        );
        assert_eq!(
            p.transport.seen.lock().unwrap().len(),
            2,
            "one retry, then stop"
        );
    }

    // ------------------------------------------------------------------
    // P4.D78 — the Ollama retry-without-`think` quartet (streaming half).
    //
    // v4 wraps the stream OPEN in the salvage, so each attempt re-arms the
    // first-byte timer (its `openStream()` refactor). Note the fourth arm:
    // v4's own suite proves a `think: false` body DOES retry (its guard is
    // `'think' in requestBody`, key presence — not truthiness), so the arm
    // that must not fire is the provider scope, not the flag's value.
    // ------------------------------------------------------------------

    fn ollama_params() -> StreamParams {
        params("qwen3:8b")
    }

    fn ollama_stream(text: &str) -> Vec<StreamBytes> {
        vec![Ok(format!(
            "{{\"model\":\"qwen3:8b\",\"message\":{{\"role\":\"assistant\",\"content\":\"{text}\"}},\"done\":false}}\n\
             {{\"model\":\"qwen3:8b\",\"message\":{{\"role\":\"assistant\",\"content\":\"\"}},\"done\":true}}\n"
        )
        .into_bytes())]
    }

    // -----------------------------------------------------------------------
    // P4.D83 — the per-profile request timeout (v4 `d89babc4`, Ollama 1.0.42)
    //
    // Not oracle-checkable (the P4.15 ruling: a corpus cannot observe wall
    // clock), so the quartet is unit-tier. The first three read the policy the
    // transport was HANDED — deterministic, no clock — and the fourth proves the
    // number actually bounds a real socket, so a composition that resolved
    // correctly and then dropped the result cannot pass.
    // -----------------------------------------------------------------------

    fn ollama_params_with(bag: serde_json::Value) -> StreamParams {
        StreamParams {
            profile_parameters: Some(bag),
            ..ollama_params()
        }
    }

    async fn policy_for(params: &StreamParams) -> TransportPolicy {
        let t = FakeStreamTransport::new(ollama_stream("hi"));
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let _ = drain(p.stream_message("OLLAMA", None, params).await).await;
        let seen = *p.transport.seen_policy.lock().unwrap();
        seen.expect("execute_stream was called")
    }

    /// The profile's number becomes the streaming call's budget — and ONLY the
    /// budget: it is a better default, not a caller's ceiling, so the retry
    /// count stands (v4 passes it as `resolveRequestTimeoutMs`'s `defaultMs`).
    #[tokio::test]
    async fn a_profile_timeout_sets_the_streaming_budget_and_keeps_retries() {
        let policy = policy_for(&ollama_params_with(
            serde_json::json!({ "request_timeout_seconds": 900 }),
        ))
        .await;
        assert_eq!(policy.timeout, Duration::from_secs(900));
        assert_eq!(
            policy.max_retries,
            TransportPolicy::default().max_retries,
            "a provider-side default must not disable retries"
        );
    }

    /// Blank, absent, unparseable and non-positive all fall through, leaving the
    /// shared 300 s default (v4 `DEFAULT_REQUEST_TIMEOUT_SECONDS`) untouched.
    /// The string form is accepted because a hand-edited bag carries strings.
    #[tokio::test]
    async fn an_unusable_profile_timeout_falls_through_to_the_default() {
        for bag in [
            serde_json::json!({}),
            serde_json::json!({ "request_timeout_seconds": "" }),
            serde_json::json!({ "request_timeout_seconds": "soon" }),
            serde_json::json!({ "request_timeout_seconds": 0 }),
            serde_json::json!({ "request_timeout_seconds": -30 }),
            serde_json::json!({ "request_timeout_seconds": null }),
            serde_json::json!({ "request_timeout_seconds": true }),
        ] {
            let policy = policy_for(&ollama_params_with(bag.clone())).await;
            assert_eq!(
                policy,
                TransportPolicy::default(),
                "bag {bag} must leave the policy alone"
            );
        }
        // …and the string form of a real number IS honoured.
        let policy = policy_for(&ollama_params_with(
            serde_json::json!({ "request_timeout_seconds": "45" }),
        ))
        .await;
        assert_eq!(policy.timeout, Duration::from_secs(45));
    }

    /// A caller-supplied budget still WINS, and (being a ceiling on one attempt)
    /// forbids retrying past itself — the cheap-LLM task deadlines stay hard.
    /// Also: only OLLAMA offers the setting, so the same bag on another provider
    /// changes nothing.
    #[tokio::test]
    async fn a_caller_budget_beats_the_profile_and_other_providers_ignore_it() {
        let mut capped = ollama_params_with(serde_json::json!({ "request_timeout_seconds": 900 }));
        capped.request_timeout_ms = Some(20_000);
        let policy = policy_for(&capped).await;
        assert_eq!(policy.timeout, Duration::from_millis(20_000));
        assert_eq!(policy.max_retries, 0);

        let t = FakeStreamTransport::new(vec![]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let deepseek = StreamParams {
            profile_parameters: Some(serde_json::json!({ "request_timeout_seconds": 900 })),
            ..params("deepseek-chat")
        };
        let _ = drain(p.stream_message("DEEPSEEK", None, &deepseek).await).await;
        assert_eq!(
            p.transport.seen_policy.lock().unwrap().unwrap(),
            TransportPolicy::default(),
            "no other provider reads request_timeout_seconds at the pin"
        );
    }

    /// The fourth arm, and the only one that touches a socket: a profile budget
    /// that RESOLVES correctly and is then dropped on the floor would pass the
    /// three assertions above. Against an endpoint that accepts the connection
    /// and never answers, the stream must fail at the profile's number rather
    /// than hold the turn for the shared 300 s default.
    ///
    /// Feature-gated with the concrete transport, like `transport.rs`'s own
    /// deadline proofs; `cargo test --workspace` compiles core with
    /// `native-transport` (quilltap-host requires it and cargo unifies features),
    /// so this runs in the ordinary gate.
    #[cfg(feature = "native-transport")]
    #[tokio::test]
    async fn a_profile_timeout_actually_bounds_the_wire() {
        use tokio::io::AsyncReadExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = sock.read(&mut buf).await;
                    // Hold it open, silently, forever.
                    std::future::pending::<()>().await;
                });
            }
        });

        let provider = WireStreamingProvider::new(
            crate::model::transport::ReqwestTransport::new(),
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let params = ollama_params_with(serde_json::json!({ "request_timeout_seconds": 0.2 }));
        let started = std::time::Instant::now();
        let chunks = drain(
            provider
                .stream_message("OLLAMA", Some(&format!("http://{addr}")), &params)
                .await,
        )
        .await;
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the profile's 0.2 s budget must fire, not the 300 s default (took {:?})",
            started.elapsed()
        );
        assert!(
            chunks.iter().any(|c| c.is_err()),
            "a silent endpoint must surface as a stream error, got {chunks:?}"
        );
    }

    fn think_rejection() -> TransportError {
        TransportError {
            message: r#"HTTP 400: {"error":"\"qwen3:8b\" does not support disabling thinking"}"#
                .to_string(),
            status: Some(400),
        }
    }

    /// Arm 1 — rejected, then retried WITHOUT `think`, and the stream proceeds.
    #[tokio::test]
    async fn think_rejection_retries_once_without_think() {
        let t = ScriptedStreamTransport::typed(vec![
            Err(think_rejection()),
            Ok(ollama_stream("hello")),
        ]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(p.stream_message("OLLAMA", None, &ollama_params()).await).await;
        let chunks: Vec<_> = items.iter().map(|r| r.as_ref().unwrap()).collect();
        assert_eq!(
            chunks
                .iter()
                .map(|c| c.content.as_str())
                .collect::<String>(),
            "hello"
        );
        assert!(chunks.last().unwrap().done);

        let seen = p.transport.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2, "expected the original attempt + one retry");
        let first: serde_json::Value = serde_json::from_slice(&seen[0].body).unwrap();
        // The default profile sends `think: false` — and v4 retries anyway,
        // because its guard is key PRESENCE.
        assert_eq!(first["think"], serde_json::json!(false));
        let retry: serde_json::Value = serde_json::from_slice(&seen[1].body).unwrap();
        assert!(
            retry.get("think").is_none(),
            "the retry deletes the think key"
        );
        // Nothing else moves: the retry is the same body minus one key.
        assert_eq!(first["model"], retry["model"]);
        assert_eq!(first["options"], retry["options"]);
        assert_eq!(seen[0].url, seen[1].url);
    }

    /// Arm 2 — both attempts rejected: one error carrying the SECOND message,
    /// and exactly two calls (the retry body has no `think` left to delete, so
    /// the salvage cannot loop).
    #[tokio::test]
    async fn think_rejection_twice_is_a_single_error() {
        let t = ScriptedStreamTransport::typed(vec![
            Err(think_rejection()),
            Err(TransportError {
                message: "HTTP 500: still thinking about it".to_string(),
                status: Some(500),
            }),
        ]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(p.stream_message("OLLAMA", None, &ollama_params()).await).await;
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].as_ref().unwrap_err().message,
            "HTTP 500: still thinking about it"
        );
        assert_eq!(p.transport.seen.lock().unwrap().len(), 2);
    }

    /// Arm 3 — a think-UNRELATED error never retries (v4's `isThinkRejection`).
    #[tokio::test]
    async fn a_non_think_error_never_retries() {
        let t = ScriptedStreamTransport::typed(vec![
            Err(TransportError {
                message: r#"HTTP 404: {"error":"model \"nope\" not found"}"#.to_string(),
                status: Some(404),
            }),
            Ok(ollama_stream("unreachable")),
        ]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(p.stream_message("OLLAMA", None, &ollama_params()).await).await;
        assert_eq!(items.len(), 1);
        assert!(items[0].as_ref().unwrap_err().message.contains("404"));
        assert_eq!(p.transport.seen.lock().unwrap().len(), 1);
    }

    /// Arm 4 — the salvage is Ollama-only. A think-mentioning failure on another
    /// provider surfaces unchanged (that provider's body has no `think` key and
    /// v4's code does not exist outside the Ollama plugin).
    #[tokio::test]
    async fn the_salvage_is_ollama_only() {
        let t = ScriptedStreamTransport::typed(vec![
            Err(think_rejection()),
            Ok(responses_api_stream("unreachable")),
        ]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(p.stream_message("OPENAI", None, &params("gpt-4o")).await).await;
        assert_eq!(items.len(), 1);
        assert_eq!(p.transport.seen.lock().unwrap().len(), 1);
    }

    /// A pre-stream failure with NO chaining keeps the single-error behavior — no
    /// retry, exactly one transport call (the gate on `previous_response_id`).
    #[tokio::test]
    async fn pre_stream_failure_without_chaining_does_not_retry() {
        let prms = openai_params_with_prev(None);
        let t = ScriptedStreamTransport::new(vec![Err("HTTP 401: unauthorized".to_string())]);
        let p = WireStreamingProvider::new(
            t,
            keys(),
            TransportPolicy::default(),
            "Quilltap/test".to_string(),
        );
        let items = drain(p.stream_message("OPENAI", None, &prms).await).await;
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].as_ref().unwrap_err().message,
            "HTTP 401: unauthorized"
        );
        assert_eq!(
            p.transport.seen.lock().unwrap().len(),
            1,
            "no retry without a chaining id"
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
