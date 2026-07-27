//! Chat timestamp utilities — v4 `lib/chat/timestamp-utils.ts` ported.
//!
//! Calculates and formats the "current time" string injected into system
//! prompts, with IANA-timezone-aware output and optional fictional (auto-
//! incrementing) time. Mirrors v4's six exported functions:
//! [`resolve_timezone`], [`parse_timestamp_in_timezone`],
//! [`calculate_current_timestamp`], [`should_inject_timestamp`],
//! [`format_timestamp_for_system_prompt`], and
//! [`ensure_fictional_base_real_time`].
//!
//! # Timezone mechanics (the CLDR/TZDB seam)
//!
//! v4 formats via `Intl.DateTimeFormat` with an IANA `timeZone`. Its whole
//! observable surface is (a) the wall-clock date-parts of the instant *in that
//! zone* and (b) fixed `en-US` literals (month/day names, `AM`/`PM`, the
//! `" at "` FRIENDLY separator, the `+HH:MM` offset). The literals are hardcoded
//! here; the ONLY thing that needs a timezone database is the DST-correct UTC
//! offset for an IANA zone at a precise instant.
//!
//! ICU4X's `icu_time` offset API is name-timestamp-granular (it returns both a
//! zone's standard *and* daylight offsets, not the one in effect at an instant),
//! so it can't resolve the exact offset across a DST transition. `jiff` bundles
//! its own TZDB and *is* instant-exact — its `to_offset` matches `Intl`
//! byte-for-byte on every corpus instant, including both US DST boundaries, and
//! errors on an unknown zone exactly as `Intl.DateTimeFormat` throws. So `jiff`
//! is used for the offset lookup only; all string shaping stays hand-rolled.
//!
//! # Injected clock / local offset (determinism)
//!
//! v4 reads `Date.now()` internally. Mirroring [`crate::clock`], the impure
//! `now_ms` is injected as a parameter so the differential can pin it. The
//! `timezone === undefined` path in v4 reads the *host* system TZ (via
//! `Intl` with no `timeZone`, and `date.getTimezoneOffset()` for the ISO
//! offset) — inherently host-coupled. That host offset is likewise injected as
//! `local_offset_minutes` (JS `getTimezoneOffset()` convention: positive = west
//! of UTC), so no formatting path silently reads an ambient clock.

use serde_json::Value;

use crate::clock::iso_from_unix_ms;

/// The result of [`calculate_current_timestamp`] — v4's `CalculatedTimestamp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalculatedTimestamp {
    /// Formatted timestamp string for display / prompt injection.
    pub formatted: String,
    /// ISO-8601 timestamp value (with timezone offset when a zone is specified).
    pub iso_value: String,
    /// Whether this is a fictional timestamp.
    pub is_fictional: bool,
}

/// The timestamp-format enum — v4 `TimestampFormat`
/// (`z.enum(['ISO8601','FRIENDLY','DATE_ONLY','TIME_ONLY','CUSTOM'])`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampFormat {
    Iso8601,
    Friendly,
    DateOnly,
    TimeOnly,
    Custom,
}

/// The inject-mode enum — v4 `TimestampMode`
/// (`z.enum(['NONE','START_ONLY','EVERY_MESSAGE','EVERY_N_MINUTES'])`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampMode {
    None,
    StartOnly,
    EveryMessage,
    EveryNMinutes,
}

/// The subset of v4's `TimestampConfig` these functions consume. Zod defaults
/// (`mode`→NONE, `format`→FRIENDLY, `useFictionalTime`→false, `autoPrepend`→
/// true, `intervalMinutes`→15) are materialized by the caller's parse; this
/// struct carries the already-resolved values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampConfig {
    pub mode: TimestampMode,
    pub format: TimestampFormat,
    /// Custom format string (used when `format == Custom`).
    pub custom_format: Option<String>,
    pub use_fictional_time: bool,
    /// Base fictional timestamp (ISO-8601), when fictional time is active.
    pub fictional_base_timestamp: Option<String>,
    /// Real timestamp when the fictional base was set (ISO-8601).
    pub fictional_base_real_time: Option<String>,
    pub auto_prepend: bool,
    /// IANA timezone name override for this chat.
    pub timezone: Option<String>,
    /// Minimum minutes between announcements for `EVERY_N_MINUTES` (default 15).
    pub interval_minutes: i64,
}

/// Date components read as they appear on a clock in a target timezone — v4's
/// `DatePartsInTimezone`. `month` is 0-indexed, `day_of_week` is 0 = Sunday.
struct DatePartsInTimezone {
    year: i64,
    month: i64, // 0-indexed (January = 0)
    day: i64,
    day_of_week: i64, // 0 = Sunday
    hours: i64,       // 0-23
    minutes: i64,
    seconds: i64,
}

