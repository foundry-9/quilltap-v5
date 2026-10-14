//! Episodic-spine helpers (the episodic recall overhaul, v4 `8bf3cb5f`,
//! `lib/memory/episodic.ts`).
//!
//! **Partly LANE-REMOVE shims (work order P4.D289, §R.10(o)).** In the
//! `83334b9a5` span v4 moved the WRITE half out of this module:
//! `resolveWhenPhrase` to `lib/memory/time/clock.ts` (`e0ac7493a` — the ONE
//! rule table, stamping the window START on the memory calendar; ported as
//! [`crate::memory_time::resolve_when_phrase`]), `buildMemoryEmbeddingText`
//! to `index-store/embedding-text.ts` (`af48f6cc2`; ported as
//! [`crate::memory_embedding_text::embedding_text_for`]), and the anchor
//! line onto the memory calendar (`c2795733f`, bug 212; ported as
//! [`crate::memory_embedding_text::memory_anchor_line`]). Their callers live
//! in other lanes' files (admission P4.D290, recall P4.D291, embeddings
//! P4.D292, extraction P4.D296, maintenance P4.D297), so the three old
//! functions keep their pre-refactor bodies VERBATIM (UTC, the pre stamps)
//! until their owners re-point (LR-1). `EpisodicAnchorView`, the event
//! clocks and `js_date_parse_ms` stay here as v4 keeps them (LR-12).
//!
//! Small, pure utilities shared by the write path (memory-service, memory-gate,
//! extraction) and the read path (injector, tools):
//!
//!  - [`build_memory_anchor_line`] / [`build_memory_embedding_text`]: the
//!    `(when: 2026-07-14 · place: Lighthouse Point)` anchor line appended to a
//!    memory's embedded text so the vector itself carries temporal/place signal.
//!  - [`resolve_when_phrase`]: deterministic server-side resolution of a
//!    model-emitted `when` phrase (absolute or relative) into an ISO
//!    `occurredAt`, anchored to the source turn's timestamp. Its callers (all
//!    landed in round 3): the memory processor's `resolve_candidate_anchors`,
//!    the fold-episode pass, and the create-path fallback anchors.
//!  - [`event_reference_time_ms`]: the event clock (`occurredAt ?? write
//!    clock`) used for age labels — distinct from the decay reference time in
//!    memory-weighting, which stays on the write/reinforce clock.
//!
//! Pure + I/O-free, exactly as v4 keeps it importable from the forked job child.
//!
//! ## JS-fidelity notes
//!
//!  - `Date.parse` is V8's own parser ([`js_date_parse_ms`] →
//!    [`crate::v8_date_parse`]): the ES5 ISO grammar AND the legacy fallback
//!    (`"July 14, 2026"`, `2026/07/14`, …) — the ISO-subset port it replaced
//!    was P4.D289's `JS_DATE_PARSE_ISO_SUBSET` finding. V8's ISO quirks are
//!    reproduced: month must be 01–12 but a day of 01–31 **rolls** past the
//!    month end (`2026-02-31` → Mar 3 — and the date-only when-phrase branch
//!    then returns the RAW `2026-02-31T00:00:00.000Z`, v4's exact behavior);
//!    fraction digits truncate to milliseconds; lowercase `t`/`z` accepted.
//!  - A zone-less datetime parses as **UTC** here. In JS it parses in the
//!    process's LOCAL zone — the oracle is therefore generated with `TZ=UTC`
//!    (see `harness/oracle/cases/episodic.ts`). Production `occurredAt` inputs
//!    always carry a zone, so this seam is corpus-only.
//!  - JS `\s` / `String.trim` are the JS whitespace class ([`crate::jsstr`]).

use std::sync::OnceLock;

use regex::Regex;

use crate::clock::{days_from_civil, floor_div_ms_days, iso_from_unix_ms, utc_parts};
use crate::jsstr::{js_trim, JS_WS_CLASS};

/// Structural view of the episodic fields (keeps this module Memory-import-free —
/// v4 `EpisodicAnchorView`).
#[derive(Debug, Clone, Default)]
pub struct EpisodicAnchorView {
    pub occurred_at: Option<String>,
    pub narrative_time: Option<String>,
    pub entities: Vec<String>,
}

