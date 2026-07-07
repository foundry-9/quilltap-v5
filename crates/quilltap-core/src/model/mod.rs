//! The model boundary (Phase-3 Unit 0.5) — the single seam every model call in
//! the engine goes through. It is the tier-3 differential's injection point: in
//! production this is wired to the real provider (the JSON manifest + fixed
//! stream decoders of `docs/developer/porting/provider-manifest.md`, landing later
//! in Phase 3 / Phase 4); in the differential harness it is wired to a
//! **canned responder** that returns fixed outputs keyed by call input. Because
//! the same canned response is injected on both the Rust and v4 sides, everything
//! downstream of the model call is deterministic, so the existing tier-2
//! canonical-dump machinery diffs the resulting writes — any divergence is in
//! *our* orchestration, not the model.
//!
//! The boundary has three halves: **embeddings** ([`embedding`] — the memory
//! gate's model call), **completions** ([`completion`] — the cheap-LLM task
//! pipeline's model call, first consumed by the memory-processor extraction), and
//! **streaming completions** ([`stream`] — the primary chat stream, consumed by
//! the wave-3 primary-stream service). All three follow the same canned-responder
//! shape.
//!
//! Consumers take generic `P: EmbeddingProvider` / `C: CompletionProvider` /
//! `S: StreamingCompletionProvider` parameters (not trait objects), so the async
//! boundary methods need no boxing — see [`embedding::EmbeddingProvider`] /
//! [`completion::CompletionProvider`] / [`stream::StreamingCompletionProvider`].

pub mod completion;
pub mod decoders;
pub mod embedding;
pub mod image;
pub mod stream;

pub use completion::{
    CannedCompletionProvider, CompletionError, CompletionMessage, CompletionParams,
    CompletionProvider, CompletionResponse, CompletionRole, CompletionUsage,
};
pub use decoders::{
    AnthropicSseDecoder, ChatCompletionsFlavor, ChatCompletionsSseDecoder, DecodeError,
    GooglePartsDecoder, OllamaNdjsonDecoder, ResponsesApiSseDecoder, StreamDecoder,
};
pub use embedding::{
    CannedEmbeddingProvider, EmbeddingError, EmbeddingPriority, EmbeddingProvider, EmbeddingResult,
};
pub use stream::{
    CannedStreamingProvider, StreamAttachmentFailure, StreamAttachmentResults, StreamCacheUsage,
    StreamChunk, StreamChunkResult, StreamError, StreamParams, StreamUsage,
    StreamingCompletionProvider,
};
