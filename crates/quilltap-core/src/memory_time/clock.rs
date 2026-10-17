//! Port of v4 `lib/memory/time/clock.ts` (`e0ac7493a`) — the memory clock:
//! event time on write, reference windows on read.
//!
//! v4's why, carried: both directions resolve against one [`MemoryClock`] —
//! the instance's memory calendar (instance timezone → `QUILLTAP_TIMEZONE` →
//! system; the per-chat display timezone is never consulted) — and through
//! one rule table ([`super::rules`]). [`resolve_event_time`] stamps a phrase
//! at the START of the window the table gives it; [`resolve_reference_window`]
//! returns that whole window. A handful of phrases the read scan leaves to
//! the LLM ("a month ago", "last Tuesday", named dates) have write-only rules
//! here, since an extractor's `when` must still land somewhere.
//!
//! Windows are closed on both ends as ISO strings: `to` is the last instant
//! inside (one millisecond before the next boundary), so an `occurredAt` at a
//! day's midnight belongs to that day only and never to the day before.
//!
//! v5 shape: pure, and the zone is ALWAYS an argument (work order P4.D289
//! LR-12's principle) — v4's defaulted `timeZone = getMemoryTimeZone()`
//! parameters are required here; the host resolves the zone first
//! ([`super::calendar::memory_time_zone`] /
//! [`crate::services::memory_time_zone::refresh_memory_time_zone`]).
//!
//! `Date.parse` is [`crate::episodic::js_date_parse_ms`], which reads a
//! zone-less datetime (`2026-07-14T09:30`) as UTC where v4 reads it in the
//! PROCESS zone — the ruled divergence `ZONELESS_PARSE_AS_UTC` (LR-10),
//! pinned both ways by the `memory_time` family's `TZ=America/Chicago` leg.

use std::sync::LazyLock;

use regex::Regex;

use super::calendar::{
    js_iso_string, start_of_zoned_day_ms, zoned_day_bounds_iso, zoned_parts, zoned_time_to_utc_ms,
};
use super::rules::{match_day_reference_rule, RuleWindow, DAY_REFERENCE_SCAN_PATTERN};
use super::TimeWindow;
use crate::episodic::js_date_parse_ms;
use crate::jsstr::{js_trim, utf16_len, JS_WS_CLASS};

/// The calendar a memory-time question is answered on, and the instant it is
/// asked at (v4 `MemoryClock`).
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryClock {
    pub time_zone: String,
    pub now_ms: f64,
    /// `narrative` chats skip the read-side scan (story time is not the wall
    /// calendar). v4 `chat?.timelineMode ?? 'realtime'` — whatever string the
    /// chat carries passes through, so this is a `String`, not a closed enum.
    pub timeline_mode: String,
}

/// What a `when` phrase resolved to on the write side (v4 `resolveEventTime`'s
/// `{ occurredAt, window }`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EventTime {
    pub occurred_at: Option<String>,
    pub window: Option<TimeWindow>,
}

/// What a phrase in conversation resolved to on the read side (v4
/// `DayReferenceResolution`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayReferenceResolution {
    /// The window the phrase names. Only meaningful when `past_pointing` is
    /// true: callers must not apply the window of a future reference.
    pub time_range: TimeWindow,
    /// True for past references (today/yesterday/…), false for future
    /// ("tomorrow").
    pub past_pointing: bool,
    /// Which phrase matched (lower-cased, whitespace-collapsed), for logs and
    /// the replay output.
    pub matched: String,
}

/// The clock for a chat (v4 `clockFor(chat, instance, nowMs)`), given the
/// memory-calendar zone ALREADY resolved: v4's `getMemoryTimeZone(
/// instance.timeZone ?? null)` is the caller's
/// [`super::calendar::memory_time_zone`] (`instance_tz`, `host_fallback`) —
/// core never reads an ambient zone. `timeline_mode` is the chat's
/// `timelineMode` (absent → `"realtime"`).
pub fn clock_for(timeline_mode: Option<&str>, memory_tz: &str, now_ms: f64) -> MemoryClock {
    MemoryClock {
        time_zone: memory_tz.to_string(),
        now_ms,
        timeline_mode: timeline_mode.unwrap_or("realtime").to_string(),
    }
}

