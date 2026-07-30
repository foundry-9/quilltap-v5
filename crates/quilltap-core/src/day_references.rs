//! Port of lib/memory/day-references.ts (v4 `505dcb1f`) — deterministic
//! day-reference resolution.
//!
//! The per-turn distillation (`extractMemorySearchKeywords`) asks a cheap LLM to
//! decide whether a turn is retrospective and to resolve any referenced period
//! into an absolute `timeRange`. Small models reliably fumble the *same-day*
//! case: "the mission today" reads as present tense to them, so the whole
//! episodic path (temporal flip, `occurredWithin` window, multi-probe) stays
//! inert on exactly the turn that needs it most. Worse, when a model does emit a
//! range it resolves the calendar in UTC — at 21:44 CDT "today" is already
//! tomorrow in UTC, and the window misses every memory it was meant to catch.
//!
//! This module resolves the common English day references deterministically,
//! against the SERVER-LOCAL calendar. Quilltap is self-hosted, so the server's
//! local timezone IS the user's timezone: v4 uses plain `Date` local-time
//! accessors (`getFullYear`/`getMonth`/`getDate`, `new Date(y, m, d)`) and its
//! header warns "Never use the `getUTC*` family for these boundaries".
//!
//! **The one v5 seam:** v4 reads the *ambient process zone*; core never does
//! (parallel test threads would race, and `jiff::tz::TimeZone::system()` reads
//! the environment at lookup). The zone arrives as an explicit IANA name
//! instead — see [`resolve_day_reference`]. A single fixed offset would NOT do:
//! `new Date(y, m, d)` resolves the offset *per constructed day*, so a window
//! spanning a DST transition needs real zone math.
//!
//! Pure + I/O-free — no logging, no DB, no LLM — so it is trivially
//! unit-testable and safe to call from the job child (same contract as
//! [`crate::recall_tags`]).

use std::sync::LazyLock;

use jiff::tz::TimeZone;
use regex::Regex;

use crate::clock::iso_from_unix_ms;
use crate::recall_tags::TimeWindow;

/// v4 `DayReferenceResolution`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayReferenceResolution {
    /// Absolute UTC window covering the referenced local calendar period. ISO
    /// strings via `toISOString()` — consumers compare with `Date.parse`, so the
    /// UTC rendering of a local boundary is exactly right.
    ///
    /// Only meaningful when [`Self::past_pointing`] is true: callers must NOT
    /// apply the window of a future-pointing reference (it is computed and
    /// returned for debug logging only).
    pub time_range: TimeWindow,
    /// True when the reference points at the past (today/yesterday/…), false for
    /// the future ("tomorrow").
    pub past_pointing: bool,
    /// Which phrase matched — for debug logging and the replay output. Always
    /// the lowercased, trimmed form (v4 returns `matched: p`, the normalized
    /// phrase, not the source casing).
    pub matched: String,
}

