//! Port of v4 `lib/memory/time/calendar.ts` (`c2795733f`'s
//! `memory-calendar.ts`, renamed by `e0ac7493a`) — the memory calendar, ONE
//! timezone per instance for event time (bug 212).
//!
//! v4's why, carried: the Commonplace Book answers one question in two
//! directions — on write, "when did this happen?" (`resolve_when_phrase`
//! turns "last night" into an `occurredAt`); on read, "which memories fall in
//! the period this turn asks about?" (`resolve_day_reference` turns "last
//! night" into a window, and the keyword distillation's date-only `timeRange`
//! is widened to whole days). Each direction used to keep its own calendar —
//! UTC midnights on write, the server-local clock on read, UTC days for the
//! classifier — so a memory written as "last night" missed the window for
//! "last night". Locked decision 10 of the memory system refactor: one
//! calendar per instance, resolved as the instance timezone (the Salon-level
//! `ChatSettings.timezone`) → `QUILLTAP_TIMEZONE` → the system zone. The
//! per-chat timestamp timezone stays display-only and is never consulted
//! here.
//!
//! v5 shape: this module is pure; the zone is always an ARGUMENT. v4's env
//! and system rungs collapse into the host's calendar NAME (`server_tz`,
//! already `QUILLTAP_TIMEZONE` / `TZ`-reconciled by `quilltap-web`), passed
//! as `host_fallback` (work order P4.D289 LR-3); the instance read and v4's
//! five-minute cache live in [`crate::services::memory_time_zone`].
//!
//! The arithmetic is v4's, literally (LR-7): wall-clock fields read through
//! the zone, a SECOND-TRUNCATED offset, and `zonedTimeToUtcMs`'s two-pass
//! guess — never jiff's `Disambiguation` — with `Date.UTC`'s overflow
//! normalisation and two-digit-year window from the ONE twin
//! ([`crate::chat_timestamp::date_utc_ms`]).

use crate::recall_tags::TimeWindow;
use jiff::tz::{Offset, TimeZone};

/// v4 `DAY_MS` (exported as `MEMORY_CALENDAR_DAY_MS`).
pub const MEMORY_CALENDAR_DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The ONE zone resolver behind every memory-calendar read (LR-6).
///
/// v4's validity is "`new Intl.DateTimeFormat('en-US', { timeZone })` does
/// not throw". Measured on the pin's Node (24.13.1, ICU 78.2, tz 2025c):
/// `Intl` accepts IANA names and backward links case-insensitively
/// (`US/Central`, `us/central`, `EST5EDT`, `Asia/Calcutta`), and the
/// ECMA-402 offset zones `±HH`, `±HHMM`, `±HH:MM` up to `±23:59` — the sign
/// may be U+2212 — while refusing `Z`, `+24:00`, `+05:30:00` and `Factory`.
/// The offset forms become fixed zones here, so the calendar math follows v4;
/// every name goes to jiff's tz database. A string the two runtimes still
/// judge differently is the ruled divergence `MEMORY_ZONE_VALIDITY`.
pub fn resolve_zone(tz: &str) -> Option<TimeZone> {
    if let Some(offset) = parse_offset_zone(tz) {
        return Some(TimeZone::fixed(offset));
    }
    TimeZone::get(tz).ok()
}

/// ECMA-402's `UTCOffset` time-zone identifier: `±HH`, `±HHMM` or `±HH:MM`,
/// hours `00..=23`, minutes `00..=59`, sign `+`, `-` or U+2212.
fn parse_offset_zone(tz: &str) -> Option<Offset> {
    let (negative, rest) = if let Some(r) = tz.strip_prefix('+') {
        (false, r)
    } else if let Some(r) = tz.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = tz.strip_prefix('\u{2212}') {
        (true, r)
    } else {
        return None;
    };
    let b = rest.as_bytes();
    let two = |s: &[u8]| -> Option<i32> {
        if s.len() == 2 && s.iter().all(u8::is_ascii_digit) {
            Some(((s[0] - b'0') * 10 + (s[1] - b'0')) as i32)
        } else {
            None
        }
    };
    let (hours, minutes) = match b.len() {
        2 => (two(b)?, 0),
        4 => (two(&b[..2])?, two(&b[2..])?),
        5 if b[2] == b':' => (two(&b[..2])?, two(&b[3..])?),
        _ => return None,
    };
    if hours > 23 || minutes > 59 {
        return None;
    }
    let seconds = (hours * 3600 + minutes * 60) * if negative { -1 } else { 1 };
    Offset::from_seconds(seconds).ok()
}

