//! The streaming half of the model boundary — the tier-3 seam for the primary
//! chat stream (`processMessage` → `runPrimaryStream`). Mirrors v4's
//! `provider.streamMessage(params, apiKey)` from
//! `packages/plugin-types/src/providers/text.ts` (`LLMParams` / `StreamChunk`),
//! as driven by `lib/services/chat-message/streaming.service.ts` (`streamMessage`
//! generator, ~lines 312–443).
//!
//! The boundary sits at the provider call itself: the streaming service builds
//! `LLMParams`, iterates `provider.streamMessage(...)`, and folds the chunks
//! (accumulating `content`, remembering the last `usage` / `cacheUsage` /
//! `rawProviderUsage`, acting on `done`). Everything above the provider call —
//! content-block normalisation, the LLM-call log, finish-reason extraction — is
//! ported orchestration that must live *inside* the differential, so the seam is
//! the same one the future primary-stream oracle will mock (`createLLMProvider`'s
//! returned provider). API-key acquisition stays host-side (resolved before the
//! provider call; the canned responder needs no key, the real wire decoders of
//! `docs/developer/porting/provider-manifest.md` arrive later).
//!
//! **Oracle injection is deferred.** Like [`super::completion`] (which waited for
//! the memory-processor differential), the v4-oracle-side canned injection —
//! recording each `provider|model|temperature|messages` key and replaying the
//! same chunk sequence — lands with the wave-3 **primary-stream differential**.
//! This module ships the trait, the canned provider, and self-tests only.
//!
//! ## The streaming idiom
//!
//! A real provider produces chunks over time from a network read, so the natural
//! shape is an async sequence. We deliver it as a [`tokio::sync::mpsc::Receiver`]
//! of [`StreamChunkResult`] (a `Result` so a mid-stream provider error is a first-
//! class stream item — v4 streams can throw *after* emitting content). This uses
//! only the `sync` tokio feature the core already links (no new dep, no
//! `tokio_stream`/`futures-util`/`async-channel`), keeps the future `Send`, and
//! lets a real provider spawn the network read on the host's runtime and forward
//! decoded chunks into the sender. Consumers stay generic over
//! `P: StreamingCompletionProvider` (no trait object, no boxing), matching the
//! embedding/completion halves.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

pub use super::completion::{CompletionMessage, CompletionRole};

// ============================================================================
// The carrying message type (P4.13 unit 1)
// ============================================================================