/// v4 `toWindow`: `{ from: ISO(fromMs), to: ISO(endMs − 1) }` — every window
/// closes one millisecond before the next boundary. `None` only where v4's
/// `toISOString()` would throw (an Invalid Date — outside every caller's
/// contract: a finite anchor on a resolved zone never produces one).
fn to_window(w: &RuleWindow) -> Option<TimeWindow> {
    Some(TimeWindow {
        from: js_iso_string(w.from_ms)?,
        to: js_iso_string(w.end_ms - 1.0)?,
    })
}

/// JS `s.replace(/\s+/g, ' ')`.
fn collapse_ws(s: &str) -> String {
    static WS_RUN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(&format!("{JS_WS_CLASS}+")).expect("ws class compiles"));
    WS_RUN.replace_all(s, " ").into_owned()
}

// ---------------------------------------------------------------------------
// Write side
// ---------------------------------------------------------------------------

/// v4 `MONTH_NAMES` (january … december → 0–11). Before v4 `25b8fab6a` v4
/// indexed a plain object, so `MONTH_NAMES['constructor']` was `Object` (not
/// `undefined`) and a phrase like `constructor 5` reached `zonedTimeToUtcMs`
/// with a function for a month and THREW (`RangeError: Invalid time value`
/// from `formatToParts`). This table is a closed list: v5 answers such a
/// phrase like any other unknown word — and v4 `25b8fab6a` converged (bug 247:
/// its word tables are read through `Object.hasOwn`), so the `memory_time`
/// family now compares those phrases as plain equality rows.
fn month_index(name: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    MONTHS.iter().position(|m| *m == name).map(|i| i as i64)
}

/// v4 `UNIT_DAYS`.
fn unit_days(unit: &str) -> Option<i64> {
    Some(match unit {
        "day" | "days" => 1,
        "week" | "weeks" => 7,
        "month" | "months" => 30,
        "year" | "years" => 365,
        _ => return None,
    })
}

/// v4 `WORD_NUMBERS` (`a`/`an` 1, one … fourteen, `couple` 2, `few` 3). v4's
/// plain-object lookup answered `Object` for `constructor`, which then failed
/// `n > 0` (NaN) exactly as `None` does here; v4 `25b8fab6a` converged (bug
/// 247 — an own-key lookup now answers `undefined`), an unobservable change on
/// this arm.
fn word_number(word: &str) -> Option<f64> {
    Some(match word {
        "a" | "an" | "one" => 1.0,
        "two" | "couple" => 2.0,
        "three" | "few" => 3.0,
        "four" => 4.0,
        "five" => 5.0,
        "six" => 6.0,
        "seven" => 7.0,
        "eight" => 8.0,
        "nine" => 9.0,
        "ten" => 10.0,
        "eleven" => 11.0,
        "twelve" => 12.0,
        "thirteen" => 13.0,
        "fourteen" => 14.0,
        _ => return None,
    })
}

/// v4 `WEEKDAYS` (0 = Sunday).
const WEEKDAYS: [&str; 7] = [
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];

macro_rules! ws_re {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static RE: LazyLock<Regex> = LazyLock::new(|| {
                Regex::new(&$pat.replace("WS", JS_WS_CLASS)).expect("when-phrase regex compiles")
            });
            &RE
        }
    };
}

