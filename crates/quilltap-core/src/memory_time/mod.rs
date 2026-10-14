//! Port of v4's `lib/memory/time/` (`e0ac7493a`; the calendar is
//! `c2795733f`'s `memory-calendar.ts` renamed — bug 212) — the ONE memory
//! calendar and the ONE day-reference rule table, read both ways.
//!
//! - [`calendar`]: the memory-calendar zone and v4's zoned arithmetic (the
//!   two-pass `zonedTimeToUtcMs`, copied literally).
//! - [`rules`]: the nine-rule day-reference table.
//! - [`clock`]: the write side (`resolve_event_time` stamps the window START)
//!   and the read side (`resolve_reference_window`, every window closing 1 ms
//!   early).
//! - [`anchors`][]: `resolve_episodic_anchors`.
//! - [`age_label`][]: `format_memory_age`.
//!
//! Every function here takes its zone EXPLICITLY — core never reads an
//! ambient zone (`host_zone.rs`). The stateful five-minute instance-zone
//! refresh lives in [`crate::services::memory_time_zone`].

pub mod age_label;
pub mod anchors;
pub mod calendar;
pub mod clock;
pub mod rules;

/// v4 `TimeWindow` (`{ from, to }`, closed ISO bounds) — the ONE window type
/// (LR-11: the distill and the recall context already share it).
pub use crate::recall_tags::TimeWindow;
pub use age_label::format_memory_age;
pub use anchors::{resolve_episodic_anchors, EpisodicAnchorInput, EpisodicAnchors};
pub use calendar::{
    is_valid_time_zone, memory_time_zone, resolve_zone, start_of_zoned_day_ms,
    start_of_zoned_week_ms, zoned_date_stamp, zoned_day_bounds_iso, zoned_parts,
    zoned_time_on_day_ms, zoned_time_to_utc_ms, ZonedParts, MEMORY_CALENDAR_DAY_MS,
};
pub use clock::{
    clock_for, normalise_time_range, resolve_day_reference, resolve_event_time,
    resolve_reference_window, resolve_when_phrase, DayReferenceResolution, EventTime, MemoryClock,
};
pub use rules::{
    day_number_words_pattern, day_reference_scan_source, match_day_reference_rule,
    DayReferenceRule, RuleWindow, DAY_NUMBER_WORDS, DAY_REFERENCE_SCAN_PATTERN,
    DAY_REFERENCE_TABLE, MAX_TABLE_DAYS_AGO,
};