/// Number words the "N days ago" form accepts, alongside digits 1–14. Insertion
/// order is the alternation order in [`DAY_REFERENCE_PATTERN`] (v4 builds it
/// from `Object.keys(NUMBER_WORDS)`).
const NUMBER_WORDS: [(&str, i64); 14] = [
    ("one", 1),
    ("two", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
    ("eleven", 11),
    ("twelve", 12),
    ("thirteen", 13),
    ("fourteen", 14),
];

/// Largest `N days ago` the lexicon resolves (beyond this, leave it to the LLM).
const MAX_DAYS_AGO: i64 = 14;

/// The lexicon as a single ordered alternation. Order matters twice over:
/// longest-first so "day before yesterday" is consumed whole (never re-read as a
/// bare "yesterday"), and the scan resumes after each match so an inner phrase
/// can never win the latest-match tie-break against its own container.
///
/// JS `\b` and `\d` are ASCII-only, so the Rust pattern uses `(?-u:\b)` and an
/// explicit `[0-9]` class (the `starts_with_date_prefix` precedent in
/// `distill.rs`). `(?i)` mirrors the `i` flag; the `g` flag is the `find_iter`
/// loop below. The `regex` crate's alternation is leftmost-FIRST (same
/// preference order as V8's backtracking engine), which is what makes the
/// longest-first ordering load-bearing on both sides.
static DAY_REFERENCE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    let words: Vec<&str> = NUMBER_WORDS.iter().map(|(w, _)| *w).collect();
    let alternation = [
        "day before yesterday".to_string(),
        "last night".to_string(),
        "earlier today".to_string(),
        "this morning".to_string(),
        "this afternoon".to_string(),
        "this evening".to_string(),
        "tonight".to_string(),
        "yesterday".to_string(),
        "today".to_string(),
        format!("(?:[0-9]{{1,2}}|{}) days? ago", words.join("|")),
        "this week".to_string(),
        "last week".to_string(),
        "next week".to_string(),
        "tomorrow".to_string(),
    ]
    .join("|");
    Regex::new(&format!("(?i)(?-u:\\b)(?:{alternation})(?-u:\\b)"))
        .expect("day-reference pattern compiles")
});

/// The local civil date + weekday of an instant in `zone` — v4's
/// `getFullYear`/`getMonth`/`getDate`/`getDay` on the input `Date`.
fn local_date(zone: &TimeZone, now_ms: i64) -> Option<jiff::civil::Date> {
    let ts = jiff::Timestamp::from_millisecond(now_ms).ok()?;
    Some(zone.to_datetime(ts).date())
}

/// Local midnight of the day `offset_days` from the local day containing the
/// instant — v4 `startOfLocalDay`.
///
/// v4 writes `new Date(y, m, date + offsetDays, 0, 0, 0, 0)`, whose day
/// overflow/underflow normalizes to plain day arithmetic on the local date
/// (JS `MakeDay(y, m, 1) + (d + offset - 1)`), so `checked_add(days)` is exact.
/// A local midnight that does not exist (a spring-forward gap at 00:00 — Havana,
/// Santiago, Lord Howe) resolves through jiff's `compatible` disambiguation,
/// which is V8's own choice for gaps (shift forward past the gap) and for folds
/// (take the earlier offset).
fn start_of_local_day(zone: &TimeZone, date: jiff::civil::Date, offset_days: i64) -> Option<i64> {
    local_time_on_day(zone, date, offset_days, 0)
}

/// Local wall-clock hour on the day `offset_days` from the local day containing
/// the instant — v4 `localTimeOnDay`.
fn local_time_on_day(
    zone: &TimeZone,
    date: jiff::civil::Date,
    offset_days: i64,
    hours: i8,
) -> Option<i64> {
    let shifted = date
        .checked_add(jiff::Span::new().days(i32::try_from(offset_days).ok()?))
        .ok()?;
    let dt = shifted.to_datetime(jiff::civil::time(hours, 0, 0, 0));
    let ts = zone.to_ambiguous_timestamp(dt).compatible().ok()?;
    Some(ts.as_millisecond())
}

/// Local midnight of the most recent Monday (today, when today is Monday) —
/// v4 `startOfLocalWeek`. `getDay()`: 0 = Sunday … 6 = Saturday, Monday-anchored
/// weeks, so Sunday is 6 days in.
fn start_of_local_week(zone: &TimeZone, date: jiff::civil::Date, week_offset: i64) -> Option<i64> {
    // jiff's `Weekday::to_sunday_zero_offset()` is JS `getDay()` exactly.
    let js_day = date.weekday().to_sunday_zero_offset() as i64;
    let days_since_monday = (js_day + 6) % 7;
    start_of_local_day(zone, date, -days_since_monday + week_offset * 7)
}

/// v4 `window(from, to)` — both ends via `.toISOString()`.
fn window(from_ms: i64, to_ms: i64) -> TimeWindow {
    TimeWindow {
        from: iso_from_unix_ms(from_ms),
        to: iso_from_unix_ms(to_ms),
    }
}