// The write-side regexes, byte-faithful to v4's: they run on the ALREADY
// lower-cased phrase with no `i` flag (so they are case-sensitive here too);
// JS `\s` → the JS whitespace class; JS `\d` / `[a-z]` are ASCII.
ws_re!(the_prefix_re, r"^theWS+");
ws_re!(
    iso_re,
    r"^([0-9]{4})-([0-9]{2})-([0-9]{2})(t[0-9:.]+(z|[+-][0-9]{2}:?[0-9]{2})?)?$"
);
ws_re!(
    month_first_re,
    r"^(?:onWS+)?([a-z]+)WS+([0-9]{1,2})(?:st|nd|rd|th)?(?:,?WS+([0-9]{4}))?$"
);
ws_re!(
    day_first_re,
    r"^(?:onWS+)?([0-9]{1,2})(?:st|nd|rd|th)?WS+(?:ofWS+)?([a-z]+)(?:,?WS+([0-9]{4}))?$"
);
ws_re!(anchor_moment_re, r"^(earlier|just now|now|this night)$");
// v4: `^(?:about\s+|around\s+|some\s+)?(?:a\s+(?=couple|few))?([a-z]+|\d+)\s+
// (?:of\s+)?(days?|weeks?|months?|years?)\s+(?:ago|back|earlier|before)$`.
// Rust's regex has no lookahead; the optional `a\s+(?=couple|few)` followed
// by `([a-z]+|\d+)` is rewritten as the alternation `a\s+((?:couple|few)
// [a-z]*)` | `([a-z]+|\d+)`, tried in that order (v4 tries the optional group
// first). Equivalent: after the lookahead the `[a-z]+` branch must run to the
// next whitespace, i.e. a letter run starting `couple` / `few`; and when the
// lookahead holds, the group-skipped path cannot succeed (`a` would have to be
// followed by `of ` or a unit, and `couple` / `few` are neither).
ws_re!(
    ago_re,
    r"^(?:aboutWS+|aroundWS+|someWS+)?(?:aWS+((?:couple|few)[a-z]*)|([a-z]+|[0-9]+))WS+(?:ofWS+)?(days?|weeks?|months?|years?)WS+(?:ago|back|earlier|before)$"
);
ws_re!(
    weekday_re,
    r"^lastWS+(sunday|monday|tuesday|wednesday|thursday|friday|saturday)$"
);
ws_re!(season_re, r"^lastWS+(spring|summer|fall|autumn|winter)$");

/// Resolve a model's `when` phrase into an `occurredAt`, anchored at
/// `anchor_ms` (the source message's time) on `time_zone`'s calendar (v4
/// `resolveEventTime(phrase, anchorMs, { timeZone })`).
///
///  - A phrase the day-reference table knows is stamped at the START of its
///    window, and the window is returned too ("last night" → 18:00 the day
///    before; "last week" → the previous Monday; "today" → local midnight).
///  - Write-only rules: ISO datetimes as given; ISO dates and named dates
///    ("July 14", "14 July 2026") at local midnight, a yearless date in the
///    anchor's future rolled back a year; "N days/weeks/months/years ago",
///    "a couple of days back"; "last month/year/<weekday>/<season>"; "the
///    other day"; "earlier", "just now", "now" (the anchor itself).
///  - Anything else, future references included, resolves to nothing —
///    recall degrades, never blocks.
pub fn resolve_event_time(phrase: Option<&str>, anchor_ms: f64, time_zone: &str) -> EventTime {
    resolve_event_time_inner(phrase, anchor_ms, time_zone).unwrap_or_default()
}

