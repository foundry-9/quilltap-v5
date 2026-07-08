//! Phase-3 **services** — the first code that makes decisions rather than just
//! persisting rows. Each service sits on the trusted Phase-2 data layer (repos +
//! the partitioned apply path) and the Phase-3 foundations (the writer-task
//! runtime [`crate::db::runtime`] and the model boundary [`crate::model`]), so any
//! failure localizes to the service, not the store.
//!
//! Ported so far:
//!
//! * [`memory_gate`] — the pre-write similarity gate (v4 `createMemoryWithGate` +
//!   `runMemoryGate`): the append-or-reinforce decision. The first model-dependent
//!   service, verified tier-3 → tier-2 (a canned embedding injected identically on
//!   both differential sides, then a structural DB diff).
//! * [`memory_service`] — the cascade-delete family (v4 `deleteMemoryWithVector` +
//!   the three `deleteMemoriesBy*WithVectors` cascades): the vector-store-aware
//!   wrappers around the deletion chokepoint. No model call; verified by a plain
//!   tier-2 differential.
//! * [`housekeeping`] — the retention sweep (v4 `runHousekeeping` /
//!   `needsHousekeeping`): protection-gated policy deletions, the opt-in
//!   stored-vector similarity merge, and cap enforcement, applied through the
//!   chokepoint. No model call; verified by a plain tier-2 differential.
//! * [`turn_orchestrator`] — the turn-orchestration decision core (v4
//!   `shouldChainNext` / `persistTurnParticipantId` + the `handleTurnAction`
//!   mutation core): the per-step chain decision (guards + all-LLM auto-pause +
//!   queue pop + weighted selection) and the nudge/queue/dequeue/skip/query turn
//!   actions, over the ported pure turn manager. No model call (the model-calling
//!   chain *driver* `executeTurnChain` is a later wave); RNG + wall clock
//!   injected; verified by a plain tier-2 differential.

pub mod aesthetics;
pub mod agent_mode;
pub mod answer_confirmation;
pub mod api_key_service;
pub mod appearance_resolution;
pub mod aurora_notifications;
pub mod avatar_generation;
pub mod avatar_prompt;
pub mod build_context;
pub mod carina_runner;
pub mod character_avatar_job;
pub mod chat_events;
pub mod chat_files;
pub mod cheap_llm_exec;
pub mod commonplace_notifications;
pub mod compression;
pub mod compression_cache;
pub mod concierge_notifications;
pub mod context_summary;
pub mod conversation_summary_vault_bridge;
pub mod core_whisper;
pub mod cost_events;
pub mod courier_transport;
pub mod dangerous_content;
pub mod file_fallback;
pub mod first_message_context;
pub mod frozen_archive;
pub mod host_notifications;
pub mod housekeeping;
pub mod housekeeping_outcome_cache;
pub mod image_job_common;
pub mod image_job_storage;
pub mod image_profile_resolution;
pub mod image_scene_tasks;
pub mod job_runner;
pub mod job_scheduler;
pub mod knowledge_injector;
pub mod lantern_notifications;
pub mod librarian_notifications;
pub mod llm_errors;
pub mod llm_logging;
pub mod maintenance;
pub mod memory_gate;
pub mod memory_processor;
pub mod memory_recap;
pub mod memory_service;
pub mod message_context;
pub mod message_finalizer;
pub mod native_tool_loop;
pub mod off_scene;
pub mod orchestrator;
pub mod participant_resolver;
pub mod pricing_fetcher;
pub mod primary_stream;
pub mod prospero_notifications;
pub mod provider_failover;
pub mod pseudo_tool;
pub mod queue_service;
pub mod recovery;
pub mod regenerate_swipe;
pub mod scene_state_tracking;
pub mod story_background_job;
pub mod suparna_mail;
pub mod suparna_notifications;
pub mod text_tool_loop;
pub mod tool_build;
pub mod tool_call_threading;
pub mod tool_execution;
pub mod turn_orchestrator;
pub mod user_identity_resolver;
pub mod wardrobe_transfers;