/// One entry in an assistant turn's `toolCalls` array (v4
/// `LLMMessage.toolCalls[]`). Serializes byte-identical to v4's
/// `{ id, type: 'function', function: { name, arguments } }`. Canonical home of
/// the type both the threading helpers and the stream boundary carry (moved here
/// from `services::tool_call_threading`, which re-exports it).
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct ToolCallPayload {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct ToolCallFunction {
    pub name: String,
    /// The `JSON.stringify(arguments)` string.
    pub arguments: String,
}

/// A message of a streaming call — v4's `LLMMessage` as the chat path sends it
/// to `streamMessage`. The carrying type of the provider-I/O seam (P4.13
/// unit 1): unlike the old `[{role, content}]` view (which silently discarded
/// the tool-call linkage before any request builder ran — dogfood finding #25),
/// this type can represent everything the wire needs, and its illegal states
/// are unrepresentable:
///
/// - a native tool result **cannot exist without its provider call ID**
///   ([`StreamMessage::Tool`] requires one; v4's id-less arm is the explicit
///   `[Tool Result: …]` user-text fallback, [`StreamMessage::tool_result_fallback`]);
/// - an assistant turn that invoked tools **carries its calls in the same
///   variant** — the calls cannot be separated from the turn.
///
/// The canned tier-3 key ([`canned_stream_key`]) projects role+content only, so
/// oracle-recorded keys are unchanged by the metadata riding here.
#[derive(Clone, Debug, PartialEq)]
pub enum StreamMessage {
    System {
        content: String,
    },
    User {
        content: String,
        /// v4 `msg.cacheControl` (`{ type: 'ephemeral' }`) — the summary-head
        /// prompt-cache breakpoint the context builder stamps; consumed by the
        /// anthropic builder's per-message cache arm.
        cache_control: Option<serde_json::Value>,
        /// v4 `msg.attachments` (`FileAttachment[]`) — carried as the VERBATIM
        /// JSON bags the file subsystem built (`{id, filepath, filename,
        /// mimeType, size, data, url?, …}`), so no conversion can drop a field
        /// (P4.21; dogfood #37's structural cause was a conversion that read
        /// `content` alone). The request builders read `id`/`mimeType`/`data`/
        /// `url`/`filename` with JS truthiness and emit each provider's wire
        /// image/document parts; the canned tier-3 key ignores them (role +
        /// content only), so oracle-recorded keys are unchanged. Empty for every
        /// message the attachment stamper (`message_context`) didn't touch —
        /// v4 stamps the merged slate onto the LAST user message only.
        attachments: Vec<serde_json::Value>,
    },
    Assistant {
        content: String,
        /// The native tool calls this turn made (empty for a plain turn). When
        /// non-empty the request builders emit the provider's paired
        /// assistant-with-tool-calls shape.
        tool_calls: Vec<ToolCallPayload>,
        /// The assistant's own reasoning text (v4 `msg.reasoningContent`) —
        /// echoed by the DeepSeek / Z.AI transforms on a tool-call turn.
        reasoning_content: Option<String>,
        /// The Gemini thought signature (v4 `msg.thoughtSignature`) — the
        /// google round-trip.
        thought_signature: Option<String>,
        /// v4 `msg.cacheControl` — see [`StreamMessage::User`].
        cache_control: Option<serde_json::Value>,
    },
    /// A native tool result, paired to its call by `call_id` (required by
    /// construction — v4's providers either drop or mis-send an id-less tool
    /// message, so v5 makes one unrepresentable; the id-less case is the
    /// explicit user-text fallback below).
    Tool {
        call_id: String,
        /// The tool's function name (v4 `msg.name` — google's `functionResponse`
        /// correlation; `None` falls back to the call id there).
        name: Option<String>,
        content: String,
    },
}

impl StreamMessage {
    pub fn system(content: impl Into<String>) -> Self {
        StreamMessage::System {
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        StreamMessage::User {
            content: content.into(),
            cache_control: None,
            attachments: Vec::new(),
        }
    }

    /// The user message's attachments (empty for every other role — only v4's
    /// user messages ever carry them onto the wire).
    pub fn attachments(&self) -> &[serde_json::Value] {
        match self {
            StreamMessage::User { attachments, .. } => attachments,
            _ => &[],
        }
    }

    /// A plain assistant turn (no tool calls, no reasoning metadata).
    pub fn assistant(content: impl Into<String>) -> Self {
        StreamMessage::Assistant {
            content: content.into(),
            tool_calls: Vec::new(),
            reasoning_content: None,
            thought_signature: None,
            cache_control: None,
        }
    }

    /// The per-message cache-control hint (v4 `msg.cacheControl`; `None` on
    /// roles that never carry one).
    pub fn cache_control(&self) -> Option<&serde_json::Value> {
        match self {
            StreamMessage::User { cache_control, .. }
            | StreamMessage::Assistant { cache_control, .. } => cache_control.as_ref(),
            _ => None,
        }
    }

    /// v4's id-less tool-result framing (the `tool_call_threading` fallback
    /// rule): a plain **user** message `[Tool Result: <name>]\n<content>`. This
    /// is the explicit variant of "the provider gave no call ID" — v4 behavior,
    /// not a v5 invention; it survives every provider formatter.
    pub fn tool_result_fallback(name: &str, content: &str) -> Self {
        StreamMessage::User {
            content: format!("[Tool Result: {name}]\n{content}"),
            cache_control: None,
            attachments: Vec::new(),
        }
    }

    /// The v4 wire role string.
    pub fn role_str(&self) -> &'static str {
        match self {
            StreamMessage::System { .. } => "system",
            StreamMessage::User { .. } => "user",
            StreamMessage::Assistant { .. } => "assistant",
            StreamMessage::Tool { .. } => "tool",
        }
    }

    /// The message's text content (the canned-key / log projection).
    pub fn content(&self) -> &str {
        match self {
            StreamMessage::System { content }
            | StreamMessage::User { content, .. }
            | StreamMessage::Assistant { content, .. }
            | StreamMessage::Tool { content, .. } => content,
        }
    }
}

/// Token usage of a stream (v4 `TokenUsage`, typically on the final chunk).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct StreamUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

/// Cache usage of a stream (v4 `CacheUsage`, normalised). Every field is
/// optional because providers report different subsets (OpenRouter cached tokens
/// + discount; Anthropic cache-creation / cache-read).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct StreamCacheUsage {
    pub cached_tokens: Option<i64>,
    pub cache_discount: Option<f64>,
    pub cache_creation_input_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
}

