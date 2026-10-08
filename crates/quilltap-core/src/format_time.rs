//! Port of the subset of v4 `lib/format-time.ts` the Post Office needs —
//! `formatDate` / `formatDateTime` — plus the other zone-less `toLocale*`
//! renderings v4 spreads across the web-search tool and the Almanack.
//!
//! v4 formats via `new Date(iso).toLocaleDateString(undefined, {...})` — the
//! **runtime's default locale and system timezone**. Confirmed empirically that
//! Node's default locale is `en-US` and the `monthStyle:'long'` + `hour/minute`
//! options render `"{Month} {D}, {YYYY} at {hh}:{mm} {AM/PM}"` (a 2-digit,
//! 12-hour clock).
//!
//! **The zone is an argument (P4.119, dogfood #121, ruled (a) 2026-09-29).**
//! This module once computed every part in UTC ("the mailbox date is
//! TZ-dependent by v4's design; we do not 'fix' it mid-port") because the
//! differential pins the oracle to `TZ=UTC`, which made "matches v4 under
//! `TZ=UTC`" read as "always UTC" — and on real data the difference cost every
//! re-rendered chat its embeddings and persisted wrong-zone letter times. Each
//! formatter now takes the zone v4's call resolves — production passes the
//! host zone the composition root read once and threaded down (P4.127; see
//! [`crate::host_zone`]), every test and differential
//! passes `TimeZone::UTC` explicitly — and renders each instant with that
//! instant's OWN offset (`Timestamp::to_zoned`), so a January letter rendered
//! in July keeps its winter offset.
//!
//! Uses `clock::iso_to_ms` (JS `Date.parse`) + the civil-date math from
//! `chat_timestamp`; en-US month names inline.

use crate::clock::iso_to_ms;
use jiff::tz::TimeZone;
use jiff::Zoned;