fn resolve_event_time_inner(phrase: Option<&str>, anchor_ms: f64, tz: &str) -> Option<EventTime> {
    let raw = js_trim(phrase?);
    if raw.is_empty() || !anchor_ms.is_finite() {
        return None;
    }
    let lower = collapse_ws(&raw.to_lowercase());
    let at = |occurred_ms: f64| -> Option<EventTime> {
        Some(EventTime {
            occurred_at: Some(js_iso_string(occurred_ms)?),
            window: None,
        })
    };
    let days_back = |days: i64| at(start_of_zoned_day_ms(anchor_ms, tz, -days));

    // The table: the start of the window the read side asks for.
    let stripped = the_prefix_re().replace(&lower, "");
    if let Some((rule, m)) = match_day_reference_rule(&stripped) {
        if let Some(w) = rule.window(&m, anchor_ms, tz) {
            if w.past_pointing {
                return Some(EventTime {
                    occurred_at: Some(js_iso_string(w.from_ms)?),
                    window: Some(to_window(&w)?),
                });
            }
            // A future reference is not an event time.
            return None;
        }
        // A rule that matched but resolved nothing ("97 days ago") falls
        // through to the write-only rules below.
    }

    // Absolute ISO datetime / date.
    if let Some(m) = iso_re().captures(&lower) {
        let parsed = js_date_parse_ms(raw)?;
        if m.get(4).is_some() {
            return at(parsed as f64);
        }
        let bounds = zoned_day_bounds_iso(&lower[..10], tz)?;
        return Some(EventTime {
            occurred_at: Some(bounds.from.clone()),
            window: Some(bounds),
        });
    }

    // "July 14, 2026" / "July 14" / "14 July 2026" / "14 July"
    let named = month_first_re()
        .captures(&lower)
        .and_then(|m| {
            let month = month_index(&m[1])?;
            Some((
                month,
                m[2].to_string(),
                m.get(3).map(|y| y.as_str().to_string()),
            ))
        })
        .or_else(|| {
            day_first_re().captures(&lower).and_then(|m| {
                let month = month_index(&m[2])?;
                Some((
                    month,
                    m[1].to_string(),
                    m.get(3).map(|y| y.as_str().to_string()),
                ))
            })
        });
    if let Some((month, day_text, year_text)) = named {
        // `parseInt` over `\d{1,2}` / `\d{4}`: always a plain decimal. v4's
        // `monthFirst[3] ? parseInt(...) : null` — a matched `"0000"` is a
        // truthy string, so year 0 is GIVEN (and `Date.UTC`'s two-digit
        // window makes it 1900, as v4's does).
        let day: i64 = day_text.parse().ok()?;
        let given_year: Option<i64> = match year_text {
            Some(y) => Some(y.parse().ok()?),
            None => None,
        };
        if (1..=31).contains(&day) {
            let mut year = given_year.unwrap_or_else(|| zoned_parts(anchor_ms, tz).year);
            let mut candidate = zoned_time_to_utc_ms(year, month, day, 0, tz);
            // A yearless date after the anchor is last year's (retold events
            // are past).
            if given_year.is_none() && candidate > anchor_ms {
                year -= 1;
                candidate = zoned_time_to_utc_ms(year, month, day, 0, tz);
            }
            return at(candidate);
        }
    }

    // The anchor moment itself.
    if anchor_moment_re().is_match(&lower) {
        return at(anchor_ms);
    }

    // "N days ago", "a week ago", "about 3 months back", "a couple of days ago"
    if let Some(m) = ago_re().captures(&lower) {
        let word = m
            .get(1)
            .or_else(|| m.get(2))
            .map(|g| g.as_str())
            .unwrap_or("");
        // v4: digits → `parseInt(_, 10)` (only compared, so f64 is exact
        // enough), else `WORD_NUMBERS[word]`.
        let n = if !word.is_empty() && word.bytes().all(|b| b.is_ascii_digit()) {
            word.parse::<f64>().ok()
        } else {
            word_number(word)
        };
        let unit = unit_days(&m[3]);
        if let (Some(n), Some(unit)) = (n, unit) {
            if n > 0.0 && n < 10000.0 {
                return days_back(n as i64 * unit);
            }
        }
    }

    match lower.as_str() {
        "last month" => return days_back(30),
        "last year" => return days_back(365),
        "the other day" => return days_back(2),
        _ => {}
    }

    // "last <weekday>": the most recent strictly-past occurrence.
    if let Some(m) = weekday_re().captures(&lower) {
        let target = WEEKDAYS.iter().position(|d| *d == &m[1])? as i64;
        let anchor_day = zoned_parts(anchor_ms, tz).weekday;
        let back = (anchor_day - target + 7) % 7;
        return days_back(if back == 0 { 7 } else { back });
    }

    // Coarse: "last <season>" is about half a year back — enough for
    // windowing.
    if season_re().is_match(&lower) {
        return days_back(182);
    }

    None
}

/// The string form of [`resolve_event_time`] (v4 `resolveWhenPhrase(phrase,
/// anchorIso, timeZone)`): a `when` phrase, an ISO anchor (`Date.parse`d — an
/// unparseable anchor resolves nothing), the zone. Returns `occurredAt`.
pub fn resolve_when_phrase(
    phrase: Option<&str>,
    anchor_iso: &str,
    time_zone: &str,
) -> Option<String> {
    let anchor_ms = js_date_parse_ms(anchor_iso)
        .map(|ms| ms as f64)
        .unwrap_or(f64::NAN);
    resolve_event_time(phrase, anchor_ms, time_zone).occurred_at
}

// ---------------------------------------------------------------------------
// Read side
// ---------------------------------------------------------------------------