/// Attachment-processing results carried on a chunk (v4 `AttachmentResults`).
/// Serializes to v4's `{ sent, failed }` shape (the differential diffs it
/// against the oracle-recorded results; the finalizer emits it on the done
/// event).
#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize)]
pub struct StreamAttachmentResults {
    /// IDs of attachments sent successfully.
    pub sent: Vec<String>,
    /// Attachments that failed, with the error text.
    pub failed: Vec<StreamAttachmentFailure>,
}

/// One failed attachment (v4 `AttachmentResults.failed[]`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StreamAttachmentFailure {
    pub id: String,
    pub error: String,
}

/// A single normalised chunk of a streamed completion — the v4 `StreamChunk`
/// vocabulary. This is the type the five future wire decoders of
/// `docs/developer/porting/provider-manifest.md` will emit; it stays faithful to
/// v4's shape so the decoders have a fixed target.
///
/// Every field except `content` / `done` is optional, arriving typically on the
/// terminal chunk. `raw_response` is v4's `rawResponse: unknown` — kept as an
/// opaque JSON value (finish-reason extraction / tool-call detection consume it
/// above the seam), so this port carries it verbatim rather than pre-parsing.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StreamChunk {
    /// Content delta in this chunk (v4's `content: string`; often empty on the
    /// terminal usage-only chunk).
    pub content: String,
    /// Whether this is the final chunk.
    pub done: bool,
    /// Token usage (typically on the final chunk).
    pub usage: Option<StreamUsage>,
    /// Normalised cache usage (typically on the final chunk).
    pub cache_usage: Option<StreamCacheUsage>,
    /// Provider-shape `usage` sub-object, captured pre-normalisation for
    /// cache-instrumentation diagnostics (v4 `rawProviderUsage`). An explicit
    /// `Some(Null)` vs `None` is preserved because v4 distinguishes a present-but-
    /// null field from an absent one.
    pub raw_provider_usage: Option<serde_json::Value>,
    /// Attachment processing results (typically on the final chunk).
    pub attachment_results: Option<StreamAttachmentResults>,
    /// Raw provider response, opaque here (v4 `rawResponse: unknown`) — consumed
    /// above the seam for finish-reason / tool-call detection.
    pub raw_response: Option<serde_json::Value>,
    /// Google Gemini thought signature.
    pub thought_signature: Option<String>,
    /// Reasoning / chain-of-thought content (typically on the final chunk).
    pub reasoning_content: Option<String>,
}

impl StreamChunk {
    /// A plain content delta (the common non-terminal chunk).
    pub fn content(text: impl Into<String>) -> Self {
        Self {
            content: text.into(),
            ..Default::default()
        }
    }

    /// A terminal chunk carrying only `done` + optional `usage`.
    pub fn done(usage: Option<StreamUsage>) -> Self {
        Self {
            done: true,
            usage,
            ..Default::default()
        }
    }
}

/// One item of a stream: a [`StreamChunk`] or a mid-stream provider error. A v4
/// generator can `throw` after yielding chunks, so an error is a first-class
/// stream item — a consumer sees every chunk before the failure, then the error.
pub type StreamChunkResult = Result<StreamChunk, StreamError>;

/// Error from a streaming call — either raised before the first chunk (the
/// stream is a single `Err`) or mid-stream (the `Err` follows the chunks emitted
/// so far). The message text is carried verbatim: v4's failover path inspects it.
#[derive(Clone, Debug)]
pub struct StreamError {
    pub message: String,
}

impl StreamError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for StreamError {}