/// True when `tz` is a zone v4's `Intl` would accept (v4 `isValidTimeZone`:
/// `!tz` → false, then the formatter must not throw).
pub fn is_valid_time_zone(tz: Option<&str>) -> bool {
    match tz {
        Some(t) if !t.is_empty() => resolve_zone(t).is_some(),
        _ => false,
    }
}

/// The memory timezone (v4 `getMemoryTimeZone`, with v4's cached-instance
/// rung supplied by the caller): a valid `instance_tz` → a valid
/// `host_fallback` → `"UTC"` (v4 `systemTimeZone()`'s last resort, as a
/// NAME). The winning string is returned as given, as v4 returns it.
pub fn memory_time_zone(instance_tz: Option<&str>, host_fallback: &str) -> String {
    if let Some(tz) = instance_tz.filter(|t| is_valid_time_zone(Some(t))) {
        return tz.to_string();
    }
    if is_valid_time_zone(Some(host_fallback)) {
        return host_fallback.to_string();
    }
    "UTC".to_string()
}

/// v4 `ZonedParts` — wall-clock fields of an instant in a zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZonedParts {
    pub year: i64,
    /// 0-based, like `Date#getMonth`.
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    /// 0 = Sunday … 6 = Saturday, like `Date#getDay`.
    pub weekday: i64,
}

/// JS `new Date(ms)`'s TimeClip: finite and within ±8.64e15, truncated
/// toward zero; `None` is an Invalid Date.
fn time_clip(ms: f64) -> Option<i64> {
    if !ms.is_finite() || ms.abs() > 8.64e15 {
        return None;
    }
    Some(ms.trunc() as i64)
}

/// Wall-clock fields of the instant `ms` in `tz` (v4 `zonedParts`, via the
/// `en-US` `h23` formatter: `hour % 24`, weekday 0 = Sunday).
///
/// The zone contributes only its UTC offset at the instant (jiff, clamped
/// into jiff's −9999..=9999 range — `Intl` formats far beyond it, and a
/// `zonedDayBoundsIso("9999-12-31")` reads year 10000); the local instant is
/// then split with the proleptic civil-day arithmetic `Intl` uses.
///
/// Every ported caller guards exactly where v4's does (a finite instant, a
/// resolved zone). Outside that contract v4's `formatToParts` throws a
/// `RangeError`; here a non-finite instant reads as the epoch and an unknown
/// zone as UTC, so a misuse degrades instead of panicking.
pub fn zoned_parts(ms: f64, tz: &str) -> ZonedParts {
    let zone = resolve_zone(tz).unwrap_or(TimeZone::UTC);
    let m = time_clip(ms).unwrap_or(0);
    // jiff's millisecond domain (its `Timestamp::MAX` carries sub-ms digits
    // `from_millisecond` refuses, so the bounds are the whole-ms ones).
    let clamped = m.clamp(-377_705_023_201_000, 253_402_207_200_000);
    let offset_s = jiff::Timestamp::from_millisecond(clamped)
        .map(|ts| zone.to_offset(ts).seconds() as i64)
        .unwrap_or(0);
    let local = m + offset_s * 1000;
    let days = local.div_euclid(MEMORY_CALENDAR_DAY_MS);
    let rem = local.rem_euclid(MEMORY_CALENDAR_DAY_MS);
    let (year, month, day) = crate::clock::civil_from_days(days);
    // Intl's `year: 'numeric'` renders the ERA year with no sign: 1 BCE
    // (proleptic year 0) prints `1`, year −50 prints `51`. v4 reads that
    // number back through `Date.UTC`, so a wall far enough out of range for
    // the two-digit window to fling the guess into BCE follows it there.
    let year = if year <= 0 { 1 - year } else { year };
    ZonedParts {
        year,
        month: month as i64 - 1,
        day: day as i64,
        hour: (rem / 3_600_000) % 24,
        minute: (rem / 60_000) % 60,
        second: (rem / 1000) % 60,
        weekday: (days + 4).rem_euclid(7),
    }
}

/// Offset of `tz` from UTC at the instant `ms`, in ms (UTC−5 → −5h) — v4
/// `offsetMs`: `Date.UTC(parts…) − floor(ms / 1000) × 1000`, SECOND-truncated.
fn offset_ms(ms: f64, tz: &str) -> f64 {
    let p = zoned_parts(ms, tz);
    let as_utc =
        crate::chat_timestamp::date_utc_ms(p.year, p.month, p.day, p.hour, p.minute, p.second);
    as_utc as f64 - (ms / 1000.0).floor() * 1000.0
}

