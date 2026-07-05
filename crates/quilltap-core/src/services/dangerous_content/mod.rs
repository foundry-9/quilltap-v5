//! The dangerous-content ("Concierge") subsystem (v4
//! `lib/services/dangerous-content/` + the `CHAT_DANGER_CLASSIFICATION` job
//! runner). Ported leaf-to-root:
//!
//!   - [`chat_override`] — the two-field danger status derivation.
//!   - [`resolver`] — the effective [`crate::db::chat_settings::DangerousContentSettings`]
//!     (global + per-chat off-duty / exempt short-circuits).
//!   - [`gatekeeper`] — content classification (moderation-provider seam →
//!     cheap-LLM), the pure parse/map leaves, and the classification cache.
//!   - [`provider_routing`] — the uncensored-reroute resolution (the REAL
//!     implementor of the [`crate::services::provider_failover::DangerousContentRouter`]
//!     seam) + the image-provider variants.
//!   - [`manual_flip`] — the operator's manual tri-state Concierge flip.
//!   - [`gatekeeper_job`] — the `CHAT_DANGER_CLASSIFICATION` job runner
//!     (classify → persist the chat-level danger fields + system event).
//!
//! ## Tracked deferrals (handed to the unifier / later waves)
//!
//!   - **Spine integration** — constructing the real router + gatekeeper at the
//!     orchestrator composition point, and the orchestrator-corpus cases that
//!     need them (the danger-resolver OFF short-circuit + a live
//!     uncensored-reroute), edit files W4.4a owns; the unifier lands them.
//!   - **Concierge announcements** ([`manual_flip::ConciergeAnnouncer`] /
//!     [`gatekeeper_job::ConciergeAnnouncer`]) — the personified-system writer
//!     (`concierge-notifications/writer.ts`) posts synthetic messages; seamed
//!     (default no-op), a W4.6 personified-writer deferral.
//!   - **Moderation plugin / cheap-LLM API key / `logLLMCall` / job runner
//!     infra** — see the per-module docs.

pub mod chat_override;
pub mod gatekeeper;
pub mod gatekeeper_job;
pub mod manual_flip;
mod prompt_text;
pub mod provider_routing;
pub mod resolver;
