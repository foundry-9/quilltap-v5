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
//!
//! Everything else (repos, services, the Request/Response/Event boundary)
//! lands in later phases.

pub mod dbkey;
pub mod memory_weighting;
pub mod recall_history;
pub mod recall_tags;
pub mod write_partition;
