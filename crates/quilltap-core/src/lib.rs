//! quilltap-core — the portable engine.
//!
//! Phase-0/1 surface is small and growing:
//!   * `dbkey` — recovers the master pepper from the on-disk `quilltap.dbkey`
//!     file (AES-256-GCM + PBKDF2). NB: this unwraps the FILE; the DATABASES
//!     themselves are ChaCha20/sqleet, not SQLCipher — see CLAUDE.md.
//!   * `memory_weighting` — pure scoring functions ported from v4 and verified
//!     against the differential oracle.
//!   * `recall_tags` — recall-side targeting-tag multipliers (scope/project
//!     gating, temporal/context/participant/anti-repetition), likewise
//!     oracle-verified.
//!   * `recall_history` — the per-chat anti-repetition ring buffer (producer of
//!     the "recently whispered" set `recall_tags` consumes), oracle-verified.
//!   * `write_partition` — parent-side write-batch classification, per-database
//!     partitioning, main-primary policy, and the folder-conflict id remap;
//!     oracle-verified.
//!   * `context_compression` — the pure sliding-window compression sizing
//!     (triggers, message split, history block); oracle-verified.
//!   * `context_budget` — the per-purpose token-allocation arithmetic over a
//!     model's context window (summarize trigger, recent-message count,
//!     max-available, allocation split); oracle-verified.
//!   * `enclave_budget` — the autonomous-run budget arithmetic: the pre-turn
//!     exhaustion verdict and the progress-toward-binding-cap fraction that
//!     drives pacing milestones; oracle-verified.
//!   * `pricing` — the pure LLM cost arithmetic (`estimate_cost`) plus the
//!     cost-aware model-selection helpers; oracle-verified.
//!   * `model_classes` — the built-in LLM capability tiers and their lookups;
//!     oracle-verified.
//!   * `token_estimation` — character-based token counting (estimate / per-message
//!     / per-conversation, truncation, context-usage %); oracle-verified.
//!
//! Everything else (repos, services, the Request/Response/Event boundary)
//! lands in later phases.

pub mod context_budget;
pub mod context_compression;
pub mod dbkey;
pub mod enclave_budget;
pub mod memory_weighting;
pub mod model_classes;
pub mod pricing;
pub mod recall_history;
pub mod recall_tags;
pub mod token_estimation;
pub mod write_partition;
