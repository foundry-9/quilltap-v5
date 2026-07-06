//! The five sans-IO wire-stream decoders (Phase-3 wave-4, W4.7b).
//!
//! Each decoder is a **push state machine**: [`StreamDecoder::push`] takes a
//! slice of already-received bytes (from wherever the host transport read them —
//! arbitrary boundaries, one byte at a time, mid-SSE-frame, mid-JSON,
//! mid-UTF-8) and emits zero or more normalised [`StreamChunk`]s;
//! [`StreamDecoder::finish`] flushes any terminal chunk at transport EOF and is
//! idempotent. No decoder does IO, spawns tasks, or touches `tokio` — the host
//! (Phase 4 / CLI, W4.7d) owns the socket and feeds bytes in. This is what makes
//! every decoder a leaf unit the differential harness can oracle field-by-field
//! against v4's real plugin `streamMessage` generator.
//!
//! The decoders reproduce **exactly** the normalisation each v4 plugin performs
//! on top of its SDK/transport — the `StreamChunk` vocabulary
//! ([`super::stream::StreamChunk`]) is the fixed target. Each also assembles the
//! terminal `raw_response` value verbatim (the `rawResponse` v4 hands back for
//! `detectToolCallsInResponse` / W4.7c's `parseToolCalls` — the tool-call
//! accumulator lands there, since the consuming streaming service reads tool
//! calls only off `raw_response`, never off a chunk field).
//!
//! ## The five (design-doc table, `provider-manifest.md`)
//!
//! | enum | v4 providers | wire |
//! |---|---|---|
//! | [`chat_completions_sse`] | openai-compatible, deepseek, z-ai, openrouter | OpenAI Chat Completions SSE |
//! | [`responses_api_sse`] | openai, grok | OpenAI Responses API SSE |
//! | [`anthropic_sse`] | anthropic | Anthropic Messages SSE |
//! | [`google_parts`] | google | genai `generateContentStream` (`data:` SSE) |
//! | [`ollama_ndjson`] | ollama | newline-delimited JSON |
//!
//! ## Divergences from the design-doc table (STOP-rule flags)
//!
//! Reading v4's `plugins/dist/*/provider.ts` at HEAD `8617ce7a` surfaced that
//! the four "chat-completions-sse" providers do NOT share one normalisation:
//!
//! - **deepseek / z-ai** go through the OpenAI SDK; reasoning is
//!   `delta.reasoning_content`; the terminal `raw_response` is
//!   `{ choices:[{ index, message:{ role, content:"", tool_calls?,
//!   reasoning_content? }, finish_reason }], usage }`.
//! - **openrouter** (its tool/vision path, `streamViaChatCompletions`) is a
//!   raw `fetch` decoder; reasoning is `delta.reasoning`; the terminal
//!   `raw_response` is `{ choices:[{ finishReason, delta:{ toolCalls? } }],
//!   usage }` (camelCase!). Its no-tools path uses the OpenRouter SDK's
//!   OpenResponses protocol, which is **out of scope** (a distinct, undocumented
//!   wire the design doc did not enumerate — a tracked deferral).
//! - **openai-compatible** base only ever yields `content` deltas + a terminal
//!   `done` carrying `usage` — no tool accumulation, no `raw_response`.
//!
//! So [`chat_completions_sse`] is parameterised by a [`chat_completions_sse::Flavor`]
//! selecting the emission + `raw_response` shape, over ONE shared SSE + wire
//! parser. The enum NAME is unchanged per the STOP rule; the flavor is an
//! internal selector the manifest picks alongside the decoder.

use super::stream::StreamChunk;

pub mod anthropic_sse;
pub mod chat_completions_sse;
pub mod google_parts;
pub mod ollama_ndjson;
pub mod responses_api_sse;
pub mod sse;

pub use anthropic_sse::AnthropicSseDecoder;
pub use chat_completions_sse::{ChatCompletionsSseDecoder, Flavor as ChatCompletionsFlavor};
pub use google_parts::GooglePartsDecoder;
pub use ollama_ndjson::OllamaNdjsonDecoder;
pub use responses_api_sse::ResponsesApiSseDecoder;

/// A fatal decode error — the wire could not be interpreted at all (e.g. a
/// provider `error` event mid-stream, an Anthropic `error` frame). A decoder
/// surfaces this as `Err`; the host maps it to a [`super::stream::StreamError`]
/// item on the channel (v4's generator would `throw` at the same point). Content
/// already emitted before the error stays emitted — v4 streams can fail after
/// yielding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub message: String,
}

impl DecodeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DecodeError {}

/// A sans-IO push decoder: bytes in, normalised [`StreamChunk`]s out.
pub trait StreamDecoder {
    /// Feed received bytes; emit zero or more chunks produced by this push.
    /// A decode failure (mid-stream provider error) returns `Err` — any chunks
    /// produced *before* the failure in the same push are lost from the return,
    /// so decoders that can both emit and fail in one push should not; in
    /// practice an error event is its own frame and produces only `Err`.
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamChunk>, DecodeError>;

    /// Transport EOF: emit any terminal chunk not yet produced. **Idempotent** —
    /// calling it twice yields the terminal chunk at most once.
    fn finish(&mut self) -> Result<Vec<StreamChunk>, DecodeError>;
}