const MONTHS: [&str; 12] = [
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
const MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const DAY_NAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const DAYS_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Civil (year, month 1-12, day 1-31) + weekday from days since the Unix epoch.
/// Howard Hinnant's `civil_from_days`, matching [`crate::clock`]'s inverse. The
/// weekday is computed the branch-free way: 1970-01-01 (day 0) was a Thursday,
/// so `weekday = (days + 4).rem_euclid(7)` with 0 = Sunday, matching JS
/// `getUTCDay()` on the same instant.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

/// Split a wall-clock instant (given as milliseconds and the offset already
/// applied, i.e. "local" ms) into its calendar/clock parts. `local_ms` is the
/// UTC ms shifted by the zone offset, so its civil decomposition reads as the
/// clock in that zone. Seconds precision (v4's Intl uses `second: '2-digit'`).
fn parts_from_local_ms(local_ms: i64) -> DatePartsInTimezone {
    let days = local_ms.div_euclid(86_400_000);
    let rem = local_ms.rem_euclid(86_400_000);
    let (y, mo, d) = civil_from_days(days);
    let day_of_week = (days + 4).rem_euclid(7);
    DatePartsInTimezone {
        year: y,
        month: mo - 1, // 0-indexed to match v4
        day: d,
        day_of_week,
        hours: rem / 3_600_000,
        minutes: (rem / 60_000) % 60,
        seconds: (rem / 1000) % 60,
    }
}

/// An error resolving a timezone offset — the analogue of `Intl.DateTimeFormat`
/// throwing `RangeError: Invalid time zone specified`. v4's
/// `calculateCurrentTimestamp` does not catch this; a caller that passes a bad
/// IANA name would see the throw propagate. We surface it as a `Result` at the
/// boundary rather than panicking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidTimezone(pub String);

impl std::fmt::Display for InvalidTimezone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid time zone specified: {}", self.0)
    }
}

impl std::error::Error for InvalidTimezone {}

/// The UTC offset (seconds, east-positive) in effect for an IANA `timezone` at
/// the instant `utc_ms`, DST-correct — the sole TZDB call. `jiff`'s `to_offset`
/// matches `Intl`/`Date` byte-for-byte across DST transitions (probed).
fn zone_offset_seconds(utc_ms: i64, timezone: &str) -> Result<i64, InvalidTimezone> {
    let ts = jiff::Timestamp::from_millisecond(utc_ms)
        .map_err(|_| InvalidTimezone(timezone.to_string()))?;
    let tz =
        jiff::tz::TimeZone::get(timezone).map_err(|_| InvalidTimezone(timezone.to_string()))?;
    Ok(tz.to_offset(ts).seconds() as i64)
}

/// v4 `getDatePartsInTimezone`: read the instant's clock parts in a zone.
/// `None` timezone uses the injected host `local_offset_minutes` (JS
/// `getTimezoneOffset()` convention: positive = west of UTC, so we subtract it).
fn get_date_parts_in_timezone(
    utc_ms: i64,
    timezone: Option<&str>,
    local_offset_minutes: i64,
) -> Result<DatePartsInTimezone, InvalidTimezone> {
    let offset_ms = match timezone {
        Some(tz) => zone_offset_seconds(utc_ms, tz)? * 1000,
        None => -local_offset_minutes * 60_000,
    };
    Ok(parts_from_local_ms(utc_ms + offset_ms))
}

/// JS `Date.UTC(year, month0, day, hours, minutes, seconds)` → Unix ms.
///
/// Reproduces the two behaviours the fixed-width `NAIVE_TIMESTAMP_PATTERN` can
/// still reach: a two-digit year is read as 19xx (`Date.UTC(50, …)` is 1950),
/// and every other field simply *rolls over* rather than erroring — a month of
/// `13` lands in the next year, a day of `32` in the next month. Both fall out
/// of arithmetic, so nothing here validates.
fn date_utc_ms(year: i64, month0: i64, day: i64, hours: i64, minutes: i64, seconds: i64) -> i64 {
    // Date.UTC's legacy two-digit-year window.
    let year = if (0..=99).contains(&year) {
        year + 1900
    } else {
        year
    };
    let y = year + month0.div_euclid(12);
    let m = month0.rem_euclid(12) + 1;
    let days = crate::clock::days_from_civil(y, m, 1) + (day - 1);
    (days * 86_400 + hours * 3_600 + minutes * 60 + seconds) * 1000
}

/// v4's `NAIVE_TIMESTAMP_PATTERN`, hand-rolled:
/// `^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2})(?::(\d{2}))?$` — the shape an
/// `<input type="datetime-local">` produces. Returns
/// `(year, month, day, hours, minutes, seconds)` with `month` still 1-based
/// (v4 subtracts one at the `Date.UTC` call). A trailing `Z` or `+05:30` makes
/// the string absolute and fails the match, exactly as the regex does.
fn match_naive_timestamp(value: &str) -> Option<(i64, i64, i64, i64, i64, i64)> {
    let b = value.as_bytes();
    if !value.is_ascii() {
        return None;
    }
    fn digits(b: &[u8], at: usize, n: usize) -> Option<i64> {
        let slice = b.get(at..at + n)?;
        if !slice.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(slice).ok()?.parse().ok()
    }
    let year = digits(b, 0, 4)?;
    if b.get(4) != Some(&b'-') {
        return None;
    }
    let month = digits(b, 5, 2)?;
    if b.get(7) != Some(&b'-') {
        return None;
    }
    let day = digits(b, 8, 2)?;
    if !matches!(b.get(10), Some(b'T') | Some(b' ')) {
        return None;
    }
    let hours = digits(b, 11, 2)?;
    if b.get(13) != Some(&b':') {
        return None;
    }
    let minutes = digits(b, 14, 2)?;
    let seconds = match b.len() {
        16 => 0,
        19 => {
            if b.get(16) != Some(&b':') {
                return None;
            }
            digits(b, 17, 2)?
        }
        _ => return None,
    };
    Some((year, month, day, hours, minutes, seconds))
}

