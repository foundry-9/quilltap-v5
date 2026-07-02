//! Chat timestamp utilities — v4 `lib/chat/timestamp-utils.ts` ported.
//!
//! Calculates and formats the "current time" string injected into system
//! prompts, with IANA-timezone-aware output and optional fictional (auto-
//! incrementing) time. Mirrors v4's five exported functions:
//! [`resolve_timezone`], [`calculate_current_timestamp`], [`should_inject_timestamp`],
//! [`format_timestamp_for_system_prompt`], and [`initialize_fictional_time`].
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

/// Format a `+HH:MM` / `-HH:MM` offset from a total minute count (east-positive),
/// matching v4's `getTimezoneOffset` output. `0` renders `+00:00` (the
/// named-zone UTC path, distinct from `toISOString()`'s trailing `Z`).
fn format_offset(total_minutes: i64) -> String {
    let sign = if total_minutes >= 0 { '+' } else { '-' };
    let abs = total_minutes.abs();
    format!("{sign}{:02}:{:02}", abs / 60, abs % 60)
}

/// v4 `getTimezoneOffset`: the `+HH:MM` string for the instant. For a named
/// zone, jiff's second offset divided by 60 (all corpus/modern zones are
/// minute-aligned, matching v4's minute-granular date-part diff). For `None`,
/// the injected host offset (JS convention negated to east-positive).
fn timezone_offset_string(
    utc_ms: i64,
    timezone: Option<&str>,
    local_offset_minutes: i64,
) -> Result<String, InvalidTimezone> {
    let minutes = match timezone {
        Some(tz) => zone_offset_seconds(utc_ms, tz)? / 60,
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
    let (timestamp_ms, is_fictional) =
        if config.use_fictional_time && config.fictional_base_timestamp.is_some() {
            let fictional_base = parse_date_ms(config.fictional_base_timestamp.as_deref().unwrap());
            let real_base = match config.fictional_base_real_time.as_deref() {
                Some(s) => parse_date_ms(s),
                None => now_ms,
            };
            let elapsed = now_ms - real_base;
            (fictional_base + elapsed, true)
        } else {
            (now_ms, false)
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
/// for the well-formed shapes the corpus uses. Reuses [`crate::clock::iso_to_ms`]
/// (the `…Z` shape); for an offset-bearing or otherwise-unparseable string this
/// returns 0 — but the corpus keeps fictional bases in the plain-`Z` shape, so
/// the fallback is never exercised in the differential.
fn parse_date_ms(s: &str) -> i64 {
    crate::clock::iso_to_ms(s).unwrap_or(0)
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

/// Create a new config with fictional time set — v4 `initializeFictionalTime`.
/// Records `now_ms` (injected) as the real-base ISO string, matching v4's
/// `new Date().toISOString()`.
pub fn initialize_fictional_time(
    base_config: &TimestampConfig,
    fictional_timestamp: &str,
    now_ms: i64,
) -> TimestampConfig {
    TimestampConfig {
        use_fictional_time: true,
        fictional_base_timestamp: Some(fictional_timestamp.to_string()),
        fictional_base_real_time: Some(iso_from_unix_ms(now_ms)),
        ..base_config.clone()
    }
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
    fn init_fictional() {
        let base = cfg(TimestampFormat::Friendly);
        let out = initialize_fictional_time(&base, "2000-01-01T00:00:00.000Z", NOW);
        assert!(out.use_fictional_time);
        assert_eq!(
            out.fictional_base_timestamp.as_deref(),
            Some("2000-01-01T00:00:00.000Z")
        );
        assert_eq!(
            out.fictional_base_real_time.as_deref(),
            Some("2026-07-02T12:34:56.789Z")
        );
    }
}