/// Cap on entities rendered into the anchor line so it stays one line.
const ANCHOR_MAX_ENTITIES: usize = 4;

/// Milliseconds per day.
const DAY_MS: i64 = 86_400_000;

fn date_only_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$").unwrap())
}

/// JS `String.prototype.slice(0, 10)` — UTF-16 code units, not chars.
fn js_slice10(s: &str) -> String {
    let units: Vec<u16> = s.encode_utf16().take(10).collect();
    String::from_utf16_lossy(&units)
}

/// Render the anchor line appended to a memory's embedded text, e.g.
/// `(when: 2026-07-14 · place: Lighthouse Point)`. Returns `""` when the memory
/// carries no anchors, so callers can append unconditionally. The date is
/// rendered as the ISO calendar date (time-of-day adds noise, not signal, to
/// the embedding); `narrativeTime` rides along verbatim when present.
// LANE-REMOVE(P4.D290|P4.D291|P4.D292|P4.D296|P4.D297): v4 deleted the UTC-slice buildMemoryAnchorLine in c2795733f
pub fn build_memory_anchor_line(view: &EpisodicAnchorView) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(occurred_at) = view.occurred_at.as_deref().map(js_trim) {
        if !occurred_at.is_empty() {
            let date_only = js_slice10(occurred_at);
            if date_only_re().is_match(&date_only) {
                parts.push(format!("when: {date_only}"));
            }
        }
    }
    if let Some(narrative_time) = view.narrative_time.as_deref().map(js_trim) {
        if !narrative_time.is_empty() {
            parts.push(format!("story time: {narrative_time}"));
        }
    }
    let entities: Vec<&str> = view
        .entities
        .iter()
        .map(|e| js_trim(e))
        .filter(|e| !e.is_empty())
        .take(ANCHOR_MAX_ENTITIES)
        .collect();
    if !entities.is_empty() {
        parts.push(format!("place: {}", entities.join(", ")));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("({})", parts.join(" · "))
}

/// Build the canonical embedded text for a memory: `summary\n\ncontent`, plus
/// the anchor line when any episodic anchor is present. Single source of truth —
/// the gate, the create paths, re-embeds, and reinforcement must all agree or
/// the gate compares against differently-shaped vectors than it writes.
///
/// New writes only carry the anchor; legacy rows re-embed without one until the
/// optional batched re-embed runs. Mixed old/new embedded-text rows are an
/// accepted, indefinite state (v4's cross-cutting list).
// LANE-REMOVE(P4.D290|P4.D291|P4.D292|P4.D296|P4.D297): v4 deleted buildMemoryEmbeddingText in af48f6cc2
pub fn build_memory_embedding_text(
    summary: &str,
    content: &str,
    anchors: Option<&EpisodicAnchorView>,
) -> String {
    let base = format!("{summary}\n\n{content}");
    let anchor_line = anchors.map(build_memory_anchor_line).unwrap_or_default();
    if anchor_line.is_empty() {
        base
    } else {
        format!("{base}\n{anchor_line}")
    }
}

/// v4 `MONTH_NAMES` — full lowercase month name → 0-based month.
fn month_number(name: &str) -> Option<i64> {
    Some(match name {
        "january" => 0,
        "february" => 1,
        "march" => 2,
        "april" => 3,
        "may" => 4,
        "june" => 5,
        "july" => 6,
        "august" => 7,
        "september" => 8,
        "october" => 9,
        "november" => 10,
        "december" => 11,
        _ => return None,
    })
}

/// v4 `UNIT_MS`.
fn unit_ms(unit: &str) -> Option<i64> {
    Some(match unit {
        "day" | "days" => DAY_MS,
        "week" | "weeks" => 7 * DAY_MS,
        "month" | "months" => 30 * DAY_MS,
        "year" | "years" => 365 * DAY_MS,
        _ => return None,
    })
}