/// Scan conversation text for a day reference and resolve it to its window at
/// `now_ms` (v4 `resolveReferenceWindow(text, { timeZone, nowMs })`). The
/// latest match in the text wins (the most recent utterance is the turn's
/// focus); an unresolvable match ("97 days ago") does not displace an earlier
/// one. `None` when nothing is found. Narrative clocks are the caller's to
/// skip — story time is not the wall calendar.
pub fn resolve_reference_window(
    text: &str,
    time_zone: &str,
    now_ms: f64,
) -> Option<DayReferenceResolution> {
    if text.is_empty() || !now_ms.is_finite() {
        return None;
    }
    let mut latest: Option<DayReferenceResolution> = None;
    // v4 re-creates the `/g` regex per call (no leaked `lastIndex`) and bumps
    // `lastIndex` past a zero-length match; `find_iter` is stateless and
    // advances past an empty match itself (none of the table's alternatives
    // can match empty anyway).
    for m in DAY_REFERENCE_SCAN_PATTERN.find_iter(text) {
        let phrase = collapse_ws(&m.as_str().to_lowercase());
        let w = match_day_reference_rule(&phrase)
            .and_then(|(rule, hit)| rule.window(&hit, now_ms, time_zone));
        if let Some(w) = w {
            if let Some(time_range) = to_window(&w) {
                latest = Some(DayReferenceResolution {
                    time_range,
                    past_pointing: w.past_pointing,
                    matched: phrase,
                });
            }
        }
    }
    latest
}

/// The `(text, now, zone)` form of [`resolve_reference_window`] (v4
/// `resolveDayReference(text, now: Date, timeZone)` — `now.getTime()` is
/// `now_ms`; an Invalid Date is NaN).
pub fn resolve_day_reference(
    text: &str,
    now_ms: f64,
    time_zone: &str,
) -> Option<DayReferenceResolution> {
    resolve_reference_window(text, time_zone, now_ms)
}

/// Normalise a model's date-only bound to the local day it names on the
/// memory calendar (`from` → local midnight, `to` → the day's last instant)
/// (v4 `normaliseTimeRange({ from, to }, { timeZone })`). Full ISO datetimes
/// pass through (trimmed). `None` when either bound is unusable or
/// `from > to`.
pub fn normalise_time_range(
    from: Option<&str>,
    to: Option<&str>,
    time_zone: &str,
) -> Option<TimeWindow> {
    static DATE_PREFIX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}").expect("date prefix compiles"));
    let from = js_trim(from?);
    let to = js_trim(to?);
    if from.is_empty() || to.is_empty() {
        return None;
    }
    if !DATE_PREFIX.is_match(from) || !DATE_PREFIX.is_match(to) {
        return None;
    }
    // `.length === 10` counts UTF-16 units (the ten-ASCII prefix makes it the
    // whole string either way).
    let from_iso = if utf16_len(from) == 10 {
        zoned_day_bounds_iso(from, time_zone)?.from
    } else {
        from.to_string()
    };
    let to_iso = if utf16_len(to) == 10 {
        zoned_day_bounds_iso(to, time_zone)?.to
    } else {
        to.to_string()
    };
    let from_ms = js_date_parse_ms(&from_iso)?;
    let to_ms = js_date_parse_ms(&to_iso)?;
    if from_ms > to_ms {
        return None;
    }
    Some(TimeWindow {
        from: from_iso,
        to: to_iso,
    })
}

#[cfg(test)]
mod jest_twins {
    //! Case-for-case port of v4
    //! `lib/memory/time/__tests__/day-references.test.ts` (pin `83334b9a5`).
    //!
    //! v4's suite cannot pin `TZ` and computes every expectation through the
    //! local-time `Date` API in whatever zone the runner sits in; v5's zone is
    //! a parameter, so these run on `America/Chicago` (the diagnosed chat's
    //! zone, west of Greenwich so the local-vs-UTC case is never a no-op) and
    //! v4's `localMidnight(n)` / `localHour(n, h)` helpers become absolute
    //! literals for that zone (CDT, UTC−5, all of July 2026).

    use super::*;

    const CHI: &str = "America/Chicago";

    /// 2026-07-28 21:44 local (CDT) — the wall-clock moment from the
    /// diagnosed chat (v4 `new Date(2026, 6, 28, 21, 44)`).
    const NOW: f64 = 1_785_293_040_000.0;
    const NOW_ISO: &str = "2026-07-29T02:44:00.000Z";

