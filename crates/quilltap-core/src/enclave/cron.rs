//! The croner-semantics next-occurrence computation — U4.2 of the enclave
//! engine (`docs/developer/porting/enclave-engine.md`).
//!
//! v4 evaluates autonomous-room cron schedules with **croner 10.0.1** (JS), in
//! three identical one-shot call sites (`nextCronFireFrom` in
//! `autonomous-room-schedule-tick.ts`, `recomputeNextRun` in
//! `autonomous-room-turn.ts`, and the manual-start slot consumption in
//! `autonomous-room.service.ts`):
//!
//! ```text
//! try { return new Cron(cronExpr).nextRun(anchor) ?? null } catch { return null }
//! ```
//!
//! v4 passes **no timezone option**, so croner runs in Node's system-local
//! timezone using plain JS `Date` semantics: the anchor's wall-clock fields
//! come from the local getters (`getHours()` …), and the computed wall time is
//! mapped back to an instant with the local `new Date(y, m, d, …)` constructor.
//! That constructor's DST mapping (ES spec `LocalTZA`) is: for a *skipped*
//! wall time (spring-forward gap) use the offset **before** the transition,
//! and for a *repeated* wall time (fall-back fold) pick the **earlier**
//! occurrence — exactly `jiff`'s `Compatible` disambiguation strategy (gap →
//! `before` offset, fold → `before` offset). So the port takes the timezone as
//! an **injected IANA name** (production passes the host's local zone) and
//! maps wall ↔ instant through `jiff`, reproducing V8's local-`Date` behavior.
//!
//! **Crate decision (recorded per the work order):** no cron crate. The Rust
//! `croner` crate (3.0.1, same author) sits on chrono + chrono-tz and carries
//! its own DST disambiguation, which does not reproduce the *tz-less* JS-Date
//! path v4 actually exercises. The whole forward engine of croner-JS 10.0.1
//! (`CronPattern.parse` + `CronDate.increment/recurse/_findMatch/apply`,
//! `src/pattern.ts` + `src/date.ts`) is a few hundred lines and is transcribed
//! here byte-faithfully instead, over jiff for the two tzdb touch points. The
//! differential (`enclave_cron_equivalence`) drives v4's real installed croner
//! and pins the semantics, including the quirks:
//!
//! - `nextRun(anchor)` is strictly-after at **second** granularity: croner
//!   bumps `second += 1` and zeroes ms before searching, so an anchor exactly
//!   on a slot skips it, and odd-ms anchors round up to the next second.
//! - dom/dow **union** (legacy OR) when both are restricted; the `+` prefix on
//!   dow forces AND (`0 0 13 * +5` = Friday the 13th).
//! - `starDOM` is checked *before* `?` → `*` substitution, so `0 0 ? * 5`
//!   fires **every day** (dom reads as unrestricted-but-not-star → OR with
//!   dow) while `0 0 * * 5` fires Fridays only.
//! - Month/day name replacement is sequential-global, so `JANFEB` → `12` →
//!   December (v4's alpha-replace quirk, banked in the corpus).
//! - A fold wall time maps to the **earlier** occurrence, so an anchor between
//!   the two occurrences of a repeated 01:30 gets a `nextRun` *in the past*.
//! - `5#` (empty nth) is falsy → plain "any Friday"; `5L` = last Friday;
//!   `L`/`LW`/`15W` day-of-month modifiers per croner 10.
//! - Optional 6th (seconds) and 7th (year) fields; `@daily`-style nicknames
//!   (`@reboot` throws); 5-field seconds default to `0`.
//! - A pattern containing `:` at index > 0 is croner's **one-off datetime**
//!   branch: the string is parsed as a local (or `Z`/offset) ISO datetime,
//!   truncated to seconds, and returned iff strictly after the anchor.
//!
//! Documented bounded seams (conservative-failure, kept out of the corpus):
//! the one-off branch accepts the strict ISO-8601 subset
//! (`YYYY-MM-DDTHH:MM[:SS[.fff]][Z|±HH:MM]`, incl. V8's syntactic-day
//! normalization and `T24:00`) — V8's non-standard `Date.parse` fallback
//! formats (e.g. `"March 1, 2026 10:00"`, `±HHMM` offsets) are rejected here
//! (→ `None`) where croner would accept them; and absurd
//! out-of-`i64` numeric fields collapse to `None` where JS float `parseInt`
//! would throw a different error (both observably `null` in v4's catch).

use jiff::civil;
use jiff::tz::TimeZone;

/// Next occurrence of `cron_expr` strictly after `anchor_ms`, in the injected
/// IANA timezone `tz` — v4's `new Cron(cronExpr).nextRun(anchor)` with the
/// call-site try/catch collapsed to `None`. Returns epoch milliseconds;
/// callers reproduce v4's `.toISOString()` via [`crate::clock::iso_from_unix_ms`].
///
/// `None` covers all of v4's null-or-throw outcomes: a parse error, a valid
/// pattern with no future occurrence (croner's year-3000/-10000 search bound),
/// a one-off datetime at-or-before the anchor, and (defensively, not reachable
/// from v4 which always passes the host's own zone) an unknown timezone.
pub fn next_occurrence(cron_expr: &str, anchor_ms: i64, tz: &str) -> Option<i64> {
    try_next_occurrence(cron_expr, anchor_ms, tz).ok().flatten()
}

/// The throw-vs-null split of [`next_occurrence`]: `Err` where v4's
/// `new Cron(cronExpr)` constructor THROWS (an invalid cron pattern or an
/// unparseable one-off datetime — also, defensively, an unknown timezone),
/// `Ok(None)` where a valid construction's `nextRun(anchor)` returns null
/// (no future occurrence / a one-off at-or-before the anchor).
///
/// Most call sites collapse both to null (the schedule tick's and the turn
/// handler's try/catch), but `updateAutonomousRoomSettings` REJECTS the whole
/// edit on the constructor throw while accepting a valid-but-never-fires
/// pattern — the distinction is load-bearing there (`enclave::lifecycle`).
/// The error string is diagnostic only; v4 never persists or matches on it.
pub fn try_next_occurrence(
    cron_expr: &str,
    anchor_ms: i64,
    tz: &str,
) -> Result<Option<i64>, String> {
    let tz = TimeZone::get(tz).map_err(|_| format!("unknown timezone: {tz}"))?;
    // Cron constructor: a string containing ":" at UTF-16 index > 0 is the
    // one-off datetime branch, everything else parses as a cron pattern.
    if utf16_index_of(cron_expr, ':') > 0 {
        // croner parses the one-off datetime in the constructor; an
        // unparseable string throws there (`CronDate: Invalid date`). The
        // post-parse conversion seams inside `next_one_off` (absurd
        // out-of-range values, documented in the module header) stay
        // collapsed to `Ok(None)`.
        if parse_iso_local(cron_expr).is_none() {
            return Err(format!("invalid one-off datetime: {cron_expr}"));
        }
        Ok(next_one_off(cron_expr, anchor_ms, &tz))
    } else {
        let pat = parse_pattern(cron_expr)
            .map_err(|()| format!("invalid cron expression: {cron_expr}"))?;
        Ok(next_from_pattern(&pat, anchor_ms, &tz))
    }
}