/// Parameters of a streaming call — the fields the chat streaming path actually
/// sets on v4's `LLMParams` (see `streaming.service.ts:356–370`).
///
/// **Deferred** from v4's `LLMParams` (not set on the primary-stream path, or
/// carried above the seam, so omitted to keep the boundary minimal):
/// `strictMaxTokens` (a cheap-LLM-only flag — the streaming path never sets it)
/// and `toolChoice` / `responseFormat` / `seed` / `user` (the chat path doesn't
/// set them here). `attachments` ride the MESSAGES, not the params (P4.21,
/// matching v4's `messages[].attachments`): the file subsystem stamps the
/// merged slate onto the last user [`StreamMessage::User`], the request
/// builders emit each provider's wire parts from it, and the sent/failed
/// outcome is reported back via `StreamChunk::attachment_results`. `tools` is
/// carried as opaque JSON — the provider-specific tool serialisation is
/// settled before the call, and the canned key hashes it verbatim.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamParams {
    pub messages: Vec<StreamMessage>,
    pub model: String,
    /// `None` when the profile omits a custom temperature.
    pub temperature: Option<f64>,
    /// `None` when unset (provider default).
    pub max_tokens: Option<i64>,
    /// Nucleus sampling (v4 `topP`), `None` when unset.
    pub top_p: Option<f64>,
    /// Tool definitions, provider-serialised (v4 `tools?: unknown[]`); `None`
    /// when there are no tools (v4 sends `undefined`, not `[]`).
    pub tools: Option<serde_json::Value>,
    /// Native web-search toggle (v4 `webSearchEnabled`).
    pub web_search_enabled: bool,
    /// The connection profile's provider extras, forwarded verbatim (v4
    /// `profileParameters`).
    pub profile_parameters: Option<serde_json::Value>,
    /// Per-character prompt-cache identifier (v4 `cacheKey`); `None` for
    /// user-only chats / titling.
    pub cache_key: Option<String>,
    /// OpenAI Responses-API conversation chaining id (v4 `previousResponseId`).
    pub previous_response_id: Option<String>,
    /// Stop sequences (v4 `stop?: string | string[]`), normalised to a list;
    /// empty when unset.
    pub stop: Vec<String>,
    /// v4 `LLMParams.requestTimeoutMs` on the STREAMING half (P4.D83): a hard
    /// wall-clock ceiling for this one call, in milliseconds, bounding
    /// time-to-first-byte only (the transport module header explains why a
    /// whole-exchange ceiling would truncate a long generation).
    ///
    /// The field exists because v4's `LLMParams` carries it on both halves and
    /// because it must WIN over a provider-side profile default — that
    /// precedence is the contract, and a resolver with no way to express the
    /// caller's ceiling could not be proven to honour it. No v5 streaming caller
    /// sets one today (the cheap-LLM deadlines all go through the completion
    /// half), and no v4 streaming caller does either.
    pub request_timeout_ms: Option<i64>,
}

/// The streaming completion boundary — every streamed model call goes through
/// this trait.
///
/// `stream_message` returns (once, `async`) a [`tokio::sync::mpsc::Receiver`] of
/// [`StreamChunkResult`]s: the provider drives its network read and forwards
/// decoded chunks (or a terminal error) into the channel. `provider` / `base_url`
/// mirror v4's `createLLMProvider(profile.provider, profile.baseUrl)` factory
/// arguments — folded into the call so the trait stays a single seam. Generic-
/// consumed (no trait object) and `Send`-returning, matching the embedding /
/// completion halves.
pub trait StreamingCompletionProvider {
    fn stream_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send;
}

/// `Arc<T>` is a [`StreamingCompletionProvider`] whenever `T` is (delegating to
/// the inner value) — the production-shaped ownership answer (W4.11a). This is
/// what lets the differential (and, later, the Phase-4 host) share ONE stateful
/// streaming provider between the borrowed spine dep and the owned, effectively-
/// `'static` `TypedAskCarina` seam, so both draw from the SAME per-key queues the
/// single v4 mock served (a bare clone would duplicate the queues).
impl<T: StreamingCompletionProvider> StreamingCompletionProvider for Arc<T> {
    fn stream_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        (**self).stream_message(provider, base_url, params)
    }
}

/// The canonical lookup key for a canned stream: provider, model, temperature
/// (v4 omits vs sends — `None` keys as `-`), and the messages as the same
/// `[{role, content}]` JSON the completion half uses. The future primary-stream
/// oracle's mock computes the identical string, so a call either matches an entry
/// registered on both sides or surfaces as a corpus omission on both sides.
///
/// This reuses [`super::completion::canned_completion_key`]'s exact byte form
/// (via the shared parts renderer) so the two seams share one key discipline — a
/// streamed call and its non-streamed twin over the same messages hash
/// identically, and the tool-call metadata a [`StreamMessage`] carries never
/// shifts an oracle-recorded key.
pub fn canned_stream_key<M: super::completion::CannedKeyMessage>(
    provider: &str,
    model: &str,
    temperature: Option<f64>,
    messages: &[M],
) -> String {
    super::completion::canned_key_from_parts(
        provider,
        model,
        temperature,
        messages.iter().map(|m| (m.key_role(), m.key_content())),
    )
}

