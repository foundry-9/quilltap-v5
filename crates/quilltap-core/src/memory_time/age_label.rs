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

#[cfg(test)]
mod jest_twins {
    //! Case-for-case port of the `formatRelativeAge` describe of v4
    //! `__tests__/unit/lib/memory/scoring/protection-and-age.test.ts` (the pin
    //! `83334b9a5`; v4 imports `formatMemoryAge as formatRelativeAge`). v4's
    //! `makeMemory` fixture carries no `occurredAt`, so every case reads the
    //! write clock; `NOW` is the explicit `now_ms`.

    use super::format_memory_age;
    use crate::memory_scoring::{date_ms, truthy_date_ms};
    use crate::memory_time::calendar::js_iso_string;

    fn now_ms() -> f64 {
        date_ms("2026-03-01T00:00:00.000Z")
    }

    fn days_ago(d: f64) -> String {
        js_iso_string(now_ms() - d * 86_400_000.0).expect("in range")
    }

    /// `formatRelativeAge(makeMemory({ createdAt, lastReinforcedAt }), NOW)`.
    fn age(created_at: &str, last_reinforced_at: Option<&str>) -> String {
        format_memory_age(
            date_ms(created_at),
            truthy_date_ms(last_reinforced_at),
            None,
            now_ms(),
        )
    }

    /// `it('returns "today" for a memory created now')`.
    #[test]
    fn returns_today_for_a_memory_created_now() {
        let now_iso = js_iso_string(now_ms()).unwrap();
        assert_eq!(age(&now_iso, None), "today");
    }

    /// `it('returns "yesterday" for a memory created 1 day ago')`.
    #[test]
    fn returns_yesterday_for_a_memory_created_1_day_ago() {
        assert_eq!(age(&days_ago(1.0), None), "yesterday");
    }

    /// `it('returns "N days ago" for memories 2-6 days old')`.
    #[test]
    fn returns_n_days_ago_for_memories_2_6_days_old() {
        assert_eq!(age(&days_ago(4.0), None), "4 days ago");
    }

    /// `it('returns "last week" for memories 7-13 days old')`.
    #[test]
    fn returns_last_week_for_memories_7_13_days_old() {
        assert_eq!(age(&days_ago(10.0), None), "last week");
    }

    /// `it('returns "N weeks ago" for memories 14-29 days old')`.
    #[test]
    fn returns_n_weeks_ago_for_memories_14_29_days_old() {
        assert_eq!(age(&days_ago(21.0), None), "3 weeks ago");
    }

    /// `it('returns "last month" for memories 30-59 days old')`.
    #[test]
    fn returns_last_month_for_memories_30_59_days_old() {
        assert_eq!(age(&days_ago(45.0), None), "last month");
    }

    /// `it('returns "N months ago" for memories 60-364 days old')`.
    #[test]
    fn returns_n_months_ago_for_memories_60_364_days_old() {
        assert_eq!(age(&days_ago(90.0), None), "3 months ago");
    }

    /// `it('returns "1 year ago" for memories 365+ days old')`.
    #[test]
    fn returns_1_year_ago_for_memories_365_plus_days_old() {
        assert_eq!(age(&days_ago(400.0), None), "1 year ago");
    }

    /// `it('returns "2 years ago" for memories 730+ days old')`.
    #[test]
    fn returns_2_years_ago_for_memories_730_plus_days_old() {
        assert_eq!(age(&days_ago(800.0), None), "2 years ago");
    }

    /// `it('uses lastReinforcedAt when more recent than createdAt')`.
    #[test]
    fn uses_last_reinforced_at_when_more_recent_than_created_at() {
        assert_eq!(
            age(&days_ago(90.0), Some(&days_ago(2.0))), // 3 months ago / 2 days ago
            "2 days ago"
        );
    }
}
