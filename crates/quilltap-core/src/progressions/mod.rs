//! Character progressions — timed conditions a character carries, reported per
//! turn (v4 `lib/progressions/`, added at `0587d1e96`, ported at `25f534c0b`).
//!
//! A **progression** is a named, bounded span of time: a pregnancy due in May, a
//! cannon that recharges in ten minutes, a fermentation that finishes in three
//! weeks. The schema ([`schema`]) says what one is and where it lives — one
//! reserved `progressions` key in the character vault's `metadata.json`, no
//! table and no column. The engine ([`engine`]) derives its state against an
//! injected clock, decides whether this turn should mention it, renders the line
//! the character reads, and flattens the lot into the primitive sheet Pascal's
//! comparators read.
//!
//! Both halves are pure: no DB, no tracing, no I/O, no prompt text and no Pascal
//! knowledge. The logging reader is P4.D168's `prompt_section`, which passes an
//! `on_issue` sink down; the write path is P4.D169's applier, which mutates the
//! raw `serde_json::Map` rather than round-tripping [`schema::Progression`].

pub mod engine;
// === P4.D168 ===
pub mod prompt_section;
// === /P4.D168 ===
pub mod schema;

pub use engine::{
    default_in_progress_template, derive_progression, flatten_progressions, format_span,
    format_span_whole, infer_increment, parse_progressions, parse_report_period_ms,
    progression_placeholders, render_progression_report, should_report_progression, unit_ms,
    DerivedProgression, ProgressPrimitive, ProgressionState, RenderProgressionOptions,
    ReportReason, ShouldReportResult, UNIT_MS,
};
pub use schema::{
    is_progression_id, is_report_frequency, is_writable_progression_field, join_issues,
    parse_iso_instant, parse_iso_instant_str, parse_progress_key, parse_progression, OnComplete,
    Progression, ProgressionQuantity, TimeIncrement, ZodIssue, MAX_PROGRESSIONS_PER_CHARACTER,
    MAX_PROGRESSION_DESCRIPTION_LENGTH, MAX_PROGRESSION_NAME_LENGTH, MAX_QUANTITY_UNIT_LENGTH,
    MAX_REPORT_TEMPLATE_LENGTH, PROGRESSIONS_METADATA_KEY, WRITABLE_PROGRESSION_FIELDS,
};