impl super::completion::CannedKeyMessage for StreamMessage {
    fn key_role(&self) -> &str {
        self.role_str()
    }
    fn key_content(&self) -> &str {
        self.content()
    }
}

/// A deterministic [`StreamingCompletionProvider`] for the tier-3 differential.
/// Replays a **registered `Vec<StreamChunkResult>`** keyed by the exact call
/// input ([`canned_stream_key`]): the chunks (and any mid-stream failure entry)
/// are pushed into the channel in registration order, so the consumer observes
/// the same sequence the real provider would.
///
/// - **Unregistered input** → a single `Err` (a corpus omission — surfaced, never
///   a silent empty stream).
/// - **Fail before the first chunk** → register a sequence that is just an `Err`.
/// - **Fail mid-stream** → register chunks followed by an `Err` (v4 streams can
///   throw after emitting content).
#[derive(Clone, Default)]
pub struct CannedStreamingProvider {
    streams: HashMap<String, Vec<StreamChunkResult>>,
}

impl CannedStreamingProvider {
    /// A fresh provider with no canned streams.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the exact chunk sequence for the call `(provider, model,
    /// temperature, messages)`. The sequence may end in an `Err` to model a
    /// mid-stream (or, if it is the only item, a pre-first-chunk) failure.
    /// Registration stays keyed by `[{role, content}]` (the oracle-recorded
    /// shape), so corpora register with plain [`CompletionMessage`]s while
    /// lookups project the richer [`StreamMessage`] to the identical key.
    pub fn with_stream<M: super::completion::CannedKeyMessage>(
        mut self,
        provider: &str,
        model: &str,
        temperature: Option<f64>,
        messages: &[M],
        chunks: Vec<StreamChunkResult>,
    ) -> Self {
        let key = canned_stream_key(provider, model, temperature, messages);
        self.streams.insert(key, chunks);
        self
    }

    /// Convenience: register a successful stream from content-only chunks plus a
    /// terminal `done` chunk carrying `usage`. The strings become successive
    /// content deltas; the final item is `StreamChunk::done(usage)`.
    pub fn with_content_stream<M: super::completion::CannedKeyMessage>(
        self,
        provider: &str,
        model: &str,
        temperature: Option<f64>,
        messages: &[M],
        deltas: &[&str],
        usage: Option<StreamUsage>,
    ) -> Self {
        let mut chunks: Vec<StreamChunkResult> = deltas
            .iter()
            .map(|d| Ok(StreamChunk::content(*d)))
            .collect();
        chunks.push(Ok(StreamChunk::done(usage)));
        self.with_stream(provider, model, temperature, messages, chunks)
    }
}

