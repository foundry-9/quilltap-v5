//! Port of v4 `lib/memory/time/rules.ts` (`e0ac7493a`) — the day-reference
//! table, ONE rule set for both directions of memory time.
//!
//! v4's why, carried: the Commonplace Book answers one question two ways: on
//! write, "when did this happen?" (a model's `when` phrase becomes an
//! `occurredAt`); on read, "which memories fall in the period this turn asks
//! about?" (a phrase in the conversation becomes a window). Each rule here
//! gives the local window a phrase names; the write side stamps the START of
//! that window and the read side asks for the whole of it, so a memory written
//! as "last night" always lands inside "last night" asked (bug 212). Windows
//! are on the instance memory calendar ([`super::calendar`]), never UTC days.
//!
//! JS regex fidelity (work order P4.D289 LR-9): v4 builds every regex here
//! WITHOUT the `u` flag, so `\b` and `\d` are ASCII and `/i` folds case with
//! the non-ASCII → ASCII mappings REFUSED — `yeſterday` (U+017F) and
//! `last weeK` (KELVIN SIGN U+212A) do not match. The Rust regexes therefore
//! fold case ASCII-only (`(?i-u:…)` over the ASCII literals), bound words
//! ASCII-only (`(?-u:\b)`), and spell `\d` as `[0-9]`. (v5's pre-port
//! `day_references.rs` folded by Unicode — a latent divergence this port does
//! not inherit.)

use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::calendar::{start_of_zoned_day_ms, start_of_zoned_week_ms, zoned_time_on_day_ms};

/// A resolved window: `[from_ms, end_ms)` — `end_ms` is EXCLUSIVE (v4
/// `RuleWindow`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuleWindow {
    pub from_ms: f64,
    pub end_ms: f64,
    /// False for a future reference ("tomorrow"): computed for logging, never
    /// applied.
    pub past_pointing: bool,
}

/// The signature of a rule's window: `(match, anchor_ms, time_zone)`.
type WindowFn = fn(&Captures<'_>, f64, &str) -> Option<RuleWindow>;

/// One row of the table (v4 `DayReferenceRule`).
pub struct DayReferenceRule {
    /// Short name, for logs and tests.
    pub name: &'static str,
    /// The phrase, as v4's regular-expression SOURCE matched whole: anchored
    /// (`^…$`) on the write side, word-bounded in running text on the read
    /// side. Kept byte-identical to v4's (`\d` and all); the Rust regexes are
    /// built from it by [`rust_source`].
    pub pattern: &'static str,
    window: WindowFn,
}

impl DayReferenceRule {
    /// The window the phrase names, relative to `anchor_ms` on `time_zone`'s
    /// calendar (v4 `rule.window(match, anchorMs, timeZone)`).
    pub fn window(&self, m: &Captures<'_>, anchor_ms: f64, time_zone: &str) -> Option<RuleWindow> {
        (self.window)(m, anchor_ms, time_zone)
    }
}

impl std::fmt::Debug for DayReferenceRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DayReferenceRule")
            .field("name", &self.name)
            .field("pattern", &self.pattern)
            .finish()
    }
}

