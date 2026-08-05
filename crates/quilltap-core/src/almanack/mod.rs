//! The Almanack (System Report) — v4 `lib/tools/almanack/**` (`0cde7fbc`).
//!
//! An annual compendium of the establishment: what is installed, what is
//! configured, and what has accumulated. Formerly "the capabilities report";
//! renamed in v4 4.9 when its coverage was brought up to date with the
//! document-store cutovers it had never learned about. v5 never ported the old
//! collector (M6 parity row 578 said MISSING), so this is a port of the surface
//! v4 actually ships rather than of the thing it replaced.
//!
//! The pipeline walks the seven phases of [`phases::ALMANACK_PHASES`] in order;
//! within a phase the collectors are joined concurrently, but the phases
//! themselves run sequentially so the progress bar can name the one that is
//! actually running. Every collector is wrapped in [`db::collect`], which
//! swallows and logs failures: a report that refuses to render because one
//! section's query threw is worse than a report with one hollow section.
//!
//! ## Divergences from v4, recorded (all differential-normalized)
//!
//! - **`runtimeEnvironment` reports v5's own runtime.** The section is a
//!   truthful description of the process that produced the report, so a Rust
//!   host reports Rust facts (no `process.version`; `nodeVersion` carries the
//!   host runtime string). The labels are byte-matched; the tier-2 differential
//!   normalizes the whole block, exactly as it normalizes `generatedAt`.
//! - **`version` is v5's** crate version, not v4's `package.json`.
//! - **No `capabilities-report-progress` SSE action.** v5 has ONE global
//!   `/api/events` stream, so progress frames ride it scope-tagged by
//!   `progressId` — the standing D3-family divergence the Green Room
//!   established ([`crate::services::creation_progress`]). The new `phase`
//!   frame kind is emitted there.
//! - **The llm-logs aggregates live here**, as raw SQL over the llm-logs
//!   connection, rather than on a user-scoped repository layer v5 does not
//!   have (a structural no-op — the SQL is v4's, byte-for-byte).

pub mod phases;
pub mod render;
pub mod types;

pub use phases::{
    phase_index, AlmanackPhase, ALMANACK_NAME, ALMANACK_PHASES, ALMANACK_PHASE_COUNT,
    ALMANACK_TITLE,
};
pub use render::render_almanack_markdown;
pub use types::AlmanackReportData;