// ---------------------------------------------------------------------------
// JS string/number primitives (croner runs on parseInt / \S+ / trim / Number)
// ---------------------------------------------------------------------------

/// JS `\s` / WhiteSpace ∪ LineTerminator set (used by `\S+` field split,
/// `String.prototype.trim`, and `parseInt`'s leading-whitespace skip).
fn is_js_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_ws)
}

/// UTF-16 index of the first occurrence of `needle`, or -1 (JS `indexOf`).
fn utf16_index_of(s: &str, needle: char) -> i64 {
    let mut idx = 0i64;
    for c in s.chars() {
        if c == needle {
            return idx;
        }
        idx += c.len_utf16() as i64;
    }
    -1
}

/// JS `parseInt(s, 10)`: skip JS whitespace, optional sign, leading ASCII
/// digit run; `None` = NaN. Saturating on overflow (JS would carry a float
/// there; every saturated value fails the same bounds checks → same `null`).
fn js_parse_int(s: &str) -> Option<i64> {
    let t = s.trim_start_matches(is_js_ws);
    let mut it = t.chars().peekable();
    let mut neg = false;
    match it.peek() {
        Some('+') => {
            it.next();
        }
        Some('-') => {
            neg = true;
            it.next();
        }
        _ => {}
    }
    let mut saw = false;
    let mut v: i64 = 0;
    while let Some(&c) = it.peek() {
        match c.to_digit(10) {
            Some(d) => {
                saw = true;
                v = v.saturating_mul(10).saturating_add(d as i64);
                it.next();
            }
            None => break,
        }
    }
    if saw {
        Some(if neg { -v } else { v })
    } else {
        None
    }
}

/// JS `Number(s)` restricted to the integer strings reachable here (the nth
/// modifier of a day-of-week entry, alphabet `[0-9]` after validation).
/// `None` = NaN.
fn js_number_int(s: &str) -> Option<i64> {
    let t = js_trim(s);
    let (neg, digits) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut v: i64 = 0;
    for c in digits.chars() {
        v = v.saturating_mul(10).saturating_add((c as u8 - b'0') as i64);
    }
    Some(if neg { -v } else { v })
}

/// Sequential global case-insensitive literal replace (JS
/// `str.replace(/needle/gi, repl)`), ASCII needles only.
fn js_replace_ci(s: &str, needle: &str, repl: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let lower_s = s.to_ascii_lowercase();
    let lower_n = needle.to_ascii_lowercase();
    let mut pos = 0usize;
    while let Some(found) = lower_s[pos..].find(&lower_n) {
        let at = pos + found;
        out.push_str(&s[pos..at]);
        out.push_str(repl);
        pos = at + needle.len();
    }
    out.push_str(&s[pos..]);
    out
}

/// JS `String.prototype.length` (UTF-16 code units).
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

// ---------------------------------------------------------------------------
// Proleptic-Gregorian day arithmetic (Date.UTC / getUTCDay equivalents)
// ---------------------------------------------------------------------------

/// Minimum days per month, February pinned to 28 (croner's `DaysOfMonth`).
const DAYS_OF_MONTH: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

const LAST_OCCURRENCE: i64 = 0b100000;
const ANY_OCCURRENCE: i64 = 0b111111; // all five nth bits | LAST_OCCURRENCE
const OCCURRENCE_BITMASKS: [i64; 5] = [0b00001, 0b00010, 0b00100, 0b01000, 0b10000];

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm);
/// `m` is 1-12.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`]; returns (year, month 1-12, day).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

/// `new Date(Date.UTC(y, m0, d)).getUTCDay()` — 0 = Sunday. 1970-01-01 was a
/// Thursday (4).
fn weekday_of(y: i64, m0: i64, d: i64) -> i64 {
    (days_from_civil(y, m0 + 1, d) + 4).rem_euclid(7)
}

/// croner `getLastDayOfMonth` (month 0-11).
fn last_day_of_month(y: i64, m0: i64) -> i64 {
    if m0 != 1 {
        DAYS_OF_MONTH[m0 as usize]
    } else if is_leap(y) {
        29
    } else {
        28
    }
}

/// croner `getLastWeekday`: the last Mon-Fri day of the month.
fn last_weekday_of_month(y: i64, m0: i64) -> i64 {
    let last = last_day_of_month(y, m0);
    match weekday_of(y, m0, last) {
        0 => last - 2, // Sunday → Friday
        6 => last - 1, // Saturday → Friday
        _ => last,
    }
}

/// croner `getNearestWeekday` (the `W` modifier); -1 = day absent this month.
fn nearest_weekday(y: i64, m0: i64, day: i64) -> i64 {
    let dim = last_day_of_month(y, m0);
    if day > dim {
        return -1;
    }
    match weekday_of(y, m0, day) {
        0 => {
            if day == dim {
                day - 2
            } else {
                day + 1
            }
        }
        6 => {
            if day == 1 {
                day + 2
            } else {
                day - 1
            }
        }
        _ => day,
    }
}