/// Whether an ISO-ish string is a *date-time* carrying no zone designator — the
/// one shape JS `new Date(...)` resolves against the HOST zone rather than UTC.
/// (A date-only form like `"2026-07-25"` is UTC in JS, so it is excluded here.)
fn is_zoneless_datetime(s: &str) -> bool {
    match s.bytes().position(|c| c == b'T' || c == b't') {
        Some(t) => !s
            .bytes()
            .skip(t + 1)
            .any(|c| matches!(c, b'Z' | b'z' | b'+' | b'-')),
        None => false,
    }
}

/// JS `new Date(string).getTime()` over the ISO-8601 subset — the `Date.parse`
/// port in [`crate::episodic`], plus the host-zone shift JS applies to a
/// zone-less date-time (that parser resolves zone-less as UTC). `None` is JS's
/// `NaN` date.
fn js_new_date_ms(s: &str, local_offset_minutes: i64) -> Option<i64> {
    let ms = crate::episodic::js_date_parse_ms(s)?;
    if is_zoneless_datetime(s) {
        // getTimezoneOffset() is positive west of UTC, so local wall-clock →
        // UTC instant adds it back.
        Some(ms + local_offset_minutes * 60_000)
    } else {
        Some(ms)
    }
}

/// Interpret a timestamp string as a wall-clock reading in a target timezone —
/// v4 `parseTimestampInTimezone` (`e3a9654f`). Returns Unix ms.
///
/// `new Date("1550-07-25T10:15")` resolves a zone-less string against the
/// *server's* timezone, which then gets re-rendered in the story's — so a base
/// of 10:15 set for `Europe/Istanbul` surfaced as 6:01 PM on an
/// `America/Chicago` host (the two LMT offsets of 1550 differing by ~7h46m).
/// The fictional base is a clock reading in the story's own timezone, so that is
/// where it must be anchored.
///
/// Strings carrying an explicit zone (`…Z`, `…+05:30`) are already absolute and
/// pass through untouched, as does any input when `timezone` is `None`
/// (system-local parsing).
///
/// Resolution is iterative because the offset depends on the very instant being
/// solved for (DST, and pre-standardisation LMT offsets that carry seconds). It
/// converges in one step for fixed offsets and two across a DST boundary; three
/// is slack. The **bound is three attempts, not a fixpoint** — a pathological
/// zone that never converges lands wherever v4 lands.
///
/// An unparseable fall-through string is JS `NaN`; v5 has no NaN instant, so it
/// becomes `0` (as [`parse_date_ms`] has always done). The differential keeps
/// every base well-formed.
pub fn parse_timestamp_in_timezone(
    value: &str,
    timezone: Option<&str>,
    local_offset_minutes: i64,
) -> Result<i64, InvalidTimezone> {
    // v4 trims only for the match; the fall-through gets the raw `value`.
    let matched = match_naive_timestamp(value.trim());
    let (year, month, day, hours, minutes, seconds) = match (matched, timezone) {
        (Some(m), Some(_)) => m,
        _ => return Ok(js_new_date_ms(value, local_offset_minutes).unwrap_or(0)),
    };

    let target = date_utc_ms(year, month - 1, day, hours, minutes, seconds);

    let mut instant = target;
    for _ in 0..3 {
        let shown = get_date_parts_in_timezone(instant, timezone, local_offset_minutes)?;
        // `shown.month` is already 0-based, feeding Date.UTC's 0-based slot.
        let shown_ms = date_utc_ms(
            shown.year,
            shown.month,
            shown.day,
            shown.hours,
            shown.minutes,
            shown.seconds,
        );
        let drift = target - shown_ms;
        if drift == 0 {
            break;
        }
        instant += drift;
    }

    Ok(instant)
}

/// Format a `+HH:MM` / `-HH:MM` offset from a total minute count (east-positive),
/// matching v4's `getTimezoneOffset` output. `0` renders `+00:00` (the
/// named-zone UTC path, distinct from `toISOString()`'s trailing `Z`).
fn format_offset(total_minutes: i64) -> String {
    let sign = if total_minutes >= 0 { '+' } else { '-' };
    let abs = total_minutes.abs();
    format!("{sign}{:02}:{:02}", abs / 60, abs % 60)
}

/// v4 `getTimezoneOffset`: the `+HH:MM` string for the instant.
///
/// For a named zone v4 diffs the *displayed minute fields* of the instant in
/// that zone against UTC — never the raw offset — so the seconds of a
/// pre-standardisation LMT offset are dropped by two independent truncations,
/// and the printed offset therefore depends on the instant. `Europe/Istanbul`'s
/// 1550 LMT is +01:55:52: at :45:00 local the UTC clock reads :49:08 of the
/// previous hour, and v4 prints `+01:56`, not `+01:55`. Reproduced by taking the
/// same floor-to-the-minute on both sides. (Every minute-aligned modern zone is
/// unaffected — the two forms agree exactly there.)
///
/// For `None`, the injected host offset (JS convention negated to east-positive);
/// v4 reads `date.getTimezoneOffset()`, which is already whole minutes.
fn timezone_offset_string(
    utc_ms: i64,
    timezone: Option<&str>,
    local_offset_minutes: i64,
) -> Result<String, InvalidTimezone> {
    let minutes = match timezone {
        Some(tz) => {
            let offset_ms = zone_offset_seconds(utc_ms, tz)? * 1000;
            (utc_ms + offset_ms).div_euclid(60_000) - utc_ms.div_euclid(60_000)
        }
        None => -local_offset_minutes,
    };
    Ok(format_offset(minutes))
}