    /// v4 `localMidnight(offset)` on `America/Chicago`, for every offset the
    /// suite uses.
    fn local_midnight(offset: i64) -> &'static str {
        match offset {
            0 => "2026-07-28T05:00:00.000Z",
            -1 => "2026-07-27T05:00:00.000Z",
            -2 => "2026-07-26T05:00:00.000Z",
            -3 => "2026-07-25T05:00:00.000Z",
            -8 => "2026-07-20T05:00:00.000Z",
            -13 => "2026-07-15T05:00:00.000Z",
            -14 => "2026-07-14T05:00:00.000Z",
            other => panic!("no literal for localMidnight({other})"),
        }
    }

    fn parse(iso: &str) -> f64 {
        js_date_parse_ms(iso).unwrap_or_else(|| panic!("Date.parse({iso})")) as f64
    }

    /// v4 `before(iso)`: the last instant before `iso`.
    fn before(iso: &str) -> String {
        js_iso_string(parse(iso) - 1.0).unwrap()
    }

    fn resolve(text: &str) -> Option<DayReferenceResolution> {
        resolve_day_reference(text, NOW, CHI)
    }

    /// `it('resolves a late-evening "today" to the local day')`
    #[test]
    fn resolves_a_late_evening_today_to_the_local_day() {
        let r = resolve("No, no. I mean the mission today. I want to hear about it.")
            .expect("not null");
        assert_eq!(r.matched, "today");
        assert!(r.past_pointing);
        assert_eq!(r.time_range.from, local_midnight(0));
        assert_eq!(r.time_range.to, NOW_ISO);
    }

    /// `it('uses local calendar math, not UTC')` — Chicago sits west of
    /// Greenwich (v4's `NOW.getTimezoneOffset() > 0` arm always runs here).
    #[test]
    fn uses_local_calendar_math_not_utc() {
        let r = resolve("the mission today").unwrap();
        let utc_midnight_of_now = format!("{}T00:00:00.000Z", &NOW_ISO[..10]);
        assert_ne!(r.time_range.from, utc_midnight_of_now);
        assert_eq!(r.time_range.from, local_midnight(0));
    }

    /// `it('covers an event that happened earlier the same local day')`
    #[test]
    fn covers_an_event_that_happened_earlier_the_same_local_day() {
        let r = resolve("the mission today").unwrap();
        let midday_event = parse("2026-07-28T17:00:00.000Z"); // 12:00 CDT on the 28th
        let last_night_event = parse("2026-07-28T04:59:00.000Z"); // 23:59 CDT on the 27th
        assert!(parse(&r.time_range.from) <= midday_event);
        assert!(parse(&r.time_range.to) >= midday_event);
        assert!(parse(&r.time_range.from) > last_night_event);
    }

    /// `it.each(cases)('resolves "$matched" from "$text"')` — all eleven rows.
    #[test]
    fn resolves_matched_from_text() {
        let now = NOW_ISO.to_string();
        let cases: Vec<(&str, &str, String, String)> = vec![
            (
                "earlier today we talked",
                "earlier today",
                local_midnight(0).into(),
                now.clone(),
            ),
            (
                "how did it go this morning?",
                "this morning",
                local_midnight(0).into(),
                now.clone(),
            ),
            (
                "you seemed rattled this afternoon",
                "this afternoon",
                local_midnight(0).into(),
                now.clone(),
            ),
            (
                "that thing this evening",
                "this evening",
                local_midnight(0).into(),
                now.clone(),
            ),
            (
                "what a mess tonight",
                "tonight",
                local_midnight(0).into(),
                now.clone(),
            ),
            (
                "about yesterday",
                "yesterday",
                local_midnight(-1).into(),
                before(local_midnight(0)),
            ),
            (
                "the day before yesterday",
                "day before yesterday",
                local_midnight(-2).into(),
                before(local_midnight(-1)),
            ),
            (
                "we met 3 days ago",
                "3 days ago",
                local_midnight(-3).into(),
                before(local_midnight(-2)),
            ),
            (
                "we met three days ago",
                "three days ago",
                local_midnight(-3).into(),
                before(local_midnight(-2)),
            ),
            (
                "that was 1 day ago",
                "1 day ago",
                local_midnight(-1).into(),
                before(local_midnight(0)),
            ),
            (
                "fourteen days ago exactly",
                "fourteen days ago",
                local_midnight(-14).into(),
                before(local_midnight(-13)),
            ),
        ];
        assert_eq!(cases.len(), 11);
        for (text, matched, from, to) in cases {
            let r = resolve(text).unwrap_or_else(|| panic!("{text:?}: null"));
            assert_eq!(r.matched, matched, "{text:?}");
            assert!(r.past_pointing, "{text:?}");
            assert_eq!(r.time_range, TimeWindow { from, to }, "{text:?}");
        }
    }

    /// `it('spans "last night" from 18:00 the previous local evening to 06:00
    /// this morning')`
    #[test]
    fn spans_last_night_from_18_00_the_previous_local_evening_to_06_00_this_morning() {
        let r = resolve("you were quiet last night").unwrap();
        // localHour(-1, 18) / before(localHour(0, 6)) on CDT.
        assert_eq!(r.time_range.from, "2026-07-27T23:00:00.000Z");
        assert_eq!(r.time_range.to, before("2026-07-28T11:00:00.000Z"));
        assert!(r.past_pointing);
    }

    /// `it('resolves "this week" from the most recent local Monday to now')`
    #[test]
    fn resolves_this_week_from_the_most_recent_local_monday_to_now() {
        // NOW is a Tuesday, so the week anchor is yesterday.
        assert_eq!(zoned_parts(NOW, CHI).weekday, 2);
        let r = resolve("busy this week").unwrap();
        assert_eq!(r.time_range.from, local_midnight(-1));
        assert_eq!(r.time_range.to, NOW_ISO);
    }

    /// `it('resolves "last week" to the previous Monday–Sunday local week')`
    #[test]
    fn resolves_last_week_to_the_previous_monday_sunday_local_week() {
        let r = resolve("what you said last week").unwrap();
        assert_eq!(r.time_range.from, local_midnight(-8));
        assert_eq!(r.time_range.to, before(local_midnight(-1)));
    }

    /// `it('leaves an out-of-range "N days ago" to the LLM')`
    #[test]
    fn leaves_an_out_of_range_n_days_ago_to_the_llm() {
        assert_eq!(resolve("that was 97 days ago"), None);
        assert_eq!(resolve("that was 15 days ago"), None);
    }

    /// `it('marks "tomorrow" as future-pointing')`
    #[test]
    fn marks_tomorrow_as_future_pointing() {
        let r = resolve("let's do it tomorrow").unwrap();
        assert_eq!(r.matched, "tomorrow");
        assert!(!r.past_pointing);
    }

    /// `it('marks "next week" as future-pointing')`
    #[test]
    fn marks_next_week_as_future_pointing() {
        let r = resolve("we can start next week").unwrap();
        assert_eq!(r.matched, "next week");
        assert!(!r.past_pointing);
    }

    /// `it('lets a later future reference suppress an earlier past one')`
    #[test]
    fn lets_a_later_future_reference_suppress_an_earlier_past_one() {
        let r = resolve("we said yesterday that we would start tomorrow").unwrap();
        assert_eq!(r.matched, "tomorrow");
        assert!(!r.past_pointing);
    }

    /// `it('lets the latest match in the text win')`
    #[test]
    fn lets_the_latest_match_in_the_text_win() {
        let r = resolve("we covered that yesterday \u{2014} but today, the mission").unwrap();
        assert_eq!(r.matched, "today");
        assert_eq!(r.time_range.from, local_midnight(0));
    }

    /// `it('does not let an inner phrase beat its own container')`
    #[test]
    fn does_not_let_an_inner_phrase_beat_its_own_container() {
        let r = resolve("the day before yesterday").unwrap();
        assert_eq!(r.matched, "day before yesterday");
    }

    /// `it('is case-insensitive')`
    #[test]
    fn is_case_insensitive() {
        assert_eq!(resolve("YESTERDAY was long").unwrap().matched, "yesterday");
        assert_eq!(resolve("Today is better").unwrap().matched, "today");
    }

    /// `it('respects word boundaries')`
    #[test]
    fn respects_word_boundaries() {
        assert_eq!(resolve("we drove through Yesterdayville"), None);
        assert_eq!(resolve("the Todayer gazette"), None);
    }

    /// `it('returns null when no reference is present')`
    #[test]
    fn returns_null_when_no_reference_is_present() {
        assert_eq!(resolve("how are the soil samples looking"), None);
        assert_eq!(resolve(""), None);
    }

    /// `it('returns null on an invalid clock')` — `new Date(NaN).getTime()`
    /// is NaN.
    #[test]
    fn returns_null_on_an_invalid_clock() {
        assert_eq!(
            resolve_day_reference("the mission today", f64::NAN, CHI),
            None
        );
    }

    /// `it('is stateless across calls (no leaked regex lastIndex)')`
    #[test]
    fn is_stateless_across_calls_no_leaked_regex_last_index() {
        let first = resolve("the mission today");
        let second = resolve("the mission today");
        assert_eq!(second, first);
    }
}