/// croner `isNthWeekdayOfMonth`: does `day` satisfy the nth-occurrence bitmask
/// for its own weekday?
fn is_nth_weekday_of_month(y: i64, m0: i64, day: i64, nth: i64) -> bool {
    let wd = weekday_of(y, m0, day);
    let mut count = 0usize;
    for d in 1..=day {
        if weekday_of(y, m0, d) == wd {
            count += 1;
        }
    }
    // count is ≥ 1 (the day itself) and ≤ 5.
    if (nth & ANY_OCCURRENCE) != 0 && (OCCURRENCE_BITMASKS[count - 1] & nth) != 0 {
        return true;
    }
    if (nth & LAST_OCCURRENCE) != 0 {
        let dim = last_day_of_month(y, m0);
        for d in (day + 1)..=dim {
            if weekday_of(y, m0, d) == wd {
                return false;
            }
        }
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Pattern parsing — croner 10.0.1 `CronPattern` with v4's defaults
// (mode "auto", standard weekdays, strict stepping, legacy OR dom/dow)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Second,
    Minute,
    Hour,
    Day,
    Month,
    DayOfWeek,
    NearestWeekdays,
    Year,
}

struct Pattern {
    second: [i64; 60],
    minute: [i64; 60],
    hour: [i64; 24],
    day: [i64; 31],
    month: [i64; 12],
    day_of_week: [i64; 7],
    year: Vec<i64>, // length 10000, index = absolute year
    last_day_of_month: bool,
    last_weekday: bool,
    nearest_weekdays: [i64; 31],
    star_dom: bool,
    star_dow: bool,
    star_year: bool,
    use_and_logic: bool,
}

impl Pattern {
    fn empty() -> Self {
        Pattern {
            second: [0; 60],
            minute: [0; 60],
            hour: [0; 24],
            day: [0; 31],
            month: [0; 12],
            day_of_week: [0; 7],
            year: vec![0; 10_000],
            last_day_of_month: false,
            last_weekday: false,
            nearest_weekdays: [0; 31],
            star_dom: false,
            star_dow: false,
            star_year: false,
            use_and_logic: false,
        }
    }

    fn arr_len(&self, part: Part) -> i64 {
        match part {
            Part::Second | Part::Minute => 60,
            Part::Hour => 24,
            Part::Day | Part::NearestWeekdays => 31,
            Part::Month => 12,
            Part::DayOfWeek => 7,
            Part::Year => self.year.len() as i64,
        }
    }

    fn fill(&mut self, part: Part, value: i64) {
        match part {
            Part::Second => self.second.fill(value),
            Part::Minute => self.minute.fill(value),
            Part::Hour => self.hour.fill(value),
            Part::Day => self.day.fill(value),
            Part::Month => self.month.fill(value),
            Part::DayOfWeek => self.day_of_week.fill(value),
            Part::NearestWeekdays => self.nearest_weekdays.fill(value),
            Part::Year => self.year.fill(value),
        }
    }

    fn write(&mut self, part: Part, index: usize, value: i64) {
        match part {
            Part::Second => self.second[index] = value,
            Part::Minute => self.minute[index] = value,
            Part::Hour => self.hour[index] = value,
            Part::Day => self.day[index] = value,
            Part::Month => self.month[index] = value,
            Part::DayOfWeek => self.day_of_week[index] = value,
            Part::NearestWeekdays => self.nearest_weekdays[index] = value,
            Part::Year => self.year[index] = value,
        }
    }
}

/// The value written into a slot: the field's default (1, or the full
/// occurrence mask for day-of-week) or a `#`/`L` nth-modifier string.
#[derive(Clone)]
enum Val {
    Num(i64),
    Nth(String),
}

fn nth_or_default(nth: &Option<String>, default: i64) -> Val {
    // JS `result[1] || defaultValue` — an empty nth string is falsy.
    match nth {
        Some(s) if !s.is_empty() => Val::Nth(s.clone()),
        _ => Val::Num(default),
    }
}

/// croner `handleNicknames` + the constructor's `.trim()`. `Err` = throw.
fn handle_nicknames(pattern: &str) -> Result<String, ()> {
    let clean = js_trim(pattern).to_lowercase();
    let replaced = match clean.as_str() {
        "@yearly" | "@annually" => "0 0 1 1 *",
        "@monthly" => "0 0 1 * *",
        "@weekly" => "0 0 * * 0",
        "@daily" | "@midnight" => "0 0 * * *",
        "@hourly" => "0 * * * *",
        "@reboot" => return Err(()), // v4-observable throw
        _ => pattern,
    };
    Ok(js_trim(replaced).to_string())
}

/// croner `replaceAlphaMonths` — sequential global replaces (so `JANFEB` →
/// `12`, faithfully).
fn replace_alpha_months(conf: &str) -> String {
    let pairs = [
        ("jan", "1"),
        ("feb", "2"),
        ("mar", "3"),
        ("apr", "4"),
        ("may", "5"),
        ("jun", "6"),
        ("jul", "7"),
        ("aug", "8"),
        ("sep", "9"),
        ("oct", "10"),
        ("nov", "11"),
        ("dec", "12"),
    ];
    let mut s = conf.to_string();
    for (n, r) in pairs {
        s = js_replace_ci(&s, n, r);
    }
    s
}

/// croner `replaceAlphaDays` (standard mode) — `-sun` → `-7` first so Sunday
/// can be a range upper bound.
fn replace_alpha_days(conf: &str) -> String {
    let pairs = [
        ("-sun", "-7"),
        ("sun", "0"),
        ("mon", "1"),
        ("tue", "2"),
        ("wed", "3"),
        ("thu", "4"),
        ("fri", "5"),
        ("sat", "6"),
    ];
    let mut s = conf.to_string();
    for (n, r) in pairs {
        s = js_replace_ci(&s, n, r);
    }
    s
}

/// croner `throwAtIllegalCharacters` field alphabets (post-alpha, post-`?`).
fn field_chars_legal(part_index: usize, s: &str) -> bool {
    s.chars().all(|c| {
        let base = c == '/' || c == '*' || c.is_ascii_digit() || c == ',' || c == '-';
        match part_index {
            3 => base || matches!(c, 'W' | 'w' | 'L' | 'l'),
            5 => base || matches!(c, '#' | 'L' | 'l'),
            _ => base,
        }
    })
}

/// croner `extractNth`: split out a `#n` or trailing-`L` day-of-week modifier.
fn extract_nth(conf: &str, part: Part) -> Result<(String, Option<String>), ()> {
    if conf.contains('#') {
        if part != Part::DayOfWeek {
            return Err(());
        }
        let mut sp = conf.split('#');
        let rest = sp.next().unwrap_or("").to_string();
        let nth = sp.next().unwrap_or("").to_string(); // "5#" → "" (falsy → default)
        Ok((rest, Some(nth)))
    } else if conf.to_uppercase().ends_with('L') {
        if part != Part::DayOfWeek {
            return Err(());
        }
        Ok((conf[..conf.len() - 1].to_string(), Some("L".to_string())))
    } else {
        Ok((conf.to_string(), None))
    }
}

/// croner `setPart` (+ `setNthWeekdayOfMonth` for day-of-week).
fn set_part(pat: &mut Pattern, part: Part, index: i64, value: Val) -> Result<(), ()> {
    if part == Part::DayOfWeek {
        let index = if index == 7 { 0 } else { index };
        if !(0..=6).contains(&index) {
            return Err(());
        }
        return set_nth_weekday_of_month(pat, index as usize, value);
    }
    let in_bounds = match part {
        Part::Second | Part::Minute => (0..60).contains(&index),
        Part::Hour => (0..24).contains(&index),
        Part::Day | Part::NearestWeekdays => (0..31).contains(&index),
        Part::Month => (0..12).contains(&index),
        Part::Year => (1..10_000).contains(&index),
        Part::DayOfWeek => unreachable!(),
    };
    if !in_bounds {
        return Err(());
    }
    // A nth string can only reach the day-of-week branch (extractNth throws
    // for every other part), so `value` is always numeric here.
    let n = match value {
        Val::Num(n) => n,
        Val::Nth(_) => return Err(()),
    };
    pat.write(part, index as usize, n);
    Ok(())
}

fn set_nth_weekday_of_month(pat: &mut Pattern, index: usize, value: Val) -> Result<(), ()> {
    match &value {
        Val::Nth(s) if s.eq_ignore_ascii_case("l") => {
            pat.day_of_week[index] |= LAST_OCCURRENCE;
            Ok(())
        }
        Val::Num(n) if *n == ANY_OCCURRENCE => {
            pat.day_of_week[index] = ANY_OCCURRENCE;
            Ok(())
        }
        v => {
            // JS numeric coercion of the nth, valid strictly between 0 and 6.
            let n = match v {
                Val::Num(n) => Some(*n),
                Val::Nth(s) => js_number_int(s),
            };
            match n {
                Some(n) if n > 0 && n < 6 => {
                    pat.day_of_week[index] |= OCCURRENCE_BITMASKS[(n - 1) as usize];
                    Ok(())
                }
                _ => Err(()),
            }
        }
    }
}

fn handle_number(
    pat: &mut Pattern,
    part: Part,
    conf: &str,
    offset: i64,
    default: i64,
) -> Result<(), ()> {
    let (rest, nth) = extract_nth(conf, part)?;
    let nearest = conf.to_uppercase().contains('W');
    if nearest && part != Part::Day {
        return Err(());
    }
    let part = if nearest { Part::NearestWeekdays } else { part };
    let i = js_parse_int(&rest).ok_or(())? + offset;
    set_part(pat, part, i, nth_or_default(&nth, default))
}

fn handle_range(
    pat: &mut Pattern,
    part: Part,
    conf: &str,
    offset: i64,
    default: i64,
) -> Result<(), ()> {
    if conf.to_uppercase().contains('W') {
        return Err(());
    }
    let (rest, nth) = extract_nth(conf, part)?;
    let sp: Vec<&str> = rest.split('-').collect();
    if sp.len() != 2 {
        return Err(());
    }
    let lower = js_parse_int(sp[0]).ok_or(())? + offset;
    let upper = js_parse_int(sp[1]).ok_or(())? + offset;
    if lower > upper {
        return Err(());
    }
    let val = nth_or_default(&nth, default);
    let mut i = lower;
    while i <= upper {
        set_part(pat, part, i, val.clone())?;
        i += 1;
    }
    Ok(())
}

fn handle_range_with_stepping(
    pat: &mut Pattern,
    part: Part,
    conf: &str,
    offset: i64,
    default: i64,
) -> Result<(), ()> {
    if conf.to_uppercase().contains('W') {
        return Err(());
    }
    let (rest, nth) = extract_nth(conf, part)?;
    // ^(\d+)-(\d+)/(\d+)$
    let dash = rest.find('-').ok_or(())?;
    let slash = rest.find('/').ok_or(())?;
    if slash < dash {
        return Err(());
    }
    let (a, b, c) = (&rest[..dash], &rest[dash + 1..slash], &rest[slash + 1..]);
    let all_digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if !all_digits(a) || !all_digits(b) || !all_digits(c) {
        return Err(());
    }
    let lower = js_parse_int(a).ok_or(())? + offset;
    let upper = js_parse_int(b).ok_or(())? + offset;
    let steps = js_parse_int(c).ok_or(())?;
    if steps == 0 || steps > pat.arr_len(part) || lower > upper {
        return Err(());
    }
    let val = nth_or_default(&nth, default);
    let mut i = lower;
    while i <= upper {
        set_part(pat, part, i, val.clone())?;
        i += steps;
    }
    Ok(())
}

fn handle_stepping(pat: &mut Pattern, part: Part, conf: &str, default: i64) -> Result<(), ()> {
    if conf.to_uppercase().contains('W') {
        return Err(());
    }
    let (rest, nth) = extract_nth(conf, part)?;
    let sp: Vec<&str> = rest.split('/').collect();
    if sp.len() != 2 {
        return Err(());
    }
    // Strict (non-sloppy) stepping: only the wildcard prefix is allowed —
    // `/10` and `0/10` both throw in croner 10's default configuration.
    if sp[0].is_empty() || sp[0] != "*" {
        return Err(());
    }
    let steps = js_parse_int(sp[1]).ok_or(())?;
    let len = pat.arr_len(part);
    if steps == 0 || steps > len {
        return Err(());
    }
    let val = nth_or_default(&nth, default);
    let mut i = 0i64;
    while i < len {
        // Stepping writes raw array indices (no valueIndexOffset), matching
        // croner: `*/10` in day-of-month = days 1, 11, 21, 31.
        set_part(pat, part, i, val.clone())?;
        i += steps;
    }
    Ok(())
}

fn part_to_array(
    pat: &mut Pattern,
    part: Part,
    conf: &str,
    offset: i64,
    default: i64,
) -> Result<(), ()> {
    // Empty entries throw, except an emptied day field whose `L`/`LW` modifier
    // was stripped into the pattern-level flags.
    let exempt = part == Part::Day && (pat.last_day_of_month || pat.last_weekday);
    if conf.is_empty() && !exempt {
        return Err(());
    }
    if conf == "*" {
        pat.fill(part, default);
        return Ok(());
    }
    let split: Vec<&str> = conf.split(',').collect();
    if split.len() > 1 {
        for sub in split {
            part_to_array(pat, part, sub, offset, default)?;
        }
        return Ok(());
    }
    if conf.contains('-') && conf.contains('/') {
        handle_range_with_stepping(pat, part, conf, offset, default)
    } else if conf.contains('-') {
        handle_range(pat, part, conf, offset, default)
    } else if conf.contains('/') {
        handle_stepping(pat, part, conf, default)
    } else if !conf.is_empty() {
        handle_number(pat, part, conf, offset, default)
    } else {
        Ok(())
    }
}

/// croner `CronPattern.parse` with v4's defaults. `Err(())` = any throw
/// (collapsed by the call-site catch).
fn parse_pattern(raw: &str) -> Result<Pattern, ()> {
    let mut pat = Pattern::empty();

    // Nicknames (@daily etc.) — the check is `indexOf("@") >= 0`.
    let pattern_str = if raw.contains('@') {
        handle_nicknames(raw)?
    } else {
        raw.to_string()
    };

    // Split on any JS whitespace run (`match(/\S+/g)`).
    let mut parts: Vec<String> = pattern_str
        .split(is_js_ws)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    if parts.len() < 5 || parts.len() > 7 {
        return Err(());
    }
    // Seconds default, year wildcard.
    if parts.len() == 5 {
        parts.insert(0, "0".to_string());
    }
    if parts.len() == 6 {
        parts.push("*".to_string());
    }

    // `LW` / `L` in day-of-month become pattern-level flags.
    if parts[3].to_uppercase() == "LW" {
        pat.last_weekday = true;
        parts[3] = String::new();
    } else if parts[3].to_uppercase().contains('L') {
        parts[3] = parts[3].replace(['L', 'l'], "");
        pat.last_day_of_month = true;
    }

    // starDOM / starYear are checked BEFORE `?` substitution (the `?`-quirk).
    pat.star_dom = parts[3] == "*";
    pat.star_year = parts[6] == "*";

    // Alpha month/day names (JS `.length >= 3` is UTF-16 units).
    if utf16_len(&parts[4]) >= 3 {
        parts[4] = replace_alpha_months(&parts[4]);
    }
    if utf16_len(&parts[5]) >= 3 {
        parts[5] = replace_alpha_days(&parts[5]);
    }

    // `+` day-of-week prefix = explicit AND logic.
    if let Some(rest) = parts[5].strip_prefix('+') {
        pat.use_and_logic = true;
        parts[5] = rest.to_string();
        if parts[5].is_empty() {
            return Err(());
        }
    }

    // starDOW also precedes `?` substitution.
    pat.star_dow = parts[5] == "*";

    // `?` is a wildcard alias, substituted across every field.
    if pattern_str.contains('?') {
        for p in parts.iter_mut() {
            *p = p.replace('?', "*");
        }
    }

    // Field alphabets.
    for (i, p) in parts.iter().enumerate() {
        if !field_chars_legal(i, p) {
            return Err(());
        }
    }

    part_to_array(&mut pat, Part::Second, &parts[0], 0, 1)?;
    part_to_array(&mut pat, Part::Minute, &parts[1], 0, 1)?;
    part_to_array(&mut pat, Part::Hour, &parts[2], 0, 1)?;
    part_to_array(&mut pat, Part::Day, &parts[3], -1, 1)?;
    part_to_array(&mut pat, Part::Month, &parts[4], -1, 1)?;
    part_to_array(&mut pat, Part::DayOfWeek, &parts[5], 0, ANY_OCCURRENCE)?;
    part_to_array(&mut pat, Part::Year, &parts[6], 0, 1)?;
    // (croner's `dayOfWeek[7] → [0]` copy is dead code at length 7 — setPart
    // already normalizes 7 to 0 — so there is nothing to mirror.)

    Ok(pat)
}

// ---------------------------------------------------------------------------
// The increment engine — croner `CronDate.increment/recurse/_findMatch/apply`
// ---------------------------------------------------------------------------

/// croner `CronDate`'s field bag: a wall-clock time in the target zone.
/// `month` is 0-11, `day` 1-31 (like the JS original).
#[derive(Clone, Copy)]
struct Wall {
    ms: i64,
    second: i64,
    minute: i64,
    hour: i64,
    day: i64,
    month: i64,
    year: i64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

/// croner `RecursionSteps`: (target, index offset); the parent of step `n` is
/// implicit in [`inc_parent`].
const STEPS: [(Target, i64); 5] = [
    (Target::Month, 0),
    (Target::Day, -1),
    (Target::Hour, 0),
    (Target::Minute, 0),
    (Target::Second, 0),
];

fn get_field(w: &Wall, t: Target) -> i64 {
    match t {
        Target::Month => w.month,
        Target::Day => w.day,
        Target::Hour => w.hour,
        Target::Minute => w.minute,
        Target::Second => w.second,
    }
}

fn set_field(w: &mut Wall, t: Target, v: i64) {
    match t {
        Target::Month => w.month = v,
        Target::Day => w.day = v,
        Target::Hour => w.hour = v,
        Target::Minute => w.minute = v,
        Target::Second => w.second = v,
    }
}

fn inc_parent(w: &mut Wall, doing: usize) {
    match doing {
        0 => w.year += 1,
        1 => w.month += 1,
        2 => w.day += 1,
        3 => w.hour += 1,
        _ => w.minute += 1,
    }
}

fn pattern_arr(pat: &Pattern, t: Target) -> &[i64] {
    match t {
        Target::Month => &pat.month,
        Target::Day => &pat.day,
        Target::Hour => &pat.hour,
        Target::Minute => &pat.minute,
        Target::Second => &pat.second,
    }
}

/// croner `CronDate.apply()`: if any field could be out of bounds, renormalize
/// the whole bag through `Date.UTC` semantics (fields may be arbitrarily out
/// of range, including Feb 29 in a leap year which round-trips unchanged but
/// still returns true — the caller relies on that for the month re-check).
fn apply(w: &mut Wall) -> bool {
    let out = w.month > 11
        || w.month < 0
        || w.day > DAYS_OF_MONTH[w.month as usize]
        || w.day < 1
        || w.hour > 59
        || w.minute > 59
        || w.second > 59
        || w.hour < 0
        || w.minute < 0
        || w.second < 0;
    if !out {
        return false;
    }
    let ym = w.year + w.month.div_euclid(12);
    let mn = w.month.rem_euclid(12);
    let days = days_from_civil(ym, mn + 1, 1) + (w.day - 1);
    let total =
        days * 86_400_000 + w.hour * 3_600_000 + w.minute * 60_000 + w.second * 1_000 + w.ms;
    let day_num = total.div_euclid(86_400_000);
    let mut rem = total.rem_euclid(86_400_000);
    let (y, m, d) = civil_from_days(day_num);
    w.year = y;
    w.month = m - 1;
    w.day = d;
    w.hour = rem / 3_600_000;
    rem %= 3_600_000;
    w.minute = rem / 60_000;
    rem %= 60_000;
    w.second = rem / 1_000;
    w.ms = rem % 1_000;
    true
}

/// croner `_findMatch` (forward direction): 1 = current value matches,
/// 2 = advanced to a later value, 3 = no match at this level.
fn find_next(w: &mut Wall, target: Target, pat: &Pattern, offset: i64) -> u8 {
    let original = get_field(w, target);
    let precomputed_last_day = if pat.last_day_of_month {
        Some(last_day_of_month(w.year, w.month))
    } else {
        None
    };
    let f_dom_week_day = if !pat.star_dow && target == Target::Day {
        Some(weekday_of(w.year, w.month, 1))
    } else {
        None
    };
    let arr = pattern_arr(pat, target);
    let len = arr.len() as i64;

    let mut i = original + offset;
    while i < len {
        let mut mtch: i64 = if i >= 0 { arr[i as usize] } else { 0 };

        // Nearest-weekday (`W`) days.
        if target == Target::Day && mtch == 0 {
            for day_with_w in 0..31i64 {
                if pat.nearest_weekdays[day_with_w as usize] != 0 {
                    let execution_day = nearest_weekday(w.year, w.month, day_with_w - offset);
                    if execution_day == -1 {
                        continue;
                    }
                    if execution_day == i - offset {
                        mtch = 1;
                        break;
                    }
                }
            }
        }
        // Last weekday of month (`LW`).
        if target == Target::Day
            && pat.last_weekday
            && i - offset == last_weekday_of_month(w.year, w.month)
        {
            mtch = 1;
        }
        // Last day of month (`L`).
        if target == Target::Day
            && pat.last_day_of_month
            && Some(i - offset) == precomputed_last_day
        {
            mtch = 1;
        }
        // Day-of-week, combined with day-of-month by croner's legacy rule:
        // `+` forces AND; otherwise a non-star dom ORs with dow (the union),
        // and a star dom ANDs (dom side is all-ones, so dow decides).
        if target == Target::Day && !pat.star_dow {
            let f_dom = f_dom_week_day.unwrap_or(0);
            let mut dow_match = pat.day_of_week[((f_dom + ((i - offset) - 1)) % 7) as usize];
            if dow_match != 0 && (dow_match & ANY_OCCURRENCE) != 0 {
                dow_match = if is_nth_weekday_of_month(w.year, w.month, i - offset, dow_match) {
                    1
                } else {
                    0
                };
            }
            let dom_hit = mtch != 0;
            let dow_hit = dow_match != 0;
            let hit = if pat.use_and_logic {
                dom_hit && dow_hit
            } else if !pat.star_dom {
                dom_hit || dow_hit
            } else {
                dom_hit && dow_hit
            };
            mtch = if hit { 1 } else { 0 };
        }

        if mtch != 0 {
            set_field(w, target, i - offset);
            return if original != i - offset { 2 } else { 1 };
        }
        i += 1;
    }
    3
}

/// croner `CronDate.recurse` — every recursive call in the original is in tail
/// position, so this is the loop-ified transcription. Returns false where the
/// JS returns null (no occurrence within the year-3000/-10000 search bound).
fn recurse(w: &mut Wall, pat: &Pattern) -> bool {
    let mut doing: usize = 0;
    loop {
        // Year-constraint fast-forward (7-field patterns only).
        if doing == 0 && !pat.star_year {
            if w.year >= 0 && (w.year as usize) < pat.year.len() && pat.year[w.year as usize] == 0 {
                let mut found_year = -1i64;
                let mut y = w.year + 1;
                while (y as usize) < pat.year.len() && y < 10_000 {
                    if pat.year[y as usize] == 1 {
                        found_year = y;
                        break;
                    }
                    y += 1;
                }
                if found_year == -1 {
                    return false;
                }
                w.year = found_year;
                w.month = 0;
                w.day = 1;
                w.hour = 0;
                w.minute = 0;
                w.second = 0;
                w.ms = 0;
            }
            if w.year >= 10_000 {
                return false;
            }
        }

        let (target, offset) = STEPS[doing];
        let res = find_next(w, target, pat, offset);

        if res > 1 {
            // Reset all finer-grained levels.
            for step in STEPS.iter().skip(doing + 1) {
                set_field(w, step.0, -step.1);
            }
            if res == 3 {
                // No match at this level: bump the parent, reset this level,
                // renormalize, and restart from the month level.
                inc_parent(w, doing);
                set_field(w, target, -offset);
                apply(w);
                if doing == 0 && !pat.star_year {
                    while w.year >= 0
                        && (w.year as usize) < pat.year.len()
                        && pat.year[w.year as usize] == 0
                        && w.year < 10_000
                    {
                        w.year += 1;
                    }
                    if w.year >= 10_000 || (w.year as usize) >= pat.year.len() {
                        return false;
                    }
                }
                doing = 0;
                continue;
            } else if apply(w) {
                // The advanced value renormalized (e.g. Feb 30 → March):
                // re-verify one level up. Unreachable at the month level (the
                // finer fields were just reset to in-range values), where the
                // JS would index RecursionSteps[-1] and throw — mirror the
                // caught-throw as "no occurrence".
                if doing == 0 {
                    return false;
                }
                doing -= 1;
                continue;
            }
        }

        doing += 1;
        if doing >= STEPS.len() {
            return true;
        }
        let bound = if pat.star_year { 3_000 } else { 10_000 };
        if w.year >= bound {
            return false;
        }
    }
}

// ---------------------------------------------------------------------------
// Wall ↔ instant bridges (V8 local-Date semantics via jiff Compatible)
// ---------------------------------------------------------------------------

/// `new Date(y, m, d, h, min, s, ms)` in the target zone: jiff's Compatible
/// disambiguation is exactly the ES `LocalTZA` rule (gap → pre-transition
/// offset, fold → earlier occurrence).
fn wall_to_instant_ms(tz: &TimeZone, w: &Wall) -> Option<i64> {
    let dt = civil::DateTime::new(
        i16::try_from(w.year).ok()?,
        i8::try_from(w.month + 1).ok()?,
        i8::try_from(w.day).ok()?,
        i8::try_from(w.hour).ok()?,
        i8::try_from(w.minute).ok()?,
        i8::try_from(w.second).ok()?,
        0,
    )
    .ok()?;
    let ts = tz.to_ambiguous_timestamp(dt).compatible().ok()?;
    Some(ts.as_millisecond() + w.ms)
}

/// The anchor's wall-clock fields (JS local getters on the input `Date`).
fn instant_to_wall(tz: &TimeZone, ms: i64) -> Option<Wall> {
    let ts = jiff::Timestamp::from_millisecond(ms).ok()?;
    let dt = tz.to_datetime(ts);
    Some(Wall {
        ms: (dt.subsec_nanosecond() as i64) / 1_000_000,
        second: dt.second() as i64,
        minute: dt.minute() as i64,
        hour: dt.hour() as i64,
        day: dt.day() as i64,
        month: dt.month() as i64 - 1,
        year: dt.year() as i64,
    })
}

/// croner `CronDate.getTime()` round-trip of an existing instant: project to
/// wall fields, then rebuild through the local constructor. During a fall-back
/// fold the rebuild picks the earlier occurrence, so an instant in the
/// repeated hour's second pass shifts an hour earlier — a real croner
/// behavior the corpus pins.
fn reconstruct_through_wall(tz: &TimeZone, ms: i64) -> Option<i64> {
    let w = instant_to_wall(tz, ms)?;
    let whole = Wall { ms: 0, ..w };
    Some(wall_to_instant_ms(tz, &whole)? + w.ms)
}

/// The pattern branch of `nextRun`: seed from the anchor's local fields,
/// `second += 1; ms = 0`, then search.
fn next_from_pattern(pat: &Pattern, anchor_ms: i64, tz: &TimeZone) -> Option<i64> {
    let mut w = instant_to_wall(tz, anchor_ms)?;
    w.second += 1;
    w.ms = 0;
    apply(&mut w);
    if !recurse(&mut w, pat) {
        return None;
    }
    wall_to_instant_ms(tz, &w)
}

// ---------------------------------------------------------------------------
// The one-off datetime branch (`new Cron("2026-12-25T09:00:00")`)
// ---------------------------------------------------------------------------

struct IsoParts {
    y: i64,
    mo: i64,
    d: i64,
    h: i64,
    mi: i64,
    s: i64,
    /// Minutes east of UTC; `Some(0)` for `Z`. `None` = no offset (local).
    offset_min: Option<i64>,
}

/// The strict ISO-8601 subset of `Date.parse` that croner's one-off branch
/// consumes: `YYYY-MM-DDTHH:MM[:SS[.f{1,9}]][Z|±HH:MM]`. Fractional seconds
/// are consumed but dropped (croner truncates the one-off to whole seconds).
/// V8 validates components *syntactically* (day 01-31 in any month, hour
/// 00-23 or exactly `24:00[:00[.000]]`) and normalizes calendar overflow
/// (`2026-02-30` → Mar 2 — probed on Node 24, banked in the corpus). V8's
/// non-standard fallback formats are a documented seam (→ `None`).
fn parse_iso_local(s: &str) -> Option<IsoParts> {
    let b = s.as_bytes();
    let digits = |from: usize, n: usize| -> Option<i64> {
        if from + n > b.len() {
            return None;
        }
        let mut v = 0i64;
        for &c in &b[from..from + n] {
            if !c.is_ascii_digit() {
                return None;
            }
            v = v * 10 + (c - b'0') as i64;
        }
        Some(v)
    };
    let lit = |at: usize, c: u8| -> bool { b.get(at) == Some(&c) };

    let y = digits(0, 4)?;
    if !lit(4, b'-') {
        return None;
    }
    let mo = digits(5, 2)?;
    if !lit(7, b'-') {
        return None;
    }
    let d = digits(8, 2)?;
    if !lit(10, b'T') {
        return None;
    }
    let h = digits(11, 2)?;
    if !lit(13, b':') {
        return None;
    }
    let mi = digits(14, 2)?;
    let mut pos = 16usize;
    let mut sec = 0i64;
    let mut frac_zero = true;
    if lit(pos, b':') {
        sec = digits(pos + 1, 2)?;
        pos += 3;
        if lit(pos, b'.') {
            let mut n = 0usize;
            while n < 9 && pos + 1 + n < b.len() && b[pos + 1 + n].is_ascii_digit() {
                if b[pos + 1 + n] != b'0' {
                    frac_zero = false;
                }
                n += 1;
            }
            if n == 0 {
                return None;
            }
            pos += 1 + n;
        }
    }
    let offset_min = if pos == b.len() {
        None
    } else if lit(pos, b'Z') && pos + 1 == b.len() {
        Some(0)
    } else if lit(pos, b'+') || lit(pos, b'-') {
        let sign = if b[pos] == b'-' { -1 } else { 1 };
        let oh = digits(pos + 1, 2)?;
        if !lit(pos + 3, b':') {
            return None;
        }
        let om = digits(pos + 4, 2)?;
        if pos + 6 != b.len() || oh > 23 || om > 59 {
            return None;
        }
        Some(sign * (oh * 60 + om))
    } else {
        return None;
    };
    // V8's syntactic ranges: day 01-31 (calendar overflow normalizes later),
    // hour 24 only as exactly 24:00[:00[.000…]].
    let hour_ok = h <= 23 || (h == 24 && mi == 0 && sec == 0 && frac_zero);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || !hour_ok || mi > 59 || sec > 59 {
        return None;
    }
    Some(IsoParts {
        y,
        mo,
        d,
        h,
        mi,
        s: sec,
        offset_min,
    })
}

fn next_one_off(expr: &str, anchor_ms: i64, tz: &TimeZone) -> Option<i64> {
    let p = parse_iso_local(expr)?;
    // Resolve the string to an instant (seconds precision — croner's
    // TimePoint drops fractional seconds).
    let once0_ms = match p.offset_min {
        Some(off) => {
            (days_from_civil(p.y, p.mo, p.d) * 86_400 + p.h * 3_600 + p.mi * 60 + p.s - off * 60)
                * 1_000
        }
        None => {
            // Normalize calendar overflow (Feb 30 → Mar 2, 24:00 → next day)
            // the way V8's MakeDay/MakeTime do, then map the normalized wall
            // time through the local constructor.
            let total_secs =
                days_from_civil(p.y, p.mo, p.d) * 86_400 + p.h * 3_600 + p.mi * 60 + p.s;
            let day_num = total_secs.div_euclid(86_400);
            let mut rem = total_secs.rem_euclid(86_400);
            let (ny, nmo, nd) = civil_from_days(day_num);
            let h = rem / 3_600;
            rem %= 3_600;
            let dt = civil::DateTime::new(
                i16::try_from(ny).ok()?,
                nmo as i8,
                nd as i8,
                h as i8,
                (rem / 60) as i8,
                (rem % 60) as i8,
                0,
            )
            .ok()?;
            tz.to_ambiguous_timestamp(dt)
                .compatible()
                .ok()?
                .as_millisecond()
        }
    };
    // croner routes the resolved instant back through the local wall fields
    // (TimePoint → fromDate → getTime), which shifts a fold's second
    // occurrence to the earlier one.
    let once_ms = reconstruct_through_wall(tz, once0_ms)?;
    let anchor_recon = reconstruct_through_wall(tz, anchor_ms)?;
    // Strictly after: `once.getTime() <= previousRun.getTime()` → null.
    if once_ms <= anchor_recon {
        None
    } else {
        Some(once_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{iso_from_unix_ms, iso_to_ms};

    const CHI: &str = "America/Chicago";
    const KOL: &str = "Asia/Kolkata";

    fn next_iso(expr: &str, anchor_iso: &str, tz: &str) -> Option<String> {
        let anchor = iso_to_ms(anchor_iso).expect("test anchor ISO");
        next_occurrence(expr, anchor, tz).map(iso_from_unix_ms)
    }

    // Pinned values below were verified against v4's real croner 10.0.1
    // (TZ-pinned Node probe, 2026-07-08); the full corpus lives in the
    // enclave_cron_equivalence differential.

    #[test]
    fn plain_next_minute_and_strictly_after() {
        assert_eq!(
            next_iso("* * * * *", "2026-06-15T12:00:00.000Z", CHI).as_deref(),
            Some("2026-06-15T12:01:00.000Z")
        );
        // Anchor exactly on a slot is skipped (second-granularity bump).
        assert_eq!(
            next_iso("0 12 * * *", "2026-06-15T17:00:00.000Z", CHI).as_deref(),
            Some("2026-06-16T17:00:00.000Z")
        );
        assert_eq!(
            next_iso("0 12 * * *", "2026-06-15T16:59:59.999Z", CHI).as_deref(),
            Some("2026-06-15T17:00:00.000Z")
        );
    }

    #[test]
    fn dst_gap_maps_to_post_transition_wall_time() {
        // 2026-03-08 02:30 CT does not exist; V8 maps it with the
        // pre-transition offset → 03:30 CDT.
        assert_eq!(
            next_iso("30 2 * * *", "2026-03-08T07:00:00.000Z", CHI).as_deref(),
            Some("2026-03-08T08:30:00.000Z")
        );
    }

    #[test]
    fn dst_fold_picks_earlier_occurrence_even_behind_anchor() {
        // Anchor 07:00Z = 01:00 CST (between the two 01:30s); croner returns
        // the EARLIER 01:30 CDT = 06:30Z — before the anchor. A pinned quirk.
        assert_eq!(
            next_iso("30 1 * * *", "2026-11-01T07:00:00.000Z", CHI).as_deref(),
            Some("2026-11-01T06:30:00.000Z")
        );
    }

    #[test]
    fn question_mark_dom_is_not_star_dom() {
        // `?` in dom substitutes to `*` AFTER the starDOM check, so dom ORs
        // with dow → every day; a literal `*` ANDs → Fridays only.
        assert_eq!(
            next_iso("0 0 ? * 5", "2026-06-15T12:00:00.000Z", CHI).as_deref(),
            Some("2026-06-16T05:00:00.000Z")
        );
        assert_eq!(
            next_iso("0 0 * * 5", "2026-06-15T12:00:00.000Z", CHI).as_deref(),
            Some("2026-06-19T05:00:00.000Z")
        );
    }

    #[test]
    fn dom_dow_union_and_plus_modifier() {
        // Union: fires Fri Jun 12 (dow) and Sat Jun 13 (dom).
        assert_eq!(
            next_iso("0 0 13 * 5", "2026-06-10T12:00:00.000Z", CHI).as_deref(),
            Some("2026-06-12T05:00:00.000Z")
        );
        assert_eq!(
            next_iso("0 0 13 * 5", "2026-06-12T12:00:00.000Z", CHI).as_deref(),
            Some("2026-06-13T05:00:00.000Z")
        );
        // `+5` forces AND → Friday the 13th (Nov 2026).
        assert_eq!(
            next_iso("0 0 13 * +5", "2026-06-15T12:00:00.000Z", CHI).as_deref(),
            Some("2026-11-13T06:00:00.000Z")
        );
    }

    #[test]
    fn leap_feb_29_and_never_matching_feb_30() {
        assert_eq!(
            next_iso("0 0 29 2 *", "2026-06-15T12:00:00.000Z", CHI).as_deref(),
            Some("2028-02-29T06:00:00.000Z")
        );
        assert_eq!(
            next_iso("0 0 30 2 *", "2026-06-15T12:00:00.000Z", CHI),
            None
        );
    }

    #[test]
    fn nicknames_and_reboot() {
        assert_eq!(
            next_iso("@DAILY", "2026-06-15T12:00:00.000Z", CHI).as_deref(),
            Some("2026-06-16T05:00:00.000Z")
        );
        assert_eq!(next_iso("@reboot", "2026-06-15T12:00:00.000Z", CHI), None);
        assert_eq!(next_iso("@every5", "2026-06-15T12:00:00.000Z", CHI), None);
    }

    #[test]
    fn sequential_alpha_replace_quirk() {
        // JANFEB → "1FEB" → "12" → December.
        assert_eq!(
            next_iso("0 0 1 JANFEB *", "2026-06-15T12:00:00.000Z", CHI).as_deref(),
            Some("2026-12-01T06:00:00.000Z")
        );
    }

    #[test]
    fn one_off_datetimes() {
        assert_eq!(
            next_iso("2026-12-25T09:00:00", "2026-06-15T00:00:00.000Z", CHI).as_deref(),
            Some("2026-12-25T15:00:00.000Z")
        );
        assert_eq!(
            next_iso("2026-12-25T09:00:00", "2027-01-01T00:00:00.000Z", CHI),
            None
        );
        // Fractional seconds are truncated before the strictly-after compare.
        assert_eq!(
            next_iso("2026-06-15T10:30:00.500", "2026-06-15T15:30:00.200Z", CHI),
            None
        );
        // A Z-instant in the fold's second occurrence reconstructs an hour
        // earlier through the wall round-trip.
        assert_eq!(
            next_iso("2026-11-01T07:30:00Z", "2026-10-01T00:00:00.000Z", CHI).as_deref(),
            Some("2026-11-01T06:30:00.000Z")
        );
        // V8 normalizes syntactically-valid day overflow (Feb 30 → Mar 2) and
        // accepts hour 24 as exactly 24:00 — both probed against real croner.
        assert_eq!(
            next_iso("2026-02-30T10:00:00", "2026-01-01T00:00:00.000Z", CHI).as_deref(),
            Some("2026-03-02T16:00:00.000Z")
        );
        assert_eq!(
            next_iso("2026-06-15T24:00:00", "2026-06-01T00:00:00.000Z", CHI).as_deref(),
            Some("2026-06-16T05:00:00.000Z")
        );
        assert_eq!(
            next_iso("2026-06-15T24:30:00", "2026-06-01T00:00:00.000Z", CHI),
            None
        );
        assert_eq!(next_iso("12:34", "2026-06-15T00:00:00.000Z", CHI), None);
        assert_eq!(next_iso("foo:bar", "2026-06-15T00:00:00.000Z", CHI), None);
        // ":" at index 0 routes to the pattern parser (illegal char → None).
        assert_eq!(
            next_iso(":30 * * * *", "2026-06-15T00:00:00.000Z", CHI),
            None
        );
    }

    #[test]
    fn invalid_patterns_collapse_to_none() {
        for expr in [
            "",
            "garbage",
            "* * * *",
            "* * * * * * * *",
            "60 * * * *",
            "0 24 * * *",
            "0 0 32 * *",
            "0 0 0 * *",
            "0 0 1 13 *",
            "0 0 * * 8",
            "-5 * * * *",
            "5-1 * * * *",
            "*/0 * * * *",
            "*/70 * * * *",
            "0/10 * * * *",
            "/10 * * * *",
        ] {
            assert_eq!(
                next_iso(expr, "2026-06-15T12:00:00.000Z", CHI),
                None,
                "expected None for {expr:?}"
            );
        }
    }

    #[test]
    fn fractional_offset_zone() {
        // 09:30 IST = 04:00Z.
        assert_eq!(
            next_iso("30 9 * * *", "2026-06-15T00:00:00.000Z", KOL).as_deref(),
            Some("2026-06-15T04:00:00.000Z")
        );
    }

    #[test]
    fn unknown_timezone_is_none() {
        assert_eq!(
            next_iso("* * * * *", "2026-06-15T12:00:00.000Z", "Not/AZone"),
            None
        );
    }
}