/// v4 `formatISO8601WithTimezone`: `YYYY-MM-DDTHH:MM:SS±HH:MM` (no millis).
fn format_iso8601_with_timezone(
    utc_ms: i64,
    timezone: Option<&str>,
    local_offset_minutes: i64,
) -> Result<String, InvalidTimezone> {
    let p = get_date_parts_in_timezone(utc_ms, timezone, local_offset_minutes)?;
    let offset = timezone_offset_string(utc_ms, timezone, local_offset_minutes)?;
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{offset}",
        p.year,
        p.month + 1,
        p.day,
        p.hours,
        p.minutes,
        p.seconds,
    ))
}

/// The `hour12`/`hour: 'numeric'` display hour: `12` for midnight/noon, else
/// `hours % 12`. Matches `Intl` `hour12` output (no leading zero) and the
/// FRIENDLY/TIME_ONLY templates.
fn hour12(hours: i64) -> i64 {
    let h = hours % 12;
    if h == 0 {
        12
    } else {
        h
    }
}

/// v4 FRIENDLY `Intl` format: `{Month} {D}, {YYYY} at {h}:{mm} {AM/PM}`.
fn format_friendly(p: &DatePartsInTimezone) -> String {
    let ampm = if p.hours < 12 { "AM" } else { "PM" };
    format!(
        "{} {}, {} at {}:{:02} {}",
        MONTHS[p.month as usize],
        p.day,
        p.year,
        hour12(p.hours),
        p.minutes,
        ampm
    )
}

/// v4 DATE_ONLY `Intl` format: `{Month} {D}, {YYYY}`.
fn format_date_only(p: &DatePartsInTimezone) -> String {
    format!("{} {}, {}", MONTHS[p.month as usize], p.day, p.year)
}

/// v4 TIME_ONLY `Intl` format: `{h}:{mm} {AM/PM}`.
fn format_time_only(p: &DatePartsInTimezone) -> String {
    let ampm = if p.hours < 12 { "AM" } else { "PM" };
    format!("{}:{:02} {}", hour12(p.hours), p.minutes, ampm)
}

/// v4 `formatCustom`: token-substitute a custom format string against the
/// zone-adjusted parts. Reproduces v4's *sequential global replace on the
/// accumulating result* in the exact longer-first order — so e.g. `M` is
/// substituted before `dddd`, and the `M` inside a produced day name like
/// "Monday" is not re-substituted (a real v4 quirk). Replacement values carry
/// no `$`, so a literal replace matches JS `String.replace(/…/g, str)`.
fn format_custom(p: &DatePartsInTimezone, format_string: &str) -> String {
    let year = p.year.to_string();
    let hours = p.hours;
    let minutes = p.minutes;
    let seconds = p.seconds;
    let h12 = hour12(hours);

    // Order matters: longer patterns must be replaced first (v4's order).
    let replacements: [(&str, String); 18] = [
        ("YYYY", year.clone()),
        ("YY", last2(&year)),
        ("MMMM", MONTHS[p.month as usize].to_string()),
        ("MMM", MONTHS_SHORT[p.month as usize].to_string()),
        ("MM", format!("{:02}", p.month + 1)),
        ("M", (p.month + 1).to_string()),
        ("dddd", DAY_NAMES[p.day_of_week as usize].to_string()),
        ("ddd", DAYS_SHORT[p.day_of_week as usize].to_string()),
        ("DD", format!("{:02}", p.day)),
        ("D", p.day.to_string()),
        ("HH", format!("{:02}", hours)),
        ("H", hours.to_string()),
        ("hh", format!("{:02}", h12)),
        ("h", h12.to_string()),
        ("mm", format!("{:02}", minutes)),
        ("ss", format!("{:02}", seconds)),
        ("A", (if hours < 12 { "AM" } else { "PM" }).to_string()),
        ("a", (if hours < 12 { "am" } else { "pm" }).to_string()),
    ];

    let mut result = format_string.to_string();
    for (pattern, replacement) in &replacements {
        result = result.replace(pattern, replacement);
    }
    result
}

/// JS `String(year).slice(-2)` — the last two UTF-16 code units. For the numeric
/// year string (ASCII), the last two chars.
fn last2(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    if n <= 2 {
        s.to_string()
    } else {
        chars[n - 2..].iter().collect()
    }
}

/// Resolve the timezone to use — v4 `resolveTimezone`. The fallback chain:
/// per-chat config → Salon default → `QUILLTAP_TIMEZONE` env → `None`. The env
/// var is read here (as v4 reads `process.env.QUILLTAP_TIMEZONE`); a caller that
/// needs determinism supplies the first two and controls the env. An empty
/// string is falsy in JS, so `""` is treated as "not set" and falls through.
pub fn resolve_timezone(
    config_timezone: Option<&str>,
    chat_settings_timezone: Option<&str>,
) -> Option<String> {
    if let Some(tz) = config_timezone {
        if !tz.is_empty() {
            return Some(tz.to_string());
        }
    }
    if let Some(tz) = chat_settings_timezone {
        if !tz.is_empty() {
            return Some(tz.to_string());
        }
    }
    match std::env::var("QUILLTAP_TIMEZONE") {
        Ok(tz) if !tz.is_empty() => Some(tz),
        _ => None,
    }
}

