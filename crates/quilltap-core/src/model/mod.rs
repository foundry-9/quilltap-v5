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
//! Today the boundary covers **embeddings** (the memory gate's only model call —
//! [`embedding`]). The **completion** half joins here as `model::completion` when
//! the first completion-consuming service (chat orchestration) lands; the same
//! canned-responder shape applies.
//!
//! Consumers take a generic `P: EmbeddingProvider` (not a trait object), so the
//! async boundary method needs no boxing — see [`embedding::EmbeddingProvider`].

pub mod embedding;

pub use embedding::{
    CannedEmbeddingProvider, EmbeddingError, EmbeddingPriority, EmbeddingProvider, EmbeddingResult,
};
