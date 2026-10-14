//! Port of v4 `lib/memory/time/age-label.ts` — the age label a memory carries
//! into a prompt ("3 days ago", "last week").
//!
//! v4's why, carried: it reads off the EVENT clock — `occurredAt` when
//! present, else the write/reinforce clock — so "last week" means when it
//! happened, not when it was written down. Decay (the scoring module)
//! deliberately stays on the write clock; only the human-facing label follows
//! the event. Byte-identical to the deleted `formatRelativeAge`.

/// Human-readable age of a memory, on the event clock (v4 `formatMemoryAge`).
/// `created_at_ms` / `last_reinforced_at_ms` are as
/// [`crate::memory_scoring::SalienceInput`] carries them.
pub fn format_memory_age(
    created_at_ms: f64,
    last_reinforced_at_ms: Option<f64>,
    occurred_at: Option<&str>,
    now_ms: f64,
) -> String {
    let write_clock =
        crate::memory_scoring::reference_time_ms(created_at_ms, last_reinforced_at_ms);
    let reference = crate::episodic::event_reference_time_ms(occurred_at, write_clock);
    crate::format_time::format_relative_days(reference, now_ms)
}