/// Calculate the current timestamp — v4 `calculateCurrentTimestamp`.
///
/// `now_ms` is the injected `Date.now()`. `local_offset_minutes` is the host's
/// `getTimezoneOffset()` value (positive = west of UTC), used only on the
/// `timezone == None` path. Returns [`InvalidTimezone`] if a named zone can't be
/// resolved (mirroring `Intl.DateTimeFormat`'s throw).
///
/// Fictional time: when active with a base timestamp, the timestamp advances by
/// real elapsed time since the fictional base was set (or since `now_ms` if no
/// real-base is recorded — v4's `new Date()` fallback, which makes elapsed 0).
/// A base timestamp that JS `new Date(...)` can't parse becomes `NaN` there and
/// propagates through the arithmetic; the corpus keeps bases well-formed.
pub fn calculate_current_timestamp(
    config: &TimestampConfig,
    timezone: Option<&str>,
    now_ms: i64,
    local_offset_minutes: i64,
) -> Result<CalculatedTimestamp, InvalidTimezone> {
    // v4 guards on JS truthiness, so an empty-string base is "no base".
    let fictional_base_timestamp = config
        .fictional_base_timestamp
        .as_deref()
        .filter(|s| !s.is_empty());
    let fictional_base_timestamp = fictional_base_timestamp.filter(|_| config.use_fictional_time);
    let (timestamp_ms, is_fictional) = match fictional_base_timestamp {
        Some(base) => {
            // The base is a clock reading in the story's own timezone, not the
            // server's — see `parse_timestamp_in_timezone`.
            let fictional_base = parse_timestamp_in_timezone(base, timezone, local_offset_minutes)?;
            // Without an anchor there is nothing to measure elapsed time
            // against, so the clock would report the base instant forever.
            // Writers stamp `fictionalBaseRealTime` (see
            // `ensure_fictional_base_real_time`); this fallback only covers rows
            // that predate that.
            let real_base = match config
                .fictional_base_real_time
                .as_deref()
                .filter(|s| !s.is_empty())
            {
                Some(s) => parse_date_ms(s, local_offset_minutes),
                None => now_ms,
            };
            let elapsed = now_ms - real_base;
            (fictional_base + elapsed, true)
        }
        None => (now_ms, false),
    };

    // Format with timezone support.
    let formatted = match config.format {
        TimestampFormat::Custom => match config.custom_format.as_deref() {
            Some(fmt) if !fmt.is_empty() => {
                let p = get_date_parts_in_timezone(timestamp_ms, timezone, local_offset_minutes)?;
                format_custom(&p, fmt)
            }
            // CUSTOM with no (or empty) format string → fall back to FRIENDLY.
            _ => {
                let p = get_date_parts_in_timezone(timestamp_ms, timezone, local_offset_minutes)?;
                format_friendly(&p)
            }
        },
        TimestampFormat::Iso8601 => match timezone {
            Some(_) => format_iso8601_with_timezone(timestamp_ms, timezone, local_offset_minutes)?,
            None => iso_from_unix_ms(timestamp_ms),
        },
        TimestampFormat::Friendly => {
            let p = get_date_parts_in_timezone(timestamp_ms, timezone, local_offset_minutes)?;
            format_friendly(&p)
        }
        TimestampFormat::DateOnly => {
            let p = get_date_parts_in_timezone(timestamp_ms, timezone, local_offset_minutes)?;
            format_date_only(&p)
        }
        TimestampFormat::TimeOnly => {
            let p = get_date_parts_in_timezone(timestamp_ms, timezone, local_offset_minutes)?;
            format_time_only(&p)
        }
    };

    // isoValue: offset form when a zone is specified, else toISOString() (millis + Z).
    let iso_value = match timezone {
        Some(_) => format_iso8601_with_timezone(timestamp_ms, timezone, local_offset_minutes)?,
        None => iso_from_unix_ms(timestamp_ms),
    };

    Ok(CalculatedTimestamp {
        formatted,
        iso_value,
        is_fictional,
    })
}

/// Parse an ISO-ish date string to Unix ms, matching JS `new Date(s).getTime()`
/// over the whole `Date.parse` ISO subset (`…Z`, `…±HH:MM`, and the zone-less
/// form JS reads against the host zone). An unparseable string is JS `NaN`,
/// which has no i64 counterpart, so it becomes `0`.
///
/// This is the `fictionalBaseRealTime` reader only. The *fictional base* goes
/// through [`parse_timestamp_in_timezone`] instead — it is a zone-less
/// `datetime-local` wall-clock reading, not an instant, and reading it here
/// would resolve it against the wrong zone (v4 `e3a9654f`).
fn parse_date_ms(s: &str, local_offset_minutes: i64) -> i64 {
    js_new_date_ms(s, local_offset_minutes).unwrap_or(0)
}

/// Determine whether a timestamp should be injected — v4 `shouldInjectTimestamp`.
///
/// `minutes_since_last_announcement`: for `EVERY_N_MINUTES`, minutes since the
/// most recent Host announcement. `None` fires the timestamp (v4 treats both
/// `null` and `undefined` as "fire").
pub fn should_inject_timestamp(
    config: Option<&TimestampConfig>,
    is_initial_message: bool,
    minutes_since_last_announcement: Option<i64>,
) -> bool {
    let config = match config {
        Some(c) if c.mode != TimestampMode::None => c,
        _ => return false,
    };
    match config.mode {
        TimestampMode::StartOnly => is_initial_message,
        TimestampMode::EveryMessage => true,
        TimestampMode::EveryNMinutes => {
            if is_initial_message {
                return true;
            }
            match minutes_since_last_announcement {
                None => true,
                Some(m) => {
                    // v4: `config.intervalMinutes ?? 15`. The parsed config always
                    // carries the Zod default (15), so the field is authoritative.
                    m >= config.interval_minutes
                }
            }
        }
        TimestampMode::None => false, // unreachable (guarded above)
    }
}

