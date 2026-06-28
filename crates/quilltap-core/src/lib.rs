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
//!   * `context_summary` — the rolling-window summarisation cadence (fold/hard
//!     gate, interchange count, title-check crossing, turn partition);
//!     oracle-verified.
//!   * `context_budget` — the per-purpose token-allocation arithmetic over a
//!     model's context window (summarize trigger, recent-message count,
//!     max-available, allocation split); oracle-verified.
//!   * `enclave_budget` — the autonomous-run budget arithmetic: the pre-turn
//!     exhaustion verdict and the progress-toward-binding-cap fraction that
//!     drives pacing milestones; oracle-verified.
//!   * `pricing` — the pure LLM cost arithmetic (`estimate_cost`) plus the
//!     cost-aware model-selection helpers; oracle-verified.
//!   * `about_character` / `memory_gate` / `memory_format` — the pure memory
//!     name-resolution leaves: about/holder name-set builders, the
//!     reinforced-importance formula, and name+pronoun formatting (the
//!     regex-based about-character matchers are deferred); oracle-verified.
//!   * `message_attribution` — the per-character context shaping (history-access
//!     gate, presence windows, whisper visibility, role/name attribution);
//!     oracle-verified.
//!   * `model_classes` — the built-in LLM capability tiers and their lookups;
//!     oracle-verified.
//!   * `token_estimation` — character-based token counting (estimate / per-message
//!     / per-conversation, truncation, context-usage %); oracle-verified.
//!   * `turn_state` — the multi-character turn-rotation state machine (queue
//!     ops, history-derived state, the spoken-this-cycle wrap); oracle-verified.
//!   * `all_llm_pause` — the logarithmic auto-pause thresholds for all-LLM
//!     chats; oracle-verified.
//!   * `participant_filters` — presence/control filters over a participant list
//!     (user/LLM/active resolvers); oracle-verified.
//!   * `turn_order` — the display-only predicted turn order for the participant
//!     sidebar; oracle-verified.
//!   * `select_speaker` — the weighted-random next-speaker selection (RNG
//!     injected as `random01`); oracle-verified.
//!   * small pure leaf utilities, each mirroring a v4 file: `chat_predicates`
//!     (chat-type / participant-status predicates), `semver` (parse + compare),
//!     `pronoun_gender` (image-prompt gender hint), `tag_style` (style merge),
//!     `char_count` (count-indicator colour class); all oracle-verified.
//!
//! Everything else (repos, services, the Request/Response/Event boundary)
//! lands in later phases.

pub mod about_character;
pub mod all_llm_pause;
pub mod char_count;
pub mod chat_predicates;
pub mod context_budget;
pub mod context_compression;
pub mod context_summary;
pub mod dbkey;
pub mod enclave_budget;
pub mod memory_format;
pub mod memory_gate;
pub mod memory_weighting;
pub mod message_attribution;
pub mod model_classes;
pub mod participant_filters;
pub mod pricing;
pub mod pronoun_gender;
pub mod recall_history;
pub mod recall_tags;
pub mod select_speaker;
pub mod semver;
pub mod tag_style;
pub mod token_estimation;
pub mod turn_order;
pub mod turn_state;
pub mod write_partition;
