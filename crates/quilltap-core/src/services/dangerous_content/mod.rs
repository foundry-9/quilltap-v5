//! The dangerous-content ("Concierge") subsystem (v4
//! `lib/services/dangerous-content/` + the `CHAT_DANGER_CLASSIFICATION` job
//! runner). Ported leaf-to-root:
//!
//!   - [`chat_override`] — the three-state Concierge posture (v4 `4d370a90f`):
//!     `conciergeMode` + provenance, the legacy pair ignored.
//!   - [`current_state`] — the state re-read at refusal time (#75).
//!   - [`classifier_switch`] — the classifier verdict's move to Unmoderated (#75).
//!   - [`resolver`] — the Concierge policy (`resolve_concierge_settings`,
//!     v4 `3b463d6b1`, #76): global `conciergeSettings` + the chat's state →
//!     the named questions (`failover_allowed`, `route_direct`, `pre_screen`,
//!     …), exempt → off duty → Locked → Unmoderated → Moderated.
//!   - [`legacy_concierge_settings`] — the retired settings translated into
//!     `conciergeSettings` (restore + the backup remap; #76).
//!   - [`gatekeeper`] — content classification (moderation-provider seam →
//!     cheap-LLM), the pure parse/map leaves, and the classification cache.
//!   - [`provider_routing`] — the uncensored-reroute resolution (the REAL
//!     implementor of the [`crate::services::provider_failover::DangerousContentRouter`]
//!     seam) + the image-provider variants.
//!   - [`refusal`] — the ONE refusal classifier (`classify_refusal`, five
//!     ranked evidences over a structured provider error; P4.D225, v4
//!     `8bd080267`).
//!   - [`image_failover`] — the ONE post-hoc image failover chokepoint every
//!     image call site runs through (P4.D225, v4 `8bd080267`).
//!   - [`refusal_ledger`] — the per-chat count of STATED refusals and the
//!     Concierge's auto-switch to Unmoderated (P4.D225, v4 `49059fb14`; #75).
//!   - [`manual_flip`] — the ONE Concierge state-transition chokepoint.
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
pub mod classifier_switch;
pub mod current_state;
pub mod gatekeeper;
pub mod gatekeeper_job;
pub mod image_failover;
pub mod legacy_concierge_settings;
pub mod manual_flip;
pub mod moderation_wire;
mod prompt_text;
pub mod provider_routing;
pub mod refusal;
pub mod refusal_ledger;
pub mod resolver;
pub mod retry_uncensored;
pub mod understudy;
