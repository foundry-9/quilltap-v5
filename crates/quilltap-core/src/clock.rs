//! The wall-clock seam: ISO-8601 timestamps in v4's `new Date().toISOString()`
//! shape — UTC, millisecond precision, trailing `Z`
//! (`YYYY-MM-DDTHH:MM:SS.mmmZ`). This is v4's `getCurrentTimestamp()`
//! (`base.repository.ts`) ported for the repo create/update default path.
//!
//! The conversion (`iso_from_unix_ms`) is pure and unit-tested against fixed
//! vectors; `now_iso` is the only impure entry point (reads the system clock).
//! Repo ops that mint their own timestamps (the non-sync create path) call
//! `now_iso`; ops that pin timestamps (sync, batch extraction, the tier-2
//! fixtures) pass them in and never touch this module.

use std::time::{SystemTime, UNIX_EPOCH};

/// Civil date (year, month 1-12, day 1-31) from a count of days since the Unix
/// epoch (1970-01-01). Howard Hinnant's `civil_from_days` — the exact inverse
/// of the `days_from_civil` the harness already uses, valid across the full
/// proleptic Gregorian range.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m: i64 = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (y + if m <= 2 { 1 } else { 0 }, m as u32, d)
}

/// Format a Unix timestamp in milliseconds as v4's `toISOString()` string:
/// `YYYY-MM-DDTHH:MM:SS.mmmZ`. Pure and total for non-negative inputs (the only
/// regime a repo create reaches); negative pre-epoch inputs are not used.
pub fn iso_from_unix_ms(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let (y, mo, d) = civil_from_days(days);
    let hour = rem / 3_600_000;
    let min = (rem / 60_000) % 60;
    let sec = (rem / 1000) % 60;
    let milli = rem % 1000;
    format!("{y:04}-{mo:02}-{d:02}T{hour:02}:{min:02}:{sec:02}.{milli:03}Z")
}

/// The current wall-clock instant as an ISO-8601 string (v4's
/// `getCurrentTimestamp()`). The single impure call; everything else is pure.
pub fn now_iso() -> String {
    iso_from_unix_ms(now_unix_ms())
}

/// The current wall-clock instant as Unix milliseconds (v4's `Date.now()` /
/// `new Date().getTime()`), for services that compare stored timestamps against
/// "now" (housekeeping ages, throttle windows).
pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as i64
}

/// Days since the Unix epoch for a civil (y, m, d) — Howard Hinnant's
/// `days_from_civil`, the exact inverse of [`civil_from_days`].
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Parse a stored ISO-8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SS[.mmm]Z` — the only
/// shape the repos ever mint) to Unix milliseconds, matching JS
/// `new Date(s).getTime()` on that shape. Returns `None` for anything else (the
/// analogue of JS's `NaN` date, which callers treat as "no usable time").
pub fn iso_to_ms(s: &str) -> Option<i64> {
    let (date, rest) = s.split_once('T')?;
    let time = rest.strip_suffix('Z')?;
    let mut dparts = date.split('-');
    let y: i64 = dparts.next()?.parse().ok()?;
    let mo: i64 = dparts.next()?.parse().ok()?;
    let d: i64 = dparts.next()?.parse().ok()?;
    if dparts.next().is_some() {
        return None;
    }
    let (hms, millis) = match time.split_once('.') {
        Some((a, b)) => {
            if b.len() != 3 {
                return None;
            }
            (a, b.parse::<i64>().ok()?)
        }
        None => (time, 0),
    };
    let mut tparts = hms.split(':');
    let h: i64 = tparts.next()?.parse().ok()?;
    let mi: i64 = tparts.next()?.parse().ok()?;
    let se: i64 = tparts.next()?.parse().ok()?;
    if tparts.next().is_some() {
        return None;
    }
    let days = days_from_civil(y, mo, d);
    Some((days * 86_400 + h * 3_600 + mi * 60 + se) * 1000 + millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_from_unix_ms_fixed_vectors() {
        // Vectors confirmed against Node `new Date(ms).toISOString()`.
        assert_eq!(iso_from_unix_ms(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_from_unix_ms(1), "1970-01-01T00:00:00.001Z");
        assert_eq!(iso_from_unix_ms(1_000), "1970-01-01T00:00:01.000Z");
        // 2026-07-02T12:34:56.789Z
        assert_eq!(
            iso_from_unix_ms(1_782_995_696_789),
            "2026-07-02T12:34:56.789Z"
        );
        // A leap day: 2024-02-29T23:59:59.999Z
        assert_eq!(
            iso_from_unix_ms(1_709_251_199_999),
            "2024-02-29T23:59:59.999Z"
        );
    }

    #[test]
    fn iso_to_ms_round_trips_and_rejects_junk() {
        for ms in [0i64, 1, 1_000, 1_782_995_696_789, 1_709_251_199_999] {
            assert_eq!(iso_to_ms(&iso_from_unix_ms(ms)), Some(ms));
        }
        // Seconds-precision (no millis) parses too, as `Date.parse` does.
        assert_eq!(iso_to_ms("2020-01-01T00:00:00Z"), Some(1_577_836_800_000));
        assert_eq!(iso_to_ms("not a date"), None);
        assert_eq!(iso_to_ms("2020-01-01"), None);
    }

    #[test]
    fn now_iso_has_iso_millis_z_shape() {
        let s = now_iso();
        // YYYY-MM-DDTHH:MM:SS.mmmZ — 24 chars, fixed punctuation.
        assert_eq!(s.len(), 24, "unexpected length: {s}");
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "T");
        assert_eq!(&s[19..20], ".");
        assert!(s.ends_with('Z'));
        assert!(&s[0..4] >= "2026", "year looks wrong: {s}");
    }
}