/// Format a calculated timestamp for a system prompt — v4
/// `formatTimestampForSystemPrompt`. With `auto_prepend`, prefixes
/// `Current time: `; otherwise returns the formatted string verbatim (for the
/// `{{timestamp}}` template variable).
pub fn format_timestamp_for_system_prompt(
    timestamp: &CalculatedTimestamp,
    auto_prepend: bool,
) -> String {
    if auto_prepend {
        format!("Current time: {}", timestamp.formatted)
    } else {
        timestamp.formatted.clone()
    }
}

/// JS truthiness of an optional JSON value — v4's guards are bare `!x` tests, so
/// `null`, `false`, `""` and `0` all read as "absent".
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Some(_) => true, // arrays and objects are truthy
    }
}

/// Anchor a fictional clock to real time, so it can actually advance — v4
/// `ensureFictionalBaseRealTime` (`e3a9654f`).
///
/// Story time runs 1:1 with the wall clock, measured from
/// `fictionalBaseRealTime`. Nothing populated that field for a long stretch, so
/// [`calculate_current_timestamp`] fell back to "now", measured zero elapsed
/// time, and re-reported the configured base on every single turn — the clock
/// read the same instant forever.
///
/// Every path that persists a fictional-time config must run it through here
/// first. Existing anchors are left alone: re-stamping a live chat would reset
/// its clock back to the base.
///
/// Salon- and character-level *defaults* are deliberately not anchored — a
/// default saved months ago carries no meaningful anchor. Chat creation is where
/// a config becomes a running clock, so that is where the stamp is applied
/// (inherited defaults included).
///
/// Operates on the raw `serde_json::Value` rather than the typed
/// [`TimestampConfig`], because v4's `{...config}` spread carries unknown keys
/// through untouched and the persisted blob is the thing being stamped. A
/// non-object (including `null`) passes straight through, as v4's `if (!config)`
/// guard does.
///
/// `anchor_ms` is v4's `anchor ?? new Date()` already resolved by the caller:
/// now for a fresh chat, the chat's `createdAt` when retro-fitting an existing
/// one.
pub fn ensure_fictional_base_real_time(config: &Value, anchor_ms: i64) -> Value {
    let Some(obj) = config.as_object() else {
        return config.clone();
    };
    if !js_truthy(obj.get("useFictionalTime")) || !js_truthy(obj.get("fictionalBaseTimestamp")) {
        return config.clone();
    }
    if js_truthy(obj.get("fictionalBaseRealTime")) {
        return config.clone();
    }

    let mut out = obj.clone();
    // Insert keeps an already-present (but falsy) key in its original slot, as
    // the JS spread does; a fresh key lands last.
    out.insert(
        "fictionalBaseRealTime".to_string(),
        Value::String(iso_from_unix_ms(anchor_ms)),
    );
    Value::Object(out)
}

// ===========================================================================
// The TimestampConfigSchema parse (P4.9E3B — the repository-write
// normalization, v4 `lib/schemas/settings.types.ts:76–95`)
// ===========================================================================