/// v4 `WORD_NUMBERS`.
fn word_number(word: &str) -> Option<i64> {
    Some(match word {
        "a" | "an" | "one" => 1,
        "two" | "couple" => 2,
        "three" | "few" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        _ => return None,
    })
}

/// Midnight-UTC ISO timestamp for the calendar date derived from `ms` (v4
/// `isoDateFromMs` — `new Date(ms).toISOString().slice(0,10) + 'T00:00:00.000Z'`).
fn iso_date_from_ms(ms: i64) -> String {
    let full = iso_from_unix_ms(ms);
    format!("{}T00:00:00.000Z", &full[..10])
}

macro_rules! ws_re {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static RE: OnceLock<Regex> = OnceLock::new();
            RE.get_or_init(|| Regex::new(&$pat.replace("WS", JS_WS_CLASS)).unwrap())
        }
    };
}

// The when-phrase regexes, byte-faithful to v4's (JS `\s` → the JS whitespace
// class; JS `\d`/`[a-z]` are ASCII). Literal spaces stay literal where v4 wrote
// them literally (the anchor-moment / `last week` / `the other day` forms).
ws_re!(
    iso_phrase_re,
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
ws_re!(
    anchor_moment_re,
    r"^(today|tonight|this (morning|afternoon|evening|night)|earlier( today)?|just now|now)$"
);
ws_re!(yesterday_re, r"^(yesterday|last night)$");
ws_re!(
    ago_re,
    r"^(?:aboutWS+|aroundWS+|someWS+)?([a-z]+|[0-9]+)WS+(?:ofWS+)?(days?|weeks?|months?|years?)WS+(?:ago|back|earlier|before)$"
);
ws_re!(
    couple_re,
    r"^aWS+(couple|few)WS+(?:ofWS+)?(days?|weeks?|months?|years?)WS+(?:ago|back)$"
);
ws_re!(
    weekday_re,
    r"^lastWS+(sunday|monday|tuesday|wednesday|thursday|friday|saturday)$"
);
ws_re!(season_re, r"^lastWS+(spring|summer|fall|autumn|winter)$");

/// Deterministically resolve a `when` phrase into an ISO `occurredAt`, anchored
/// to `anchor_iso` (the source turn's message timestamp). Returns `None` for
/// anything unrecognized (including future-tense phrases) — recall must
/// degrade, never block, so an unresolvable phrase simply leaves `occurredAt`
/// to the fallback stamping rule. (v4 `resolveWhenPhrase`; see the module doc
/// for the full phrase inventory.)
// LANE-REMOVE(P4.D290|P4.D291|P4.D292|P4.D296|P4.D297): v4 deleted episodic.ts's resolveWhenPhrase in e0ac7493a
pub fn resolve_when_phrase(phrase: Option<&str>, anchor_iso: &str) -> Option<String> {
    let raw = js_trim(phrase?);
    if raw.is_empty() {
        return None;
    }
    let anchor_ms = js_date_parse_ms(anchor_iso)?;
    let lower = raw.to_lowercase();

    // Absolute ISO datetime / date.
    if let Some(m) = iso_phrase_re().captures(&lower) {
        let parsed = js_date_parse_ms(raw)?;
        return Some(if m.get(4).is_some() {
            iso_from_unix_ms(parsed)
        } else {
            format!("{}T00:00:00.000Z", &lower[..10])
        });
    }

    // "July 14, 2026" / "July 14" / "14 July 2026" / "14 July"
    let named: Option<(i64, i64, Option<i64>)> = if let Some((month, day, year)) =
        month_first_re().captures(&lower).and_then(|m| {
            let month = month_number(m.get(1).unwrap().as_str())?;
            let day: i64 = m.get(2).unwrap().as_str().parse().ok()?;
            let year: Option<i64> = m.get(3).map(|y| y.as_str().parse().unwrap());
            Some((month, day, year))
        }) {
        Some((month, day, year))
    } else {
        day_first_re().captures(&lower).and_then(|m| {
            let month = month_number(m.get(2).unwrap().as_str())?;
            let day: i64 = m.get(1).unwrap().as_str().parse().ok()?;
            let year: Option<i64> = m.get(3).map(|y| y.as_str().parse().unwrap());
            Some((month, day, year))
        })
    };
    if let Some((month, day, year)) = named {
        if (1..=31).contains(&day) {
            let anchor_year = utc_parts(anchor_ms).0;
            let mut y = year.unwrap_or(anchor_year);
            // `Date.UTC(year, month, day)` — `days_from_civil` is linear in the
            // day, so an over-the-end day rolls into the next month exactly as
            // JS normalizes it (February 31 → March 3).
            let mut candidate = days_from_civil(y, month + 1, day) * DAY_MS;
            // A yearless date that lands after the anchor refers to the previous
            // year (retold events are in the past).
            if year.is_none() && candidate > anchor_ms {
                y -= 1;
                candidate = days_from_civil(y, month + 1, day) * DAY_MS;
            }
            return Some(iso_date_from_ms(candidate));
        }
    }

    // The anchor moment itself.
    if anchor_moment_re().is_match(&lower) {
        return Some(iso_from_unix_ms(anchor_ms));
    }

    if yesterday_re().is_match(&lower) {
        return Some(iso_date_from_ms(anchor_ms - DAY_MS));
    }

    // "N days ago", "a week ago", "a couple of months ago", "few days back"
    if let Some(m) = ago_re().captures(&lower) {
        let word = m.get(1).unwrap().as_str();
        let n: Option<i64> = if word.bytes().all(|b| b.is_ascii_digit()) {
            word.parse().ok()
        } else {
            word_number(word)
        };
        let unit = unit_ms(m.get(2).unwrap().as_str());
        if let (Some(n), Some(unit)) = (n, unit) {
            if n > 0 && n < 10_000 {
                return Some(iso_date_from_ms(anchor_ms - n * unit));
            }
        }
    }
    // "a couple of days ago" (couple/few consume an extra "of" word slot)
    if let Some(m) = couple_re().captures(&lower) {
        let n = word_number(m.get(1).unwrap().as_str());
        let unit = unit_ms(m.get(2).unwrap().as_str());
        if let (Some(n), Some(unit)) = (n, unit) {
            return Some(iso_date_from_ms(anchor_ms - n * unit));
        }
    }

    // "last week/month/year", "the other day" (literal single spaces in v4).
    if lower == "last week" {
        return Some(iso_date_from_ms(anchor_ms - 7 * DAY_MS));
    }
    if lower == "last month" {
        return Some(iso_date_from_ms(anchor_ms - 30 * DAY_MS));
    }
    if lower == "last year" {
        return Some(iso_date_from_ms(anchor_ms - 365 * DAY_MS));
    }
    if lower == "the other day" {
        return Some(iso_date_from_ms(anchor_ms - 2 * DAY_MS));
    }

    // "last <weekday>" — the most recent strictly-past occurrence of that weekday.
    if let Some(m) = weekday_re().captures(&lower) {
        let weekdays = [
            "sunday",
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
        ];
        let target = weekdays
            .iter()
            .position(|w| *w == m.get(1).unwrap().as_str())
            .unwrap() as i64;
        let anchor_day = utc_day_of_week(anchor_ms);
        let mut back = (anchor_day - target).rem_euclid(7);
        if back == 0 {
            back = 7; // JS `|| 7`
        }
        return Some(iso_date_from_ms(anchor_ms - back * DAY_MS));
    }

    if season_re().is_match(&lower) {
        // Coarse: treat "last <season>" as ~6 months back. Precision is not the
        // point — landing in the right half-year is enough for window retrieval.
        return Some(iso_date_from_ms(anchor_ms - 182 * DAY_MS));
    }

    None
}

/// Event-clock reference time for age labels: prefer `occurredAt` (when the
/// event happened) over the write/reinforce clock. Falls back to the supplied
/// write-clock milliseconds when `occurredAt` is absent or unparsable.
///
/// v4 `eventReferenceTimeMs` — note the JS truthiness gate: only an ABSENT or
/// EMPTY string skips the parse (whitespace-only is truthy, parses to NaN, and
/// falls back through the finiteness check).
pub fn event_reference_time_ms(occurred_at: Option<&str>, write_clock_ms: f64) -> f64 {
    event_time_ms(occurred_at).unwrap_or(write_clock_ms)
}

/// The Option form of [`event_reference_time_ms`] for constructors that carry
/// the event time as a pre-parsed field (`MemoryInputs::occurred_at_ms`):
/// `Some(ms)` only when `occurredAt` is present, non-empty (JS truthiness),
/// and parsable — exactly the cases where v4's event clock takes over.
pub fn event_time_ms(occurred_at: Option<&str>) -> Option<f64> {
    let s = occurred_at?;
    if s.is_empty() {
        return None;
    }
    js_date_parse_ms(s).map(|ms| ms as f64)
}

/// JS `new Date(ms).getUTCDay()` — 0 = Sunday.
pub(crate) fn utc_day_of_week(ms: i64) -> i64 {
    let days = floor_div_ms_days(ms);
    (days + 4).rem_euclid(7)
}

/// JS `Date.parse` — V8's own parser, ES5 ISO pass AND legacy fallback,
/// ported whole in [`crate::v8_date_parse`] (the human's ruling on P4.D289's
/// `JS_DATE_PARSE_ISO_SUBSET` finding, 2026-10-14). A string with no UTC
/// offset reads as UTC (JS: the process's local zone — the ruled
/// `ZONELESS_PARSE_AS_UTC`; every oracle pins `TZ=UTC`).
pub(crate) fn js_date_parse_ms(s: &str) -> Option<i64> {
    crate::v8_date_parse::date_parse_ms(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_date_parse_v8_quirks() {
        // Confirmed against Node 24 (see the doc comment).
        assert_eq!(js_date_parse_ms("2026-07-00"), None);
        assert_eq!(js_date_parse_ms("2026-00-10"), None);
        assert_eq!(js_date_parse_ms("2026-07-32"), None);
        assert_eq!(js_date_parse_ms("2026-13-05"), None);
        // Day rolls past the month end.
        assert_eq!(
            js_date_parse_ms("2026-02-31"),
            js_date_parse_ms("2026-03-03")
        );
        // Fraction truncation.
        assert_eq!(
            js_date_parse_ms("2026-07-14T09:30:00.123456Z"),
            js_date_parse_ms("2026-07-14T09:30:00.123Z")
        );
        assert_eq!(
            js_date_parse_ms("2026-07-14T09:30:00.5Z"),
            js_date_parse_ms("2026-07-14T09:30:00.500Z")
        );
        // Lowercase t/z; bad shapes.
        assert!(js_date_parse_ms("2026-07-10t18:45:00z").is_some());
        assert_eq!(js_date_parse_ms("2026-07-14t9:30"), None);
        assert_eq!(js_date_parse_ms("2026-07-14T23:60:00Z"), None);
        assert_eq!(js_date_parse_ms("not-a-date"), None);
    }

    #[test]
    fn utc_day_of_week_known_days() {
        // 1970-01-01 was a Thursday (4); 2026-07-14 is a Tuesday (2).
        assert_eq!(utc_day_of_week(0), 4);
        assert_eq!(utc_day_of_week(js_date_parse_ms("2026-07-14").unwrap()), 2);
    }
}

#[cfg(test)]
mod hour_24_tests {
    use super::js_date_parse_ms;

    /// V8's `Date.parse` takes hour 24 only as the day's end (P4.D289).
    #[test]
    fn hour_24_parses_only_at_the_end_of_the_day() {
        assert_eq!(
            js_date_parse_ms("2026-07-14T24:00Z"),
            js_date_parse_ms("2026-07-15T00:00:00.000Z")
        );
        assert!(js_date_parse_ms("2026-07-14T24:00:00.000Z").is_some());
        assert_eq!(js_date_parse_ms("2026-07-14T24:01Z"), None);
        assert_eq!(js_date_parse_ms("2026-07-14T24:00:01Z"), None);
        assert_eq!(js_date_parse_ms("2026-07-14T24:00:00.001Z"), None);
        assert_eq!(js_date_parse_ms("2026-07-14t24:30z"), None);
    }
}
