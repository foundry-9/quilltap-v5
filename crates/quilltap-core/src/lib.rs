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
//!   * `about_character` / `memory_gate` / `memory_format` — the memory
//!     name-resolution leaves: about/holder name-set builders, the
//!     reinforced-importance formula, name+pronoun formatting, and the
//!     word-boundary name matchers (`name_appears` / `count_name_occurrences` /
//!     `resolve_about_character_id`, the lookahead reproduced without a
//!     backtracking engine); oracle-verified.
//!   * `message_attribution` — the per-character context shaping (history-access
//!     gate, presence windows, whisper visibility, role/name attribution);
//!     oracle-verified.
//!   * `mentioned_characters` — scanning a chat corpus for non-participant
//!     character mentions (ASCII-`\b` alternation, longest-token-first,
//!     lowercased token→ids map); oracle-verified.
//!   * `chat_tasks` / `chat_utils` — cheap-LLM-task text shaping: tool-artifact
//!     stripping, visible-conversation extraction, and the chat-card preview;
//!     oracle-verified. `jsstr` holds the shared JS string-semantics primitives
//!     (the JS `\s`/`trim` set, UTF-16 length/slice) these and the regex ports
//!     build on.
//!   * `jsnum` / `format_bytes` / `format_tokens` — JS number-formatting: the
//!     `Number.prototype.toFixed` kernel (`to_fixed`, V8 half-away-from-zero
//!     rounding on the exact f64 value) and the display formatters built on it
//!     (`format_bytes`, cost / token-count strings); oracle-verified. The
//!     lowercase-`k` token-count twin lives in `token_estimation`.
//!   * `embedding_vector` / `literal_boost` / `embedding_blob` — the embedding
//!     hot paths: L2 normalisation, the profile storage policy, cosine
//!     similarity + the dimension-mismatch guard, the fallback keyword/phrase
//!     scorer, the literal-phrase boost helpers, and Float32 ↔ LE-byte BLOB
//!     conversion; oracle-verified.
//!   * `canonicalize` — byte-stable `UniversalTool` serialization for cache-prefix
//!     stability: deep code-unit key-sort of `function.parameters` plus the
//!     tool-name array sort (a documented `localeCompare` seam — the lowercase
//!     snake_case tool-name corpus collates identically under code-unit order);
//!     oracle-verified.
//!   * `canon` / `scenario_text` — pure text assembly: the memory-extraction
//!     canon blocks (self / other ALREADY ESTABLISHED rendering) and the
//!     New-Chat scenario-text combiner; oracle-verified.
//!   * `model_classes` — the built-in LLM capability tiers and their lookups;
//!     oracle-verified.
//!   * `cheap_model` — the cheap-model classifiers (`is_cheap_model`,
//!     `estimate_model_cost`, `get_cheapest_model`) and their deprecated fallback
//!     tables; the registry-sourced recommended-list / default-model are injected
//!     (the string heuristics are pure); oracle-verified.
//!   * `model_context` — the context-window lookup (`get_model_context_limit` +
//!     `has_extended_context` / `get_safe_input_limit`): its override/default
//!     tables ported as constants, with the plugin model-info / `FALLBACK_PRICING`
//!     rows / registry default injected; oracle-verified.
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
//!   * `clock` — the wall-clock seam: ISO-8601 timestamps in v4's
//!     `new Date().toISOString()` shape (`now_iso`, pure `iso_from_unix_ms`),
//!     used by the repo create/update default path.
//!   * small pure leaf utilities, each mirroring a v4 file: `chat_predicates`
//!     (chat-type / participant-status predicates), `semver` (parse + compare),
//!     `pronoun_gender` (image-prompt gender hint), `tag_style` (style merge),
//!     `char_count` (count-indicator colour class); all oracle-verified.
//!
//! Everything else (repos, services, the Request/Response/Event boundary)
//! lands in later phases.

pub mod about_character;
pub mod all_llm_pause;
pub mod canon;
pub mod canonicalize;
pub mod char_count;
pub mod chat_predicates;
pub mod chat_tasks;
pub mod chat_utils;
pub mod cheap_model;
pub mod clock;
pub mod context_budget;
pub mod context_compression;
pub mod context_summary;
pub mod db;
pub mod dbkey;
pub mod embedding_blob;
pub mod embedding_vector;
pub mod enclave_budget;
pub mod format_bytes;
pub mod format_tokens;
pub mod jsnum;
pub mod jsstr;
pub mod literal_boost;
pub mod memory_format;
pub mod memory_gate;
pub mod memory_weighting;
pub mod mentioned_characters;
pub mod message_attribution;
pub mod model_classes;
pub mod model_context;
pub mod participant_filters;
pub mod pricing;
pub mod pronoun_gender;
pub mod recall_history;
pub mod recall_tags;
pub mod scenario_text;
pub mod select_speaker;
pub mod semver;
pub mod tag_style;
pub mod token_estimation;
pub mod turn_order;
pub mod turn_state;
pub mod write_partition;
