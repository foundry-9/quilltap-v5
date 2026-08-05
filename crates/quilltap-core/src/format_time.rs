//! Port of the subset of v4 `lib/format-time.ts` the Post Office needs —
//! `formatDate` / `formatDateTime`.
//!
//! v4 formats via `new Date(iso).toLocaleDateString(undefined, {...})` — the
//! **runtime's default locale and system timezone**. Confirmed empirically that
//! Node's default locale is `en-US` and the `monthStyle:'long'` + `hour/minute`
//! options render `"{Month} {D}, {YYYY} at {hh}:{mm} {AM/PM}"` (a 2-digit,
//! 12-hour clock). Because the timezone is the *system* zone (not an explicit
//! IANA zone), this is inherently machine-dependent; the differential pins the
//! oracle to **TZ=UTC / en-US**, and this port computes the parts in **UTC** to
//! match. (Documented harness constraint — the mailbox date is TZ-dependent by
//! v4's design; we do not "fix" it mid-port.)
//!
//! Uses `clock::iso_to_ms` (JS `Date.parse`) + the civil-date math from
//! `chat_timestamp`; en-US month names inline.

use crate::clock::iso_to_ms;

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

/// Civil (year, month 1-12, day 1-31) from days since the Unix epoch (UTC).
/// Howard Hinnant's algorithm — the same math `chat_timestamp` uses.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

/// v4 `formatDateTime(dateString, { monthStyle })` under TZ=UTC / en-US.
/// Returns `""` for an empty/None input; falls back to the raw string on an
/// unparseable date (v4's `catch` / `Invalid Date` edge is not reproduced — the
/// corpus uses valid ISO timestamps).
pub fn format_date_time(date_string: Option<&str>, month_style: MonthStyle) -> String {
    let Some(s) = date_string.filter(|s| !s.is_empty()) else {
        return String::new();
    };
    let Some(ms) = iso_to_ms(s) else {
        // v4 `toLocaleDateString` on `Invalid Date` → "Invalid Date"; not banked.
        return s.to_string();
    };
    // UTC civil parts.
    let days = ms.div_euclid(86_400_000);
    let ms_in_day = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let total_minutes = ms_in_day / 60_000;
    let hour24 = (total_minutes / 60) as u32;
    let minute = (total_minutes % 60) as u32;

    let month_name = match month_style {
        MonthStyle::Long => MONTHS_LONG[(month - 1) as usize],
        MonthStyle::Short => MONTHS_SHORT[(month - 1) as usize],
    };
    // 12-hour, 2-digit hour (en-US default): 00→12 AM, 13→01 PM, 12→12 PM.
    let am = hour24 < 12;
    let hour12 = match hour24 % 12 {
        0 => 12,
        h => h,
    };
    format!(
        "{month_name} {day}, {year} at {hour12:02}:{minute:02} {}",
        if am { "AM" } else { "PM" }
    )
}

/// v4 `new Date(iso).toLocaleDateString()` (no options) under TZ=UTC / en-US:
/// `"M/D/YYYY"` (no leading zeros). Used by the `search_web` built-in result
/// formatter. Returns `"Invalid Date"` for an unparseable input (matching V8's
/// non-throwing behavior) — the corpus uses valid dates.
pub fn format_date_short_us(iso: &str) -> String {
    let Some(ms) = iso_to_ms(iso) else {
        return "Invalid Date".to_string();
    };
    let days = ms.div_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    format!("{month}/{day}/{year}")
}

/// v4 `new Date(iso).toLocaleDateString()` (no options) under TZ=UTC / en-US —
/// `"M/D/YYYY"`, no leading zeros — over the **full `Date.parse` domain**
/// ([`crate::episodic::js_date_parse_ms`]) rather than the strict-ISO
/// [`format_date_short_us`]. `None` when the input is not a date JS would
/// accept, i.e. exactly when v4's `Number.isNaN(date.getTime())` is true.
///
/// The Almanack's `formatDate` is its only caller; it renders `None` as `"N/A"`.
pub fn locale_date_us(iso: &str) -> Option<String> {
    let ms = crate::episodic::js_date_parse_ms(iso)?;
    let days = ms.div_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    Some(format!("{month}/{day}/{year}"))
}

/// v4 `new Date(iso).toLocaleString()` (no options) under TZ=UTC / en-US —
/// `"M/D/YYYY, h:mm:ss AM/PM"`. The date half has no leading zeros and neither
/// does the 12-hour hour; minutes and seconds are 2-digit. Probed live against
/// Node 24 (`12:00:00 PM`, `9:07:03 AM`, `12:00:00 AM` at midnight).
///
/// `None` on an unparseable input (v4's `NaN` guard → `"N/A"`).
pub fn locale_date_time_us(iso: &str) -> Option<String> {
    let ms = crate::episodic::js_date_parse_ms(iso)?;
    let days = ms.div_euclid(86_400_000);
    let ms_in_day = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let total_seconds = ms_in_day / 1000;
    let hour24 = total_seconds / 3600;
    let minute = (total_seconds % 3600) / 60;
    let second = total_seconds % 60;
    let am = hour24 < 12;
    let hour12 = match hour24 % 12 {
        0 => 12,
        h => h,
    };
    Some(format!(
        "{month}/{day}/{year}, {hour12}:{minute:02}:{second:02} {}",
        if am { "AM" } else { "PM" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(locale_date_us(iso).as_deref(), Some(date), "date {iso}");
            assert_eq!(
                locale_date_time_us(iso).as_deref(),
                Some(date_time),
                "datetime {iso}"
            );
        }
        // Date-only forms parse in JS (and here) — unlike `format_date_short_us`.
        assert_eq!(locale_date_us("2026-01-05").as_deref(), Some("1/5/2026"));
        // What JS rejects, we reject — the renderer's "N/A" arm.
        assert_eq!(locale_date_us("not-a-date"), None);
        assert_eq!(locale_date_time_us(""), None);
    }

    #[test]
    fn format_date_short_us_matches_v4() {
        // Full-ISO inputs (the `iso_to_ms` strict form the corpus uses). A
        // date-only `publishedDate` (`"2026-01-05"`) is a documented seam — JS
        // `Date.parse` accepts it, the strict `iso_to_ms` does not.
        assert_eq!(
            format_date_short_us("2026-06-15T00:00:00.000Z"),
            "6/15/2026"
        );
        assert_eq!(format_date_short_us("2026-01-05T00:00:00.000Z"), "1/5/2026");
        assert_eq!(
            format_date_short_us("2020-12-31T23:00:00.000Z"),
            "12/31/2020"
        );
    }

    #[test]
    fn format_date_time_long_utc() {
        assert_eq!(
            format_date_time(Some("2026-02-01T09:05:00.000Z"), MonthStyle::Long),
            "February 1, 2026 at 09:05 AM"
        );
        assert_eq!(
            format_date_time(Some("2026-11-20T23:45:00.000Z"), MonthStyle::Long),
            "November 20, 2026 at 11:45 PM"
        );
        assert_eq!(
            format_date_time(Some("2026-02-01T00:00:00.000Z"), MonthStyle::Long),
            "February 1, 2026 at 12:00 AM"
        );
        // Noon → 12 PM.
        assert_eq!(
            format_date_time(Some("2026-02-01T12:00:00.000Z"), MonthStyle::Long),
            "February 1, 2026 at 12:00 PM"
        );
        assert_eq!(format_date_time(None, MonthStyle::Long), "");
        assert_eq!(format_date_time(Some(""), MonthStyle::Long), "");
    }
}
