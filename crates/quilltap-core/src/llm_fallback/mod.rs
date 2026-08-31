//! Provider/model fallback chains (v4 `lib/llm/fallback/`, commit `65f5021c8`).
//!
//! A connection profile can name an understudy. When a call through a profile
//! fails outright — rejected key, rate limit, network error, missing model,
//! 5xx, empty response, moderation refusal — the next candidate takes the turn
//! instead of the call failing.
//!
//! The machinery lives at the provider layer rather than inside the
//! chat-message services because both callers' currencies reduce to the same
//! two questions: *is this failure worth trying someone else for?* and *who is
//! next?* Four call sites use it — the Salon's hard-error path, the Salon's
//! empty-response path, the cheap-LLM task runner, and image description.
//!
//! Public surface (v4's `index.ts`): [`classify_fallback_trigger`],
//! [`build_fallback_chain`], [`record_attempt`], [`summarize_fallback_attempts`],
//! [`FallbackRepos`]; [`pick_tier_candidate`] + [`tier_matches`]; and the types.

mod engine;
mod tier_picker;
mod types;

pub use engine::{
    build_fallback_chain, classify_fallback_trigger, record_attempt, summarize_fallback_attempts,
    FallbackRepos,
};
pub use tier_picker::{pick_tier_candidate, tier_matches};
pub use types::{
    FallbackAttempt, FallbackCandidate, FallbackCandidateKind, FallbackContext, FallbackError,
    FallbackProfile, FallbackPurpose, FallbackTrigger,
};