/// en-US full month names (`month: 'long'`).
/// Shared with the Scriptorium renderer's own `formatDateTime`
/// ([`crate::services::conversation_markdown`]), which differs only in its
/// `hour: 'numeric'` rendering.
pub(crate) const MONTHS_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
/// en-US short month names (`month: 'short'`).
const MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Whether to render the full or abbreviated month (v4 `FormatDateOptions.monthStyle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonthStyle {
    Short,
    Long,
}

/// The instant `ms` seen in `zone`, with that instant's own offset. `None`
/// only outside jiff's representable range (±9999 years), which no stored
/// timestamp reaches.
fn zoned(ms: i64, zone: &TimeZone) -> Option<Zoned> {
    Some(
        jiff::Timestamp::from_millisecond(ms)
            .ok()?
            .to_zoned(zone.clone()),
    )
}

/// en-US `M/D/YYYY` (no leading zeros) — `toLocaleDateString()`'s date half.
fn numeric_date(z: &Zoned) -> String {
    format!("{}/{}/{}", z.month(), z.day(), z.year())
}

/// The en-US 12-hour clock: `00`→12 AM, `13`→1 PM, `12`→12 PM.
fn hour12(hour24: i8) -> (i8, &'static str) {
    let meridiem = if hour24 < 12 { "AM" } else { "PM" };
    match hour24 % 12 {
        0 => (12, meridiem),
        h => (h, meridiem),
    }
}

/// v4 `formatDateTime(dateString, { monthStyle })` in `zone` / en-US.
/// Returns `""` for an empty/None input; falls back to the raw string on an
/// unparseable date (v4's `catch` / `Invalid Date` edge is not reproduced — the
/// corpus uses valid ISO timestamps).
pub fn format_date_time(
    date_string: Option<&str>,
    month_style: MonthStyle,
    zone: &TimeZone,
) -> String {
    let Some(s) = date_string.filter(|s| !s.is_empty()) else {
        return String::new();
    };
    // v4 `toLocaleDateString` on `Invalid Date` → "Invalid Date"; not banked.
    let Some(z) = iso_to_ms(s).and_then(|ms| zoned(ms, zone)) else {
        return s.to_string();
    };
    let month_name = match month_style {
        MonthStyle::Long => MONTHS_LONG[(z.month() - 1) as usize],
        MonthStyle::Short => MONTHS_SHORT[(z.month() - 1) as usize],
    };
    // 12-hour, 2-digit hour (en-US `hour: '2-digit'`): 00→12 AM, 13→01 PM.
    let (hour, meridiem) = hour12(z.hour());
    format!(
        "{month_name} {}, {} at {hour:02}:{:02} {meridiem}",
        z.day(),
        z.year(),
        z.minute()
    )
}

/// v4 `new Date(iso).toLocaleDateString()` (no options) in `zone` / en-US:
/// `"M/D/YYYY"` (no leading zeros). Used by the `search_web` built-in result
/// formatter and the Scriptorium render's same-day test. Returns
/// `"Invalid Date"` for an unparseable input (matching V8's non-throwing
/// behavior) — the corpus uses valid dates.
pub fn format_date_short_us(iso: &str, zone: &TimeZone) -> String {
    match iso_to_ms(iso).and_then(|ms| zoned(ms, zone)) {
        Some(z) => numeric_date(&z),
        None => "Invalid Date".to_string(),
    }
}

/// v4 `new Date(iso).toLocaleDateString()` (no options) in `zone` / en-US —
/// `"M/D/YYYY"`, no leading zeros — over the **full `Date.parse` domain**
/// ([`crate::episodic::js_date_parse_ms`]) rather than the strict-ISO
/// [`format_date_short_us`]. `None` when the input is not a date JS would
/// accept, i.e. exactly when v4's `Number.isNaN(date.getTime())` is true.
///
/// The Almanack's `formatDate` is its only caller; it renders `None` as `"N/A"`.
pub fn locale_date_us(iso: &str, zone: &TimeZone) -> Option<String> {
    let ms = almanack_date_parse_ms(iso, zone)?;
    Some(numeric_date(&zoned(ms, zone)?))
}

/// The Almanack formatters' parse domain: the full `Date.parse` ISO subset PLUS
/// V8's legacy space-form fallback for the SQLite `datetime('now')` vintage
/// (`"2026-08-05 09:07:03"`, optionally with fractional seconds) — real
/// instances carry such stamps (e.g. pre-ISO `completedAt` columns), and v4's
/// `new Date(...)` accepts them where the strict ISO subset renders `"N/A"`.
///
/// Deliberately NOT a widening of the shared [`crate::episodic::js_date_parse_ms`]
/// (the P4.37 order's instruction, re-affirmed by P4.119's Mandate 3 — that
/// parser's callers are the deliberately-UTC set): the space form is normalized
/// to the `T` form HERE and only here, and the zone semantics are applied here.
///
/// **A zone-less DATETIME is read in `zone`** (P4.119): V8 parses the space
/// form — like any zone-less datetime — as LOCAL time, and v4 prints it back
/// through the same local zone, so the stored wall-clock digits round-trip
/// unchanged. Parsing UTC and rendering local would SHIFT them by the offset.
/// A date-only form stays UTC (V8 reads `"2026-01-05"` as UTC midnight) and a
/// stamp with `Z` / `±HH:MM` keeps its own offset. A wall time that does not
/// exist in `zone` (the spring-forward gap) resolves forward and an ambiguous
/// one (the fall-back fold) to the earlier instant — jiff's `compatible`
/// disambiguation, which is ECMAScript's LocalTime → UTC rule.
///
/// V8's legacy fallback also tolerates a single-digit hour (`"2026-08-05
/// 9:07:03"`); no writer produces that shape, and it stays unparsed here — a
/// recorded seam, not an arm.
fn almanack_date_parse_ms(s: &str, zone: &TimeZone) -> Option<i64> {
    // `YYYY-MM-DD HH:…` — swap the single separating space for `T`.
    let b = s.as_bytes();
    let space_form = b.len() > 11
        && b[..10].iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
        && b[10] == b' '
        && b[11].is_ascii_digit();
    let t_form;
    let s = if space_form {
        t_form = format!("{}T{}", &s[..10], &s[11..]);
        t_form.as_str()
    } else {
        s
    };
    let ms = crate::episodic::js_date_parse_ms(s)?;
    if !is_zoneless_datetime(s) {
        return Some(ms);
    }
    // `js_date_parse_ms` read the wall digits as UTC; re-read them in `zone`.
    wall_ms_in_zone(ms, zone)
}

/// Re-read `wall_ms` — wall-clock digits encoded as if they were UTC epoch ms
/// — as a LOCAL time in `zone`, answering the real instant: V8's zone-less
/// `new Date(s)` / `new Date(y, m, d, h, mi, s)`. A wall time in the
/// spring-forward gap resolves forward and one in the fall-back fold to the
/// earlier instant (jiff's `compatible`, ECMAScript's LocalTime → UTC rule).
/// Under `TimeZone::UTC` this is the identity.
pub(crate) fn wall_ms_in_zone(wall_ms: i64, zone: &TimeZone) -> Option<i64> {
    let wall = jiff::Timestamp::from_millisecond(wall_ms)
        .ok()?
        .to_zoned(TimeZone::UTC)
        .datetime();
    Some(
        wall.to_zoned(zone.clone())
            .ok()?
            .timestamp()
            .as_millisecond(),
    )
}

/// Whether an input `js_date_parse_ms` accepted carries a time but no zone
/// designator — the shape V8 reads as LOCAL. The time part is digits, `:` and
/// `.` only, so any `Z` / `+` / `-` after the `T` is the designator.
fn is_zoneless_datetime(s: &str) -> bool {
    match s.find(['T', 't']) {
        Some(t) => !s[t + 1..].contains(['Z', 'z', '+', '-']),
        None => false,
    }
}

/// v4 `new Date(iso).toLocaleString()` (no options) in `zone` / en-US —
/// `"M/D/YYYY, h:mm:ss AM/PM"`. The date half has no leading zeros and neither
/// does the 12-hour hour; minutes and seconds are 2-digit. Probed live against
/// Node 24 (`12:00:00 PM`, `9:07:03 AM`, `12:00:00 AM` at midnight).
///
/// `None` on an unparseable input (v4's `NaN` guard → `"N/A"`).
pub fn locale_date_time_us(iso: &str, zone: &TimeZone) -> Option<String> {
    let z = zoned(almanack_date_parse_ms(iso, zone)?, zone)?;
    let (hour, meridiem) = hour12(z.hour());
    Some(format!(
        "{}, {hour}:{:02}:{:02} {meridiem}",
        numeric_date(&z),
        z.minute(),
        z.second()
    ))
}

/// v4 `formatRelativeDays(ts, nowMs = Date.now())` (`lib/format-time.ts:
/// 184-196`, `3ee3b1342`) — the day-resolution relative age ("today",
/// "yesterday", "3 days ago", "last week", "2 weeks ago", "last month",
/// "4 months ago", "2 years ago") for an epoch-millisecond timestamp.
///
/// v4's why, carried: this is the ladder for things that happened days or
/// months ago — a memory's age in recall, a garment last worn. The memory
/// label ([`crate::memory_weighting::format_relative_age`]) delegates here so
/// the two readings cannot drift. `now_ms` is the caller's clock (v4's
/// injectable `nowMs`; production passes [`crate::clock::now_unix_ms`]).
///
/// `days_old` is clamped at 0; the branch boundaries and `Math.floor`
/// semantics are v4's exactly (JS `Math.floor` on a non-negative f64 is
/// Rust's `.floor()`); the year branch pluralizes only when the floored
/// count exceeds 1.
pub fn format_relative_days(ts_ms: f64, now_ms: f64) -> String {
    let days_old = ((now_ms - ts_ms) / 86_400_000.0).max(0.0);

    if days_old < 1.0 {
        "today".to_string()
    } else if days_old < 2.0 {
        "yesterday".to_string()
    } else if days_old < 7.0 {
        format!("{} days ago", days_old.floor() as i64)
    } else if days_old < 14.0 {
        "last week".to_string()
    } else if days_old < 30.0 {
        format!("{} weeks ago", (days_old / 7.0).floor() as i64)
    } else if days_old < 60.0 {
        "last month".to_string()
    } else if days_old < 365.0 {
        format!("{} months ago", (days_old / 30.0).floor() as i64)
    } else {
        let years = (days_old / 365.0).floor() as i64;
        format!("{} year{} ago", years, if years > 1 { "s" } else { "" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UTC: TimeZone = TimeZone::UTC;

    /// The eight rungs of v4's `formatRelativeDays` at their boundaries
    /// (the differential over v4's real function is
    /// `tool_image_generation_equivalence`'s `formatRelativeDays` rows; the
    /// memory label's delegation is pinned by Phase-1 `memory_weighting`).
    #[test]
    fn format_relative_days_walks_v4s_ladder() {
        const DAY: f64 = 86_400_000.0;
        let now = 1_800_000_000_000.0;
        let at = |days: f64| format_relative_days(now - days * DAY, now);
        assert_eq!(at(-3.0), "today"); // a future stamp clamps to 0
        assert_eq!(at(0.999), "today");
        assert_eq!(at(1.0), "yesterday");
        assert_eq!(at(2.0), "2 days ago");
        assert_eq!(at(6.99), "6 days ago");
        assert_eq!(at(7.0), "last week");
        assert_eq!(at(14.0), "2 weeks ago");
        assert_eq!(at(29.9), "4 weeks ago");
        assert_eq!(at(30.0), "last month");
        assert_eq!(at(60.0), "2 months ago");
        assert_eq!(at(364.9), "12 months ago");
        assert_eq!(at(365.0), "1 year ago");
        assert_eq!(at(730.0), "2 years ago");
    }

    fn chicago() -> TimeZone {
        TimeZone::get("America/Chicago").unwrap()
    }

    /// Vectors captured from Node 24 under `TZ=UTC` (the oracle's pin).
    #[test]
    fn locale_formats_match_node() {
        let vectors = [
            (
                "2026-08-05T12:00:00.000Z",
                "8/5/2026",
                "8/5/2026, 12:00:00 PM",
            ),
            (
                "2026-01-05T09:07:03.000Z",
                "1/5/2026",
                "1/5/2026, 9:07:03 AM",
            ),
            (
                "2026-12-31T23:59:59.999Z",
                "12/31/2026",
                "12/31/2026, 11:59:59 PM",
            ),
            (
                "2026-06-01T00:00:00.000Z",
                "6/1/2026",
                "6/1/2026, 12:00:00 AM",
            ),
            (
                "2020-02-29T13:05:00.000Z",
                "2/29/2020",
                "2/29/2020, 1:05:00 PM",
            ),
        ];
        for (iso, date, date_time) in vectors {
            assert_eq!(
                locale_date_us(iso, &UTC).as_deref(),
                Some(date),
                "date {iso}"
            );
            assert_eq!(
                locale_date_time_us(iso, &UTC).as_deref(),
                Some(date_time),
                "datetime {iso}"
            );
        }
        // Date-only forms parse in JS (and here) — unlike `format_date_short_us`.
        assert_eq!(
            locale_date_us("2026-01-05", &UTC).as_deref(),
            Some("1/5/2026")
        );
        // The V8 legacy space form (the SQLite `datetime('now')` vintage) —
        // probed on Node 24 under TZ=UTC: `new Date('2026-08-05 09:07:03')` →
        // `8/5/2026, 9:07:03 AM`; fractional seconds accepted too.
        assert_eq!(
            locale_date_time_us("2026-08-05 09:07:03", &UTC).as_deref(),
            Some("8/5/2026, 9:07:03 AM")
        );
        assert_eq!(
            locale_date_time_us("2026-08-05 09:07:03.123", &UTC).as_deref(),
            Some("8/5/2026, 9:07:03 AM")
        );
        assert_eq!(
            locale_date_us("2026-08-05 09:07:03", &UTC).as_deref(),
            Some("8/5/2026")
        );
        // The single-digit-hour legacy shape stays unparsed — the recorded seam.
        assert_eq!(locale_date_time_us("2026-08-05 9:07:03", &UTC), None);
        // What JS rejects, we reject — the renderer's "N/A" arm.
        assert_eq!(locale_date_us("not-a-date", &UTC), None);
        assert_eq!(locale_date_time_us("", &UTC), None);
    }

    #[test]
    fn format_date_short_us_matches_v4() {
        // Full-ISO inputs (the `iso_to_ms` strict form the corpus uses). A
        // date-only `publishedDate` (`"2026-01-05"`) is a documented seam — JS
        // `Date.parse` accepts it, the strict `iso_to_ms` does not.
        assert_eq!(
            format_date_short_us("2026-06-15T00:00:00.000Z", &UTC),
            "6/15/2026"
        );
        assert_eq!(
            format_date_short_us("2026-01-05T00:00:00.000Z", &UTC),
            "1/5/2026"
        );
        assert_eq!(
            format_date_short_us("2020-12-31T23:00:00.000Z", &UTC),
            "12/31/2020"
        );
    }

    #[test]
    fn format_date_time_long_utc() {
        assert_eq!(
            format_date_time(Some("2026-02-01T09:05:00.000Z"), MonthStyle::Long, &UTC),
            "February 1, 2026 at 09:05 AM"
        );
        assert_eq!(
            format_date_time(Some("2026-11-20T23:45:00.000Z"), MonthStyle::Long, &UTC),
            "November 20, 2026 at 11:45 PM"
        );
        assert_eq!(
            format_date_time(Some("2026-02-01T00:00:00.000Z"), MonthStyle::Long, &UTC),
            "February 1, 2026 at 12:00 AM"
        );
        // Noon → 12 PM.
        assert_eq!(
            format_date_time(Some("2026-02-01T12:00:00.000Z"), MonthStyle::Long, &UTC),
            "February 1, 2026 at 12:00 PM"
        );
        assert_eq!(format_date_time(None, MonthStyle::Long, &UTC), "");
        assert_eq!(format_date_time(Some(""), MonthStyle::Long, &UTC), "");
    }

    /// P4.119 red-first: under `America/Chicago` a winter and a summer instant
    /// rendered by ONE process carry their OWN offsets (CST −6, CDT −5) — the
    /// proof the formatters are per-instant, not one offset taken at `now`.
    /// Vectors: Node 24 under `TZ=America/Chicago`.
    #[test]
    fn formatters_render_each_instant_in_its_own_offset() {
        let zone = chicago();
        // The walk's D2 bytes: a 19:40Z summer letter reads 02:40 PM (CDT).
        assert_eq!(
            format_date_time(Some("2026-09-29T19:40:00.000Z"), MonthStyle::Long, &zone),
            "September 29, 2026 at 02:40 PM"
        );
        // The same wall-clock UTC hour in January reads 01:40 PM (CST).
        assert_eq!(
            format_date_time(Some("2026-01-29T19:40:00.000Z"), MonthStyle::Long, &zone),
            "January 29, 2026 at 01:40 PM"
        );
        assert_eq!(
            locale_date_time_us("2026-07-04T03:30:00.000Z", &zone).as_deref(),
            Some("7/3/2026, 10:30:00 PM")
        );
        assert_eq!(
            locale_date_time_us("2026-12-04T03:30:00.000Z", &zone).as_deref(),
            Some("12/3/2026, 9:30:00 PM")
        );
        // The date flips at LOCAL midnight, not UTC midnight.
        assert_eq!(
            format_date_short_us("2026-06-15T04:00:00.000Z", &zone),
            "6/14/2026"
        );
        assert_eq!(
            format_date_short_us("2026-06-15T05:00:00.000Z", &zone),
            "6/15/2026"
        );
        assert_eq!(
            locale_date_us("2026-01-01T05:59:59.000Z", &zone).as_deref(),
            Some("12/31/2025")
        );
        // A date-only form is UTC midnight in V8, so it renders the day before.
        assert_eq!(
            locale_date_us("2026-01-05", &zone).as_deref(),
            Some("1/4/2026")
        );
    }

    /// Mandate 3: a zone-less stamp (the SQLite `datetime('now')` space form,
    /// or a zone-less `T` form) is parsed in the SAME zone it is rendered in,
    /// so its wall-clock digits print back unchanged — as V8's `new Date(s)`
    /// (local) + `toLocaleString()` (local) do. A stamp with `Z` keeps its own
    /// offset.
    #[test]
    fn zoneless_stamps_round_trip_their_wall_digits() {
        let zone = chicago();
        for stamp in [
            "2026-08-05 09:07:03",
            "2026-08-05 09:07:03.123",
            "2026-08-05T09:07:03",
            "2026-08-05t09:07:03",
        ] {
            assert_eq!(
                locale_date_time_us(stamp, &zone).as_deref(),
                Some("8/5/2026, 9:07:03 AM"),
                "{stamp}"
            );
        }
        assert_eq!(
            locale_date_time_us("2026-01-05 23:30:00", &zone).as_deref(),
            Some("1/5/2026, 11:30:00 PM")
        );
        assert_eq!(
            locale_date_us("2026-01-05 23:30:00", &zone).as_deref(),
            Some("1/5/2026")
        );
        // Zoned stamps are NOT re-read as local.
        assert_eq!(
            locale_date_time_us("2026-08-05T09:07:03Z", &zone).as_deref(),
            Some("8/5/2026, 4:07:03 AM")
        );
        assert_eq!(
            locale_date_time_us("2026-08-05T09:07:03-05:00", &zone).as_deref(),
            Some("8/5/2026, 9:07:03 AM")
        );
        // The spring-forward gap (02:30 does not exist on 2026-03-08 in
        // Chicago) resolves forward, as ECMAScript's LocalTime → UTC does.
        assert_eq!(
            locale_date_time_us("2026-03-08 02:30:00", &zone).as_deref(),
            Some("3/8/2026, 3:30:00 AM")
        );
        // The fall-back fold (01:30 happens twice on 2026-11-01) takes the
        // earlier instant — printed back as the same wall digits.
        assert_eq!(
            locale_date_time_us("2026-11-01 01:30:00", &zone).as_deref(),
            Some("11/1/2026, 1:30:00 AM")
        );
    }
}