/// The UTC instant of a wall-clock time in `tz` (v4 `zonedTimeToUtcMs`).
/// Day/month overflow is normalised the way `Date.UTC` does it (`day: 0` is
/// the previous month's last day), so callers can do calendar arithmetic in
/// the arguments.
///
/// v4's two-pass guess, literally: `guess = Date.UTC(y, m, d, h)`; `first =
/// offset(guess)`; `candidate = guess − first`; `second = offset(candidate)`;
/// `second === first ? candidate : guess − second`. In a DST gap or overlap
/// this picks what v4 picks — no jiff disambiguation anywhere.
pub fn zoned_time_to_utc_ms(year: i64, month: i64, day: i64, hour: i64, tz: &str) -> f64 {
    let guess = crate::chat_timestamp::date_utc_ms(year, month, day, hour, 0, 0) as f64;
    let first = offset_ms(guess, tz);
    let candidate = guess - first;
    let second = offset_ms(candidate, tz);
    if second == first {
        candidate
    } else {
        guess - second
    }
}

/// Local midnight of the day `offset_days` from the local day containing `ms`.
pub fn start_of_zoned_day_ms(ms: f64, tz: &str, offset_days: i64) -> f64 {
    let p = zoned_parts(ms, tz);
    zoned_time_to_utc_ms(p.year, p.month, p.day + offset_days, 0, tz)
}

/// Local wall-clock `hour` on the day `offset_days` from the local day
/// containing `ms`.
pub fn zoned_time_on_day_ms(ms: f64, tz: &str, offset_days: i64, hour: i64) -> f64 {
    let p = zoned_parts(ms, tz);
    zoned_time_to_utc_ms(p.year, p.month, p.day + offset_days, hour, tz)
}

/// Local midnight of the Monday starting the week `week_offset` weeks from
/// the one containing `ms`.
pub fn start_of_zoned_week_ms(ms: f64, tz: &str, week_offset: i64) -> f64 {
    let p = zoned_parts(ms, tz);
    let days_since_monday = (p.weekday + 6) % 7;
    zoned_time_to_utc_ms(
        p.year,
        p.month,
        p.day - days_since_monday + week_offset * 7,
        0,
        tz,
    )
}

/// `YYYY-MM-DD` of the local day containing `ms` (v4 `zonedDateStamp`: the
/// year UNPADDED — `String(year)` — the month and day `padStart(2, '0')`).
pub fn zoned_date_stamp(ms: f64, tz: &str) -> String {
    let p = zoned_parts(ms, tz);
    format!("{}-{:02}-{:02}", p.year, p.month + 1, p.day)
}

/// JS `new Date(ms).toISOString()` for an in-range instant, including the
/// expanded `±YYYYYY` year outside `0000..=9999`.
pub(crate) fn js_iso_string(ms: f64) -> Option<String> {
    let m = time_clip(ms)?;
    let days = m.div_euclid(MEMORY_CALENDAR_DAY_MS);
    let rem = m.rem_euclid(MEMORY_CALENDAR_DAY_MS);
    let (y, mo, d) = crate::clock::civil_from_days(days);
    let year = if (0..=9999).contains(&y) {
        format!("{y:04}")
    } else if y < 0 {
        format!("-{:06}", -y)
    } else {
        format!("+{y:06}")
    };
    Some(format!(
        "{year}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        (rem / 60_000) % 60,
        (rem / 1000) % 60,
        rem % 1000
    ))
}

/// Widen a date-only `YYYY-MM-DD` bound to the local day it names: `from` →
/// local midnight, `to` → the last millisecond before the next local midnight
/// (v4 `zonedDayBoundsIso`). `None` for anything that is not a valid calendar
/// date — checked by v4's `Date.UTC` round trip, whose two-digit-year window
/// refuses `0099-01-01` exactly as v4 does.
pub fn zoned_day_bounds_iso(date: &str, tz: &str) -> Option<TimeWindow> {
    let b = date.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || ![0, 1, 2, 3, 5, 6, 8, 9]
            .iter()
            .all(|&i| b[i].is_ascii_digit())
    {
        return None;
    }
    let year: i64 = date[0..4].parse().ok()?;
    let month: i64 = date[5..7].parse::<i64>().ok()? - 1;
    let day: i64 = date[8..10].parse().ok()?;
    let check = crate::chat_timestamp::date_utc_ms(year, month, day, 0, 0, 0);
    let (cy, cm, cd) = crate::clock::civil_from_days(check.div_euclid(MEMORY_CALENDAR_DAY_MS));
    if cy != year || cm as i64 - 1 != month || cd as i64 != day {
        return None;
    }
    let from = zoned_time_to_utc_ms(year, month, day, 0, tz);
    let next = zoned_time_to_utc_ms(year, month, day + 1, 0, tz);
    Some(TimeWindow {
        from: js_iso_string(from)?,
        to: js_iso_string(next - 1.0)?,
    })
}