/// Number words the "N days ago" rule accepts, alongside digits — in v4's
/// `Object.keys` order (the alternation order of the rule's pattern).
pub const DAY_NUMBER_WORDS: [(&str, i64); 14] = [
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

/// Largest "N days ago" the table resolves; beyond it, write-only rules (or
/// the LLM) answer.
pub const MAX_TABLE_DAYS_AGO: i64 = 14;

fn day(anchor_ms: f64, tz: &str, offset: i64) -> f64 {
    start_of_zoned_day_ms(anchor_ms, tz, offset)
}

fn week(anchor_ms: f64, tz: &str, offset: i64) -> f64 {
    start_of_zoned_week_ms(anchor_ms, tz, offset)
}

/// v4's "N days ago" pattern: `(\d{1,2}|one|…|fourteen) days? ago`, the
/// words joined from [`DAY_NUMBER_WORDS`] in order. A `const` so the table
/// can stay a `static`; [`day_number_words_pattern`] re-derives it from the
/// word list (a unit test holds them equal).
const N_DAYS_AGO_PATTERN: &str = r"(\d{1,2}|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen) days? ago";

fn n_days_ago_window(m: &Captures<'_>, a: f64, tz: &str) -> Option<RuleWindow> {
    // v4: `raw = m[1].toLowerCase()`; digits → `Number(raw)`, else the word.
    // `!n || n < 1 || n > MAX` → null (so "0 days ago" and "97 days ago"
    // match the table but resolve to nothing).
    let raw = m.get(1)?.as_str().to_lowercase();
    let n = if !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()) {
        raw.parse::<i64>().ok()?
    } else {
        DAY_NUMBER_WORDS
            .iter()
            .find(|(w, _)| *w == raw)
            .map(|(_, n)| *n)?
    };
    if !(1..=MAX_TABLE_DAYS_AGO).contains(&n) {
        return None;
    }
    Some(RuleWindow {
        from_ms: day(a, tz, -n),
        end_ms: day(a, tz, -n + 1),
        past_pointing: true,
    })
}

/// The table (v4 `DAY_REFERENCE_TABLE`), in v4's ORDER. Order matters for the
/// read scan: longest phrases first, so "day before yesterday" is consumed
/// whole and never re-read as "yesterday".
pub static DAY_REFERENCE_TABLE: [DayReferenceRule; 9] = [
    DayReferenceRule {
        name: "day before yesterday",
        pattern: "day before yesterday",
        window: |_m, a, tz| {
            Some(RuleWindow {
                from_ms: day(a, tz, -2),
                end_ms: day(a, tz, -1),
                past_pointing: true,
            })
        },
    },
    DayReferenceRule {
        name: "last night",
        pattern: "last night",
        window: |_m, a, tz| {
            Some(RuleWindow {
                from_ms: zoned_time_on_day_ms(a, tz, -1, 18),
                end_ms: zoned_time_on_day_ms(a, tz, 0, 6),
                past_pointing: true,
            })
        },
    },
    // Today and its parts: local midnight up to the anchor. A sub-day phrase
    // deliberately does not narrow the window — memories carry coarse event
    // times, and a too-tight window starves recall.
    DayReferenceRule {
        name: "today",
        pattern: "earlier today|this morning|this afternoon|this evening|tonight|today",
        window: |_m, a, tz| {
            Some(RuleWindow {
                from_ms: day(a, tz, 0),
                end_ms: a + 1.0,
                past_pointing: true,
            })
        },
    },
    DayReferenceRule {
        name: "yesterday",
        pattern: "yesterday",
        window: |_m, a, tz| {
            Some(RuleWindow {
                from_ms: day(a, tz, -1),
                end_ms: day(a, tz, 0),
                past_pointing: true,
            })
        },
    },
    DayReferenceRule {
        name: "N days ago",
        pattern: N_DAYS_AGO_PATTERN,
        window: n_days_ago_window,
    },
    DayReferenceRule {
        name: "this week",
        pattern: "this week",
        window: |_m, a, tz| {
            Some(RuleWindow {
                from_ms: week(a, tz, 0),
                end_ms: a + 1.0,
                past_pointing: true,
            })
        },
    },
    DayReferenceRule {
        name: "last week",
        pattern: "last week",
        window: |_m, a, tz| {
            Some(RuleWindow {
                from_ms: week(a, tz, -1),
                end_ms: week(a, tz, 0),
                past_pointing: true,
            })
        },
    },
    DayReferenceRule {
        name: "next week",
        pattern: "next week",
        window: |_m, a, tz| {
            Some(RuleWindow {
                from_ms: week(a, tz, 1),
                end_ms: week(a, tz, 2),
                past_pointing: false,
            })
        },
    },
    DayReferenceRule {
        name: "tomorrow",
        pattern: "tomorrow",
        window: |_m, a, tz| {
            Some(RuleWindow {
                from_ms: day(a, tz, 1),
                end_ms: day(a, tz, 2),
                past_pointing: false,
            })
        },
    },
];