/// Resolve one matched phrase into its window — v4 `resolvePhrase`.
fn resolve_phrase(
    phrase: &str,
    zone: &TimeZone,
    date: jiff::civil::Date,
    now_ms: i64,
) -> Option<DayReferenceResolution> {
    // v4 `phrase.toLowerCase().trim()`.
    let p = crate::jsstr::js_trim(&phrase.to_lowercase()).to_string();

    let hit = |from: i64, to: i64, past_pointing: bool| {
        Some(DayReferenceResolution {
            time_range: window(from, to),
            past_pointing,
            matched: p.clone(),
        })
    };

    // Today and its parts — from local midnight up to the current instant. A
    // sub-day phrase ("this morning") deliberately does NOT narrow the window:
    // memories carry coarse event times, and a too-tight window starves recall.
    if matches!(
        p.as_str(),
        "today" | "earlier today" | "this morning" | "this afternoon" | "this evening" | "tonight"
    ) {
        return hit(start_of_local_day(zone, date, 0)?, now_ms, true);
    }

    if p == "last night" {
        return hit(
            local_time_on_day(zone, date, -1, 18)?,
            local_time_on_day(zone, date, 0, 6)?,
            true,
        );
    }

    if p == "yesterday" {
        return hit(
            start_of_local_day(zone, date, -1)?,
            start_of_local_day(zone, date, 0)?,
            true,
        );
    }

    if p == "day before yesterday" {
        return hit(
            start_of_local_day(zone, date, -2)?,
            start_of_local_day(zone, date, -1)?,
            true,
        );
    }

    if p == "this week" {
        return hit(start_of_local_week(zone, date, 0)?, now_ms, true);
    }

    if p == "last week" {
        return hit(
            start_of_local_week(zone, date, -1)?,
            start_of_local_week(zone, date, 0)?,
            true,
        );
    }

    // Future references resolve a window too, but purely for the debug log —
    // the merge policy applies a window only when `pastPointing` is true.
    if p == "tomorrow" {
        return hit(
            start_of_local_day(zone, date, 1)?,
            start_of_local_day(zone, date, 2)?,
            false,
        );
    }

    if p == "next week" {
        return hit(
            start_of_local_week(zone, date, 1)?,
            start_of_local_week(zone, date, 2)?,
            false,
        );
    }

    // v4 `p.match(/^(\S+) days? ago$/)`.
    if let Some(raw) = days_ago_token(&p) {
        // `/^\d{1,2}$/.test(raw) ? Number(raw) : NUMBER_WORDS[raw]`.
        let n = if (1..=2).contains(&raw.len()) && raw.bytes().all(|b| b.is_ascii_digit()) {
            raw.parse::<i64>().ok()
        } else {
            NUMBER_WORDS
                .iter()
                .find(|(w, _)| *w == raw)
                .map(|(_, n)| *n)
        };
        // v4 `if (!n || n < 1 || n > MAX_DAYS_AGO) return null` — `Number("0")`
        // is falsy, so "0 days ago" (and any unknown word) resolves to null.
        let n = match n {
            Some(n) if (1..=MAX_DAYS_AGO).contains(&n) => n,
            _ => return None,
        };
        return hit(
            start_of_local_day(zone, date, -n)?,
            start_of_local_day(zone, date, -n + 1)?,
            true,
        );
    }

    None
}

/// v4's `/^(\S+) days? ago$/` capture: the leading non-whitespace token of an
/// `N day(s) ago` phrase, or `None` when the phrase is not that shape.
fn days_ago_token(p: &str) -> Option<&str> {
    let head = p
        .strip_suffix(" days ago")
        .or_else(|| p.strip_suffix(" day ago"))?;
    // `\S+` — one or more non-whitespace, so a head with a space (or empty)
    // fails the match.
    if head.is_empty() || head.chars().any(char::is_whitespace) {
        return None;
    }
    Some(head)
}