/// v4 `TimestampConfigSchema.parse` over one JSON value. The Zod semantics the
/// P4.d18 probe pinned (status-log.md, "the P4.d18 probe"): output keys in
/// SCHEMA DECLARATION ORDER regardless of input order; unknown keys stripped;
/// the five keyed defaults materialized when absent (`mode: "NONE"`,
/// `format: "FRIENDLY"`, `useFictionalTime: false`, `autoPrepend: true`,
/// `intervalMinutes: 15`); the four `.nullable().optional()` keys keep an
/// explicit `null` and stay ABSENT when absent; a bad value REJECTS (`Err` —
/// v4 throws a ZodError, which the create route answers as 400 and the
/// repository write surfaces as its own thrown validation).
///
/// `intervalMinutes` is `z.number().int().min(1)` — `Number.isInteger`
/// semantics (`1.0` passes and re-emits as `1`, the P4.6an precedent).
pub fn parse_timestamp_config(v: &Value) -> Result<Value, String> {
    let Some(obj) = v.as_object() else {
        return Err("expected object".to_string());
    };
    let mut out = serde_json::Map::new();

    // mode (enum, default 'NONE'). `.default()` applies to ABSENT only —
    // an explicit null rejects.
    let mode = match obj.get("mode") {
        None => "NONE".to_string(),
        Some(Value::String(s))
            if ["NONE", "START_ONLY", "EVERY_MESSAGE", "EVERY_N_MINUTES"].contains(&s.as_str()) =>
        {
            s.clone()
        }
        Some(_) => return Err("invalid mode".to_string()),
    };
    out.insert("mode".into(), Value::String(mode));

    let format = match obj.get("format") {
        None => "FRIENDLY".to_string(),
        Some(Value::String(s))
            if ["ISO8601", "FRIENDLY", "DATE_ONLY", "TIME_ONLY", "CUSTOM"]
                .contains(&s.as_str()) =>
        {
            s.clone()
        }
        Some(_) => return Err("invalid format".to_string()),
    };
    out.insert("format".into(), Value::String(format));

    // The `.nullable().optional()` string keys: absent stays absent, explicit
    // null survives, a non-string rejects.
    let nullable_string = |key: &str| -> Result<Option<Value>, String> {
        match obj.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(Value::Null)),
            Some(Value::String(s)) => Ok(Some(Value::String(s.clone()))),
            Some(_) => Err(format!("invalid {key}")),
        }
    };
    if let Some(v) = nullable_string("customFormat")? {
        out.insert("customFormat".into(), v);
    }

    let use_fictional = match obj.get("useFictionalTime") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("invalid useFictionalTime".to_string()),
    };
    out.insert("useFictionalTime".into(), Value::Bool(use_fictional));

    if let Some(v) = nullable_string("fictionalBaseTimestamp")? {
        out.insert("fictionalBaseTimestamp".into(), v);
    }
    if let Some(v) = nullable_string("fictionalBaseRealTime")? {
        out.insert("fictionalBaseRealTime".into(), v);
    }

    let auto_prepend = match obj.get("autoPrepend") {
        None => true,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("invalid autoPrepend".to_string()),
    };
    out.insert("autoPrepend".into(), Value::Bool(auto_prepend));

    if let Some(v) = nullable_string("timezone")? {
        out.insert("timezone".into(), v);
    }

    // intervalMinutes: int min 1, default 15. Number.isInteger over the f64
    // (1.0 passes, 1.5 rejects); re-emitted as a JSON integer.
    let interval = match obj.get("intervalMinutes") {
        None => 15i64,
        Some(Value::Number(n)) => {
            let f = n.as_f64().unwrap_or(f64::NAN);
            if !f.is_finite() || f.fract() != 0.0 || f.abs() >= 9.007_199_254_740_992e15 {
                return Err("invalid intervalMinutes".to_string());
            }
            let i = f as i64;
            if i < 1 {
                return Err("invalid intervalMinutes".to_string());
            }
            i
        }
        Some(_) => return Err("invalid intervalMinutes".to_string()),
    };
    out.insert("intervalMinutes".into(), Value::Number(interval.into()));

    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(format: TimestampFormat) -> TimestampConfig {
        TimestampConfig {
            mode: TimestampMode::EveryMessage,
            format,
            custom_format: None,
            use_fictional_time: false,
            fictional_base_timestamp: None,
            fictional_base_real_time: None,
            auto_prepend: true,
            timezone: None,
            interval_minutes: 15,
        }
    }

    // 2026-07-02T12:34:56.789Z
    const NOW: i64 = 1_782_995_696_789;

    #[test]
    fn friendly_utc() {
        let r = calculate_current_timestamp(&cfg(TimestampFormat::Friendly), Some("UTC"), NOW, 0)
            .unwrap();
        assert_eq!(r.formatted, "July 2, 2026 at 12:34 PM");
        assert_eq!(r.iso_value, "2026-07-02T12:34:56+00:00");
    }

    #[test]
    fn friendly_new_york_dst() {
        let r = calculate_current_timestamp(
            &cfg(TimestampFormat::Friendly),
            Some("America/New_York"),
            NOW,
            0,
        )
        .unwrap();
        assert_eq!(r.formatted, "July 2, 2026 at 8:34 AM");
        assert_eq!(r.iso_value, "2026-07-02T08:34:56-04:00");
    }

    #[test]
    fn iso_no_timezone_keeps_millis() {
        let r =
            calculate_current_timestamp(&cfg(TimestampFormat::Iso8601), None, NOW, 300).unwrap();
        assert_eq!(r.formatted, "2026-07-02T12:34:56.789Z");
        assert_eq!(r.iso_value, "2026-07-02T12:34:56.789Z");
    }

    #[test]
    fn invalid_zone_errors() {
        let e =
            calculate_current_timestamp(&cfg(TimestampFormat::Friendly), Some("Not/AZone"), NOW, 0);
        assert_eq!(e, Err(InvalidTimezone("Not/AZone".to_string())));
    }

    #[test]
    fn custom_tokens_reproduces_v4_sequential_replace_bug() {
        // v4's formatCustom does sequential global replace on the accumulating
        // result: after `dddd`→"Thursday", the later single-char `h`→"12" and
        // `a`→"pm" tokens corrupt the produced day name. This is a real v4 quirk
        // ("Thursday" → "T12ursdpmy"), reproduced faithfully.
        let mut c = cfg(TimestampFormat::Custom);
        c.custom_format = Some("YYYY-MM-DD HH:mm dddd".to_string());
        let r = calculate_current_timestamp(&c, Some("UTC"), NOW, 0).unwrap();
        assert_eq!(r.formatted, "2026-07-02 12:34 T12ursdpmy");
    }

    #[test]
    fn chatham_offset_45() {
        let r = calculate_current_timestamp(
            &cfg(TimestampFormat::Iso8601),
            Some("Pacific/Chatham"),
            NOW,
            0,
        )
        .unwrap();
        assert_eq!(r.iso_value, "2026-07-03T01:19:56+12:45");
    }

    #[test]
    fn should_inject_branches() {
        let none = TimestampConfig {
            mode: TimestampMode::None,
            ..cfg(TimestampFormat::Friendly)
        };
        assert!(!should_inject_timestamp(Some(&none), true, None));
        assert!(!should_inject_timestamp(None, true, None));

        let start = TimestampConfig {
            mode: TimestampMode::StartOnly,
            ..cfg(TimestampFormat::Friendly)
        };
        assert!(should_inject_timestamp(Some(&start), true, None));
        assert!(!should_inject_timestamp(Some(&start), false, None));

        let every = TimestampConfig {
            mode: TimestampMode::EveryMessage,
            ..cfg(TimestampFormat::Friendly)
        };
        assert!(should_inject_timestamp(Some(&every), false, None));

        let n = TimestampConfig {
            mode: TimestampMode::EveryNMinutes,
            interval_minutes: 15,
            ..cfg(TimestampFormat::Friendly)
        };
        assert!(should_inject_timestamp(Some(&n), true, Some(3)));
        assert!(should_inject_timestamp(Some(&n), false, None));
        assert!(!should_inject_timestamp(Some(&n), false, Some(14)));
        assert!(should_inject_timestamp(Some(&n), false, Some(15)));
    }

    #[test]
    fn fictional_time_advances() {
        let mut c = cfg(TimestampFormat::Iso8601);
        c.use_fictional_time = true;
        c.fictional_base_timestamp = Some("2000-01-01T00:00:00.000Z".to_string());
        c.fictional_base_real_time = Some("2026-07-02T12:34:56.789Z".to_string());
        // now == real base → elapsed 0 → fictional == base.
        let r = calculate_current_timestamp(&c, None, NOW, 0).unwrap();
        assert!(r.is_fictional);
        assert_eq!(r.iso_value, "2000-01-01T00:00:00.000Z");
    }

    #[test]
    fn format_for_prompt() {
        let ts = CalculatedTimestamp {
            formatted: "July 2, 2026 at 12:34 PM".to_string(),
            iso_value: "2026-07-02T12:34:56+00:00".to_string(),
            is_fictional: false,
        };
        assert_eq!(
            format_timestamp_for_system_prompt(&ts, true),
            "Current time: July 2, 2026 at 12:34 PM"
        );
        assert_eq!(
            format_timestamp_for_system_prompt(&ts, false),
            "July 2, 2026 at 12:34 PM"
        );
    }

    #[test]
    fn ensure_anchor_stamps_only_unanchored_fictional_clocks() {
        let fictional = serde_json::json!({
            "mode": "EVERY_MESSAGE",
            "useFictionalTime": true,
            "fictionalBaseTimestamp": "1776-07-04T16:30",
            "unknownKey": [1, 2],
        });
        let out = ensure_fictional_base_real_time(&fictional, NOW);
        assert_eq!(out["fictionalBaseRealTime"], "2026-07-02T12:34:56.789Z");
        // Unknown keys ride along, and the new key lands last.
        assert_eq!(out["unknownKey"], serde_json::json!([1, 2]));
        assert_eq!(
            out.as_object().unwrap().keys().next_back().unwrap(),
            "fictionalBaseRealTime"
        );

        // An existing anchor is never re-stamped — that would reset the clock.
        let anchored = serde_json::json!({
            "useFictionalTime": true,
            "fictionalBaseTimestamp": "1776-07-04T16:30",
            "fictionalBaseRealTime": "2020-05-05T05:05:05.000Z",
        });
        assert_eq!(ensure_fictional_base_real_time(&anchored, NOW), anchored);

        // Real-time and baseless configs are left alone; so is null.
        let real_time = serde_json::json!({ "useFictionalTime": false, "fictionalBaseTimestamp": "1776-07-04T16:30" });
        assert_eq!(ensure_fictional_base_real_time(&real_time, NOW), real_time);
        let baseless =
            serde_json::json!({ "useFictionalTime": true, "fictionalBaseTimestamp": null });
        assert_eq!(ensure_fictional_base_real_time(&baseless, NOW), baseless);
        assert_eq!(
            ensure_fictional_base_real_time(&Value::Null, NOW),
            Value::Null
        );
    }

    #[test]
    fn zoneless_base_reads_as_a_clock_in_the_story_zone() {
        // v4's worked example: 10:15 in 1550 Istanbul (LMT +01:55:52) is NOT
        // 10:15Z. Before `e3a9654f` this parsed against the server's zone; in v5
        // it parsed to 0 outright (`iso_to_ms` requires a trailing Z).
        let ms =
            parse_timestamp_in_timezone("1550-07-25T10:15", Some("Europe/Istanbul"), 0).unwrap();
        let p = get_date_parts_in_timezone(ms, Some("Europe/Istanbul"), 0).unwrap();
        assert_eq!(
            (p.year, p.month + 1, p.day, p.hours, p.minutes),
            (1550, 7, 25, 10, 15)
        );
        assert_ne!(ms, 0);

        // Absolute strings pass through untouched.
        assert_eq!(
            parse_timestamp_in_timezone("1776-07-04T16:30:00.000Z", Some("Europe/Istanbul"), 0)
                .unwrap(),
            crate::clock::iso_to_ms("1776-07-04T16:30:00.000Z").unwrap()
        );
        assert_eq!(
            parse_timestamp_in_timezone("2026-01-15T12:00:00+05:00", Some("America/Chicago"), 0)
                .unwrap(),
            crate::clock::iso_to_ms("2026-01-15T07:00:00.000Z").unwrap()
        );
    }

    #[test]
    fn fictional_clock_advances_from_its_anchor() {
        // The frozen-clock bug: 45 real minutes since the anchor must read as
        // 45 story minutes past the base.
        let mut c = cfg(TimestampFormat::Friendly);
        c.use_fictional_time = true;
        c.fictional_base_timestamp = Some("1550-07-25T10:15".to_string());
        c.fictional_base_real_time = Some(iso_from_unix_ms(NOW - 45 * 60_000));
        let r = calculate_current_timestamp(&c, Some("Europe/Istanbul"), NOW, 0).unwrap();
        assert!(r.is_fictional);
        assert_eq!(r.formatted, "July 25, 1550 at 11:00 AM");
    }
}