impl StreamingCompletionProvider for CannedStreamingProvider {
    fn stream_message(
        &self,
        provider: &str,
        _base_url: Option<&str>,
        params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        // Resolve the sequence synchronously so the returned future owns it and
        // is `'static` + `Send` (no borrow of `&self` escapes into the future).
        let key = canned_stream_key(
            provider,
            &params.model,
            params.temperature,
            &params.messages,
        );
        let sequence: Vec<StreamChunkResult> = match self.streams.get(&key) {
            Some(chunks) => chunks.clone(),
            None => vec![Err(StreamError::new(format!(
                "no canned stream registered for call (provider {provider}, model {}, \
                 temperature {:?}, {} message(s))",
                params.model,
                params.temperature,
                params.messages.len(),
            )))],
        };
        async move {
            // Bounded, but large enough that pushing the whole canned sequence
            // never blocks (the receiver is returned to the caller to drain).
            let (tx, rx) = tokio::sync::mpsc::channel(sequence.len().max(1));
            for item in sequence {
                // The receiver is live (just created); send cannot fail here.
                let _ = tx.send(item).await;
            }
            rx
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(model: &str, temperature: Option<f64>, messages: Vec<StreamMessage>) -> StreamParams {
        StreamParams {
            messages,
            model: model.to_string(),
            temperature,
            max_tokens: Some(4096),
            top_p: None,
            tools: None,
            web_search_enabled: false,
            profile_parameters: None,
            cache_key: Some("char:abc".to_string()),
            previous_response_id: None,
            stop: Vec::new(),
            request_timeout_ms: None,
        }
    }

    /// Drain a receiver into a Vec, stopping at (and including) the first `Err`.
    async fn drain(
        mut rx: tokio::sync::mpsc::Receiver<StreamChunkResult>,
    ) -> Vec<StreamChunkResult> {
        let mut out = Vec::new();
        while let Some(item) = rx.recv().await {
            let is_err = item.is_err();
            out.push(item);
            if is_err {
                break;
            }
        }
        out
    }

    #[tokio::test]
    async fn replays_chunks_in_registration_order() {
        let messages = vec![
            StreamMessage::system("you are a character"),
            StreamMessage::user("hello"),
        ];
        let provider = CannedStreamingProvider::new().with_content_stream(
            "ANTHROPIC",
            "claude-sonnet-4-5",
            Some(0.8),
            &messages,
            &["Hel", "lo ", "there"],
            Some(StreamUsage {
                prompt_tokens: 50,
                completion_tokens: 3,
                total_tokens: 53,
            }),
        );
        let rx = provider
            .stream_message(
                "ANTHROPIC",
                None,
                &params("claude-sonnet-4-5", Some(0.8), messages),
            )
            .await;
        let items = drain(rx).await;
        // Three content chunks + one terminal done chunk.
        assert_eq!(items.len(), 4);
        let contents: Vec<&str> = items
            .iter()
            .map(|r| r.as_ref().unwrap().content.as_str())
            .collect();
        assert_eq!(contents, vec!["Hel", "lo ", "there", ""]);
        let last = items.last().unwrap().as_ref().unwrap();
        assert!(last.done);
        assert_eq!(last.usage.unwrap().total_tokens, 53);
    }

    /// The `Arc<T>` blanket impl (W4.11a) delegates to the SAME inner provider, so
    /// two `Arc` clones share consumption (the sharing the ask_carina seam relies
    /// on). Proven here with a stateful pop-once queue.
    #[tokio::test]
    async fn arc_clones_share_consumption() {
        use std::sync::Mutex;
        // A minimal stateful provider: each key's sequence is served exactly once.
        struct PopOnce {
            served: Mutex<Vec<String>>,
        }
        impl StreamingCompletionProvider for PopOnce {
            fn stream_message(
                &self,
                _provider: &str,
                _base_url: Option<&str>,
                params: &StreamParams,
            ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send
            {
                let already = {
                    let mut served = self.served.lock().unwrap();
                    let seen = served.contains(&params.model);
                    served.push(params.model.clone());
                    seen
                };
                async move {
                    let (tx, rx) = tokio::sync::mpsc::channel(1);
                    let chunk = if already {
                        Err(StreamError::new("exhausted"))
                    } else {
                        Ok(StreamChunk::content("first"))
                    };
                    let _ = tx.send(chunk).await;
                    rx
                }
            }
        }
        let arc = Arc::new(PopOnce {
            served: Mutex::new(Vec::new()),
        });
        let clone = Arc::clone(&arc);
        let p = params("m", None, vec![StreamMessage::user("x")]);
        // First call via one clone: served.
        let first = drain(arc.stream_message("P", None, &p).await).await;
        assert_eq!(first[0].as_ref().unwrap().content, "first");
        // Second call via the OTHER clone: the shared state remembers → exhausted.
        let second = drain(clone.stream_message("P", None, &p).await).await;
        assert!(second[0].is_err());
    }

    #[tokio::test]
    async fn mid_stream_failure_surfaces_after_the_chunks_before_it() {
        let messages = vec![StreamMessage::user("stream then die")];
        let provider = CannedStreamingProvider::new().with_stream(
            "OPENAI",
            "gpt-x",
            Some(0.7),
            &messages,
            vec![
                Ok(StreamChunk::content("partial ")),
                Ok(StreamChunk::content("answer")),
                Err(StreamError::new("upstream connection reset")),
            ],
        );
        let rx = provider
            .stream_message("OPENAI", None, &params("gpt-x", Some(0.7), messages))
            .await;
        let items = drain(rx).await;
        assert_eq!(items.len(), 3);
        assert!(items[0].is_ok());
        assert!(items[1].is_ok());
        let err = items[2].as_ref().unwrap_err();
        assert_eq!(err.message, "upstream connection reset");
    }

    #[tokio::test]
    async fn failure_before_first_chunk_is_a_single_error() {
        let messages = vec![StreamMessage::user("fail immediately")];
        let provider = CannedStreamingProvider::new().with_stream(
            "OPENAI",
            "gpt-x",
            Some(0.7),
            &messages,
            vec![Err(StreamError::new("401 unauthorized"))],
        );
        let rx = provider
            .stream_message("OPENAI", None, &params("gpt-x", Some(0.7), messages))
            .await;
        let items = drain(rx).await;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_ref().unwrap_err().message, "401 unauthorized");
    }

    #[tokio::test]
    async fn unregistered_input_is_an_error_not_a_silent_empty_stream() {
        let provider = CannedStreamingProvider::new();
        let rx = provider
            .stream_message(
                "ANTHROPIC",
                None,
                &params("m", Some(0.3), vec![StreamMessage::user("unknown")]),
            )
            .await;
        let items = drain(rx).await;
        assert_eq!(items.len(), 1);
        assert!(items[0]
            .as_ref()
            .unwrap_err()
            .message
            .contains("no canned stream registered"));
    }

    #[tokio::test]
    async fn key_is_sensitive_to_temperature_and_messages() {
        // The failover path re-issues the same messages on a different provider
        // or with a changed temperature — each variant is its own entry.
        let base = vec![StreamMessage::user("hi")];
        let changed = vec![StreamMessage::user("hi!")];
        let provider = CannedStreamingProvider::new()
            .with_content_stream("P", "m", Some(0.3), &base, &["a"], None)
            .with_content_stream("P", "m", Some(0.9), &base, &["b"], None)
            .with_content_stream("P", "m", Some(0.3), &changed, &["c"], None);

        let first = |t: Option<f64>, msgs: Vec<StreamMessage>| {
            let provider = &provider;
            async move {
                let rx = provider
                    .stream_message("P", None, &params("m", t, msgs))
                    .await;
                drain(rx).await.into_iter().next().unwrap().unwrap().content
            }
        };
        assert_eq!(first(Some(0.3), base.clone()).await, "a");
        assert_eq!(first(Some(0.9), base.clone()).await, "b");
        assert_eq!(first(Some(0.3), changed).await, "c");
    }

    #[tokio::test]
    async fn consumer_accumulates_content_and_captures_terminal_usage() {
        // Models the streaming service's fold: accumulate `content`, remember the
        // last `usage` / `cache_usage`, act on `done` — the shape primary-stream
        // will reuse.
        let messages = vec![StreamMessage::user("tell me a story")];
        let provider = CannedStreamingProvider::new().with_stream(
            "ANTHROPIC",
            "claude",
            Some(1.0),
            &messages,
            vec![
                Ok(StreamChunk::content("Once ")),
                Ok(StreamChunk::content("upon ")),
                Ok(StreamChunk::content("a time.")),
                Ok(StreamChunk {
                    content: String::new(),
                    done: true,
                    usage: Some(StreamUsage {
                        prompt_tokens: 12,
                        completion_tokens: 4,
                        total_tokens: 16,
                    }),
                    cache_usage: Some(StreamCacheUsage {
                        cache_read_input_tokens: Some(10),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            ],
        );
        let mut rx = provider
            .stream_message("ANTHROPIC", None, &params("claude", Some(1.0), messages))
            .await;

        let mut accumulated = String::new();
        let mut last_usage: Option<StreamUsage> = None;
        let mut last_cache: Option<StreamCacheUsage> = None;
        let mut saw_done = false;
        while let Some(item) = rx.recv().await {
            let chunk = item.expect("no error in this stream");
            if !chunk.content.is_empty() {
                accumulated.push_str(&chunk.content);
            }
            if let Some(u) = chunk.usage {
                last_usage = Some(u);
            }
            if let Some(c) = chunk.cache_usage {
                last_cache = Some(c);
            }
            if chunk.done {
                saw_done = true;
            }
        }
        assert_eq!(accumulated, "Once upon a time.");
        assert_eq!(last_usage.unwrap().total_tokens, 16);
        assert_eq!(last_cache.unwrap().cache_read_input_tokens, Some(10));
        assert!(saw_done);
    }
}