/// Scan conversation text for an explicit day reference and resolve it against
/// `now_ms` using the local calendar of `tz`. Returns `None` when no reference
/// is found. Later (more recent) matches in the text win over earlier ones — the
/// most recent utterance reflects the turn's current focus, so "we talked about
/// that yesterday, but today…" resolves to today.
///
/// `now_ms` mirrors v4's `now: Date`: a non-finite value is v4's Invalid Date
/// (`!Number.isFinite(now.getTime())` → `null`), and a fractional value
/// truncates toward zero exactly as `new Date(ms)` does.
///
/// `tz` is an IANA zone name — the v5 seam standing in for v4's ambient process
/// zone (see the module header). An unresolvable name falls back to UTC, i.e. a
/// server whose zone cannot be named behaves like a UTC server; v4 cannot reach
/// that arm (the process zone always resolves).
pub fn resolve_day_reference(text: &str, now_ms: f64, tz: &str) -> Option<DayReferenceResolution> {
    // v4 `if (!text || !Number.isFinite(now.getTime())) return null`.
    if text.is_empty() || !now_ms.is_finite() {
        return None;
    }
    let now_ms = now_ms.trunc() as i64;
    let zone = TimeZone::get(tz).unwrap_or(TimeZone::UTC);
    let date = local_date(&zone, now_ms)?;

    let mut latest: Option<DayReferenceResolution> = None;
    // `find_iter` is v4's `exec` loop: non-overlapping, resuming after each
    // match (so an inner phrase never wins against its own container), and the
    // zero-length-match guard v4 carries is structurally unnecessary here —
    // `find_iter` cannot loop forever.
    for m in DAY_REFERENCE_PATTERN.find_iter(text) {
        // An unresolvable match (e.g. "97 days ago") still consumed its span;
        // the previous winner stands rather than being clobbered by a null.
        if let Some(resolved) = resolve_phrase(m.as_str(), &zone, date, now_ms) {
            latest = Some(resolved);
        }
    }
    latest
}

#[cfg(test)]
mod tests {
    //! Case-for-case port of v4 `lib/memory/__tests__/day-references.test.ts`.
    //!
    //! v4's suite cannot pin `TZ` (jest hands the file a copy of `process.env`,
    //! so V8's zone cache never sees the assignment) and therefore computes
    //! every expectation through the same local-time API the resolver uses.
    //! v5 has no such problem — the zone is a parameter — so these run under a
    //! DST-carrying zone (`America/Chicago`, the diagnosed incident's) and the
    //! expectations are absolute. The local-vs-UTC crux is what the tier-1
    //! differential's two zone legs prove against v4's real code.

    use super::*;

    const CHI: &str = "America/Chicago";

    /// 2026-07-28 21:44 America/Chicago (CDT, UTC-5) — the wall-clock moment
    /// from the diagnosed chat. The same instant is already 2026-07-29 in UTC,
    /// which is the whole bug.
    const NOW: f64 = 1_785_293_040_000.0;
    const NOW_ISO: &str = "2026-07-29T02:44:00.000Z";
    /// Local midnight of NOW's local day — v4's live-verified `from` value.
    const CHI_MIDNIGHT_28: &str = "2026-07-28T05:00:00.000Z";

    fn resolve(text: &str) -> DayReferenceResolution {
        resolve_day_reference(text, NOW, CHI).expect("resolved")
    }

    #[test]
    fn resolves_a_late_evening_today_to_the_local_day() {
        let r = resolve("No, no. I mean the mission today. I want to hear about it.");
        assert_eq!(r.matched, "today");
        assert!(r.past_pointing);
        assert_eq!(r.time_range.from, CHI_MIDNIGHT_28);
        assert_eq!(r.time_range.to, NOW_ISO);
        // The UTC-day "today" would have started here and contained none of the
        // mission — the bias this module exists to remove.
        assert_ne!(r.time_range.from, "2026-07-29T00:00:00.000Z");
    }

