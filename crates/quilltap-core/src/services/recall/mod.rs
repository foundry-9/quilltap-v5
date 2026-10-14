//! Recall — a pipeline with a trace (v4 `lib/memory/recall/`, memory
//! refactor §3.8, Phase E `584d28164`):
//!
//! ```text
//! query    build_recall_query: the turn's text, signals and ONE embedding
//! gather   vector top-K (+ probes, + anchors) ∪ the text strategy → raw hits
//! hydrate  rows, tier / importance / source / subject filters
//! score    floor · salience · window · rank_for_recall
//! select   related expansion · R6 reservation · the cut
//! assemble the turn's pools: archive · recap · head · inter-character · conversations
//! render   the section text (no debug labels)
//! deliver  what reached the model: access stamps · the ring · the debug record
//! ```
//!
//! Search stamps nothing; `deliver` is the ONE writer of `lastAccessedAt`.

pub mod context;
pub mod rank;
pub mod types;

pub use context::RecallContextInput;
pub use types::{RecallCandidate, RecallPool, RecallTrace, RecallTraceEntry, RenderedLine};
