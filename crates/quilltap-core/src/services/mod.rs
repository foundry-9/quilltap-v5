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
pub mod annotations;
pub mod answer_confirmation;
pub mod api_key_service;
pub mod appearance_resolution;
pub mod ariel_notifications;
pub mod aurora_notifications;
pub mod avatar_generation;
pub mod avatar_prompt;
// === P4.9G5 ===
pub mod backup;
// === end P4.9G5 ===
pub mod brahma_console;
pub mod build_context;
pub mod builtin_embedding;
pub mod builtin_mounts;
pub mod builtin_templates;
pub mod carina_memory_extraction;
pub mod carina_query;
pub mod carina_runner;
pub mod cascade_delete;
pub mod character_avatar_job;
pub mod character_enrichment;
pub mod chat_continuation;
pub mod chat_create;
pub mod chat_enrichment;
pub mod chat_events;
pub mod chat_files;
pub mod chat_initialize;
pub mod cheap_llm_exec;
pub mod cold_chunk_reembed;
pub mod collapse_stale_chat_caches;
pub mod commonplace_notifications;
pub mod compression;
pub mod compression_cache;
pub mod concierge_notifications;
pub mod context_summary;
pub mod context_summary_job;
pub mod conversation_summaries_regen;
pub mod conversation_summary_vault_bridge;
pub mod core_whisper;
pub mod cost_estimation;
pub mod cost_events;
pub mod courier_transport;
pub mod creation_progress;
pub mod danger_scan;
pub mod dangerous_content;
// === P4.9c: the data-directory resolver (lane C, append-only) ===
pub mod data_dir;
// === end P4.9c ===
// === P4.9G3: the delete-all-data family (lane G3, append-only) ===
pub mod delete_all;
// === end P4.9G3 ===
// === P4.6BL ===
pub mod embedding_generate_job;
// === end P4.6BL ===
pub mod embedding_provider;
pub mod embedding_reapply_profile;
pub mod embedding_refit_job;
pub mod file_fallback;
pub mod file_storage;
pub mod first_message_context;
pub mod fold_episode_pass;
pub mod frozen_archive;
pub mod help_doc_sync;
pub mod home;
pub mod host_notifications;
pub mod housekeeping;
pub mod housekeeping_outcome_cache;
pub mod image_job_common;
pub mod image_job_storage;
pub mod image_profile_resolution;
pub mod image_scene_tasks;
pub mod initial_greeting;
pub mod job_runner;
pub mod job_scheduler;
pub mod knowledge_injector;
pub mod lantern_notifications;
pub mod librarian_notifications;
pub mod llm_errors;
pub mod llm_log_cleanup_job;
pub mod llm_logging;
pub mod maintenance;
pub mod memory_dedup;
pub mod memory_extraction_job;
pub mod memory_gate;
pub mod memory_processor;
pub mod memory_recap;
pub mod memory_service;
pub mod message_context;
pub mod message_finalizer;
pub mod mount_index;
pub mod native_tool_loop;
pub mod off_scene;
pub mod orchestrator;
pub mod outfit_selections;
pub mod participant_resolver;
pub mod pascal_writer;
pub mod pre_compute;
pub mod pricing_fetcher;
pub mod primary_stream;
// ── P4.9G4 ──
pub mod profile_names;
// ── end P4.9G4 ──
pub mod prospero_notifications;
pub mod provider_failover;
pub mod provisioning;
pub mod pseudo_tool;
pub mod queue_service;
// ── P4.9G4 ──
pub mod qtap_export;
// ── end P4.9G4 ──
pub mod quilltap_import;
pub mod recall_replay;
pub mod recovery;
pub mod regenerate_swipe;
pub mod scene_state_tracking;
pub mod scheduled_maintenance;
pub mod sillytavern;
pub mod story_background_job;
pub mod suparna_mail;
pub mod suparna_notifications;
pub mod system_prompt_compiler;
pub mod text_tool_loop;
pub mod title_update_job;
pub mod tool_build;
pub mod tool_call_threading;
pub mod tool_execution;
pub mod turn_orchestrator;
pub mod turn_transcript;
pub mod user_identity_resolver;
pub mod wardrobe_transfers;
// ── P4.9E2A ──
pub mod announcer;
// ── end P4.9E2A ──
// ── P4.9E1A: the chat cast + avatar-override service layer ──
pub mod chat_avatars;
pub mod chat_participants;
// ── end P4.9E1A ──
// ── P4.9E3A ──
pub mod chat_admin;
pub mod chat_merge;
pub mod chat_rng;
pub mod chat_run_tool;

// === P4.9E3B: the chat-dialog server remainder ===
pub mod chat_export;
pub mod message_reattribute;
pub mod search_replace;
pub mod tools_inventory;
// === end P4.9E3B ===
// ── end P4.9E3A ──

// === P4.6BM: the embedding-family remainder ===
pub mod conversation_markdown;
pub mod conversation_render_job;
pub mod conversation_render_reconcile;
pub mod embedding_reindex_job;
// === end P4.6BM ===

// === P4.d27 (v4 `7391404e`) ===
/// The boot-time embedding-dimension self-heal (v4 `instrumentation.ts`
/// PHASE 3.7), wired immediately after the render reconcile.
pub mod embedding_dimension_reconcile;
// === end P4.d27 ===
// === P4.d28: the Markdown transcript export ===
pub mod markdown_transcript;
// === end P4.d28 ===