    #[test]
    fn covers_an_event_earlier_the_same_local_day() {
        let r = resolve("the mission today");
        // 2026-07-28 12:00 CDT — inside; 2026-07-27 23:59 CDT — outside.
        let midday = 1_785_258_000_000f64; // 2026-07-28T17:00:00Z
        let last_night = 1_785_214_740_000f64; // 2026-07-28T04:59:00Z
        let from = crate::episodic::event_time_ms(Some(&r.time_range.from)).unwrap();
        let to = crate::episodic::event_time_ms(Some(&r.time_range.to)).unwrap();
        assert!(from <= midday && to >= midday);
        assert!(from > last_night);
    }

    #[test]
    fn resolves_the_whole_lexicon() {
        let cases: Vec<(&str, &str, &str, &str)> = vec![
            (
                "earlier today we talked",
                "earlier today",
                CHI_MIDNIGHT_28,
                NOW_ISO,
            ),
            (
                "how did it go this morning?",
                "this morning",
                CHI_MIDNIGHT_28,
                NOW_ISO,
            ),
            (
                "you seemed rattled this afternoon",
                "this afternoon",
                CHI_MIDNIGHT_28,
                NOW_ISO,
            ),
            (
                "that thing this evening",
                "this evening",
                CHI_MIDNIGHT_28,
                NOW_ISO,
            ),
            ("what a mess tonight", "tonight", CHI_MIDNIGHT_28, NOW_ISO),
            (
                "about yesterday",
                "yesterday",
                "2026-07-27T05:00:00.000Z",
                CHI_MIDNIGHT_28,
            ),
            (
                "the day before yesterday",
                "day before yesterday",
                "2026-07-26T05:00:00.000Z",
                "2026-07-27T05:00:00.000Z",
            ),
            (
                "we met 3 days ago",
                "3 days ago",
                "2026-07-25T05:00:00.000Z",
                "2026-07-26T05:00:00.000Z",
            ),
            (
                "we met three days ago",
                "three days ago",
                "2026-07-25T05:00:00.000Z",
                "2026-07-26T05:00:00.000Z",
            ),
            (
                "that was 1 day ago",
                "1 day ago",
                "2026-07-27T05:00:00.000Z",
                CHI_MIDNIGHT_28,
            ),
            (
                "fourteen days ago exactly",
                "fourteen days ago",
                "2026-07-14T05:00:00.000Z",
                "2026-07-15T05:00:00.000Z",
            ),
        ];
        for (text, matched, from, to) in cases {
            let r = resolve(text);
            assert_eq!(r.matched, matched, "matched for {text:?}");
            assert!(r.past_pointing, "past_pointing for {text:?}");
            assert_eq!(r.time_range.from, from, "from for {text:?}");
            assert_eq!(r.time_range.to, to, "to for {text:?}");
        }
    }

    #[test]
    fn last_night_spans_18_to_06_local() {
        let r = resolve("you were quiet last night");
        assert_eq!(r.time_range.from, "2026-07-27T23:00:00.000Z");
        assert_eq!(r.time_range.to, "2026-07-28T11:00:00.000Z");
        assert!(r.past_pointing);
    }

    #[test]
    fn weeks_anchor_on_the_local_monday() {
        // NOW is a Tuesday, so this week's anchor is yesterday.
        let this_week = resolve("busy this week");
        assert_eq!(this_week.time_range.from, "2026-07-27T05:00:00.000Z");
        assert_eq!(this_week.time_range.to, NOW_ISO);
        let last_week = resolve("what you said last week");
        assert_eq!(last_week.time_range.from, "2026-07-20T05:00:00.000Z");
        assert_eq!(last_week.time_range.to, "2026-07-27T05:00:00.000Z");
    }

    #[test]
    fn leaves_an_out_of_range_n_days_ago_to_the_llm() {
        assert!(resolve_day_reference("that was 97 days ago", NOW, CHI).is_none());
        assert!(resolve_day_reference("that was 15 days ago", NOW, CHI).is_none());
        // `Number("0")` is falsy in v4's guard.
        assert!(resolve_day_reference("that was 0 days ago", NOW, CHI).is_none());
    }