/// v4's "N days ago" pattern rebuilt from [`DAY_NUMBER_WORDS`] the way v4
/// builds it (`Object.keys(DAY_NUMBER_WORDS).join('|')`).
pub fn day_number_words_pattern() -> String {
    let words: Vec<&str> = DAY_NUMBER_WORDS.iter().map(|(w, _)| *w).collect();
    format!(r"(\d{{1,2}}|{}) days? ago", words.join("|"))
}

/// A v4 pattern source as the Rust regex body with v4's non-`u` semantics:
/// `\d` → `[0-9]`, the whole body under ASCII-only case folding. Every table
/// pattern is ASCII letters, spaces, `|`, groups and `\d{1,2}`, so nothing
/// else needs translating.
fn rust_source(pattern: &str) -> String {
    format!("(?i-u:{})", pattern.replace(r"\d", "[0-9]"))
}

/// v4 `anchoredRules`: each rule's `^(?:pattern)$` with `/i`.
static ANCHORED_RULES: LazyLock<Vec<(&'static DayReferenceRule, Regex)>> = LazyLock::new(|| {
    DAY_REFERENCE_TABLE
        .iter()
        .map(|rule| {
            let re = Regex::new(&format!("^(?:{})$", rust_source(rule.pattern)))
                .expect("day-reference rule pattern compiles");
            (rule, re)
        })
        .collect()
});

/// The table rule a whole phrase names, with its match, or `None` (v4
/// `matchDayReferenceRule` — the FIRST rule whose anchored pattern matches,
/// case-insensitively, ASCII-only).
pub fn match_day_reference_rule(phrase: &str) -> Option<(&'static DayReferenceRule, Captures<'_>)> {
    ANCHORED_RULES
        .iter()
        .find_map(|(rule, re)| re.captures(phrase).map(|m| (*rule, m)))
}

/// v4 `DAY_REFERENCE_SCAN_PATTERN` (`/\b(?:(?:p1)|(?:p2)|…)\b/gi`, no `u`):
/// the whole table as one word-bounded alternation, for scanning running
/// text. Each alternative is wrapped so a match can be resolved back to its
/// rule. Iterate it with `find_iter` (the `/g` walk).
pub static DAY_REFERENCE_SCAN_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    let alternation = DAY_REFERENCE_TABLE
        .iter()
        .map(|rule| format!("(?:{})", rule.pattern))
        .collect::<Vec<_>>()
        .join("|");
    Regex::new(&format!(
        r"(?-u:\b){}(?-u:\b)",
        rust_source(&format!("(?:{alternation})"))
    ))
    .expect("day-reference scan pattern compiles")
});

/// v4's scan-pattern SOURCE string, rebuilt the way v4 builds it
/// (`'\\b(?:' + table.map(r => `(?:${r.pattern})`).join('|') + ')\\b'`) — the
/// harness compares it byte-for-byte against v4's `.source`.
pub fn day_reference_scan_source() -> String {
    let alternation = DAY_REFERENCE_TABLE
        .iter()
        .map(|rule| format!("(?:{})", rule.pattern))
        .collect::<Vec<_>>()
        .join("|");
    format!(r"\b(?:{alternation})\b")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n_days_pattern_matches_the_word_list() {
        assert_eq!(N_DAYS_AGO_PATTERN, day_number_words_pattern());
    }

    #[test]
    fn ascii_only_case_folding() {
        assert_eq!(
            match_day_reference_rule("YESTERDAY").map(|(r, _)| r.name),
            Some("yesterday")
        );
        assert!(match_day_reference_rule("ye\u{17F}terday").is_none());
        assert!(match_day_reference_rule("last wee\u{212A}").is_none());
        assert!(DAY_REFERENCE_SCAN_PATTERN
            .find("last wee\u{212A} we met")
            .is_none());
        assert!(DAY_REFERENCE_SCAN_PATTERN
            .find("ye\u{17F}terday was long")
            .is_none());
        let (rule, m) = match_day_reference_rule("THREE DAYS AGO").unwrap();
        assert_eq!(rule.name, "N days ago");
        assert_eq!(&m[1], "THREE");
    }
}