    #[test]
    fn future_references_are_marked_future_pointing() {
        let tomorrow = resolve("let's do it tomorrow");
        assert_eq!(tomorrow.matched, "tomorrow");
        assert!(!tomorrow.past_pointing);
        assert_eq!(tomorrow.time_range.from, "2026-07-29T05:00:00.000Z");
        assert_eq!(tomorrow.time_range.to, "2026-07-30T05:00:00.000Z");
        let next_week = resolve("we can start next week");
        assert_eq!(next_week.matched, "next week");
        assert!(!next_week.past_pointing);
        assert_eq!(next_week.time_range.from, "2026-08-03T05:00:00.000Z");
        assert_eq!(next_week.time_range.to, "2026-08-10T05:00:00.000Z");
        // A later future reference suppresses an earlier past one.
        let mixed = resolve("we said yesterday that we would start tomorrow");
        assert_eq!(mixed.matched, "tomorrow");
        assert!(!mixed.past_pointing);
    }

    #[test]
    fn precedence_and_safety() {
        // The latest match in the text wins.
        let r = resolve("we covered that yesterday — but today, the mission");
        assert_eq!(r.matched, "today");
        assert_eq!(r.time_range.from, CHI_MIDNIGHT_28);
        // An inner phrase never beats its own container.
        assert_eq!(
            resolve("the day before yesterday").matched,
            "day before yesterday"
        );
        // Case-insensitive, and `matched` is the normalized form.
        assert_eq!(resolve("YESTERDAY was long").matched, "yesterday");
        assert_eq!(resolve("Today is better").matched, "today");
        // Word boundaries.
        assert!(resolve_day_reference("we drove through Yesterdayville", NOW, CHI).is_none());
        assert!(resolve_day_reference("the Todayer gazette", NOW, CHI).is_none());
        // No reference, empty text, invalid clock.
        assert!(resolve_day_reference("how are the soil samples looking", NOW, CHI).is_none());
        assert!(resolve_day_reference("", NOW, CHI).is_none());
        assert!(resolve_day_reference("the mission today", f64::NAN, CHI).is_none());
        // An unresolvable later match does not clobber the earlier winner.
        let kept = resolve("we met yesterday, or was it 97 days ago");
        assert_eq!(kept.matched, "yesterday");
        // Stateless across calls (no leaked regex cursor).
        assert_eq!(resolve("the mission today"), resolve("the mission today"));
    }

    #[test]
    fn windows_spanning_a_dst_transition_carry_both_offsets() {
        // 2026-03-09 12:00 CDT — the Monday after Chicago's spring-forward
        // (2026-03-08 02:00). A single fixed offset could not produce these two
        // ends: `last week` starts in CST (-06:00) and ends in CDT (-05:00).
        const MAR9: f64 = 1_773_075_600_000.0;
        let r = resolve_day_reference("what you said last week", MAR9, CHI).expect("resolved");
        assert_eq!(r.time_range.from, "2026-03-02T06:00:00.000Z");
        assert_eq!(r.time_range.to, "2026-03-09T05:00:00.000Z");
        // `4 days ago` lands wholly inside CST.
        let r = resolve_day_reference("we met 4 days ago", MAR9, CHI).expect("resolved");
        assert_eq!(r.time_range.from, "2026-03-05T06:00:00.000Z");
        assert_eq!(r.time_range.to, "2026-03-06T06:00:00.000Z");
    }

    #[test]
    fn an_unresolvable_zone_falls_back_to_utc() {
        let r = resolve_day_reference("the mission today", NOW, "Mars/Olympus").expect("resolved");
        assert_eq!(r.time_range.from, "2026-07-29T00:00:00.000Z");
        assert_eq!(r.time_range.to, NOW_ISO);
    }
}
