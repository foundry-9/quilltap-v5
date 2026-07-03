//! Port of v4's `lib/services/chat-message/rng-pattern-detector.service.ts` —
//! scanning a user message for dice / coin-flip / spin-the-bottle patterns and
//! converting them into RNG tool calls (the pre-message auto-detect pipeline).
//!
//! ## The three regexes (JS → Rust fidelity)
//!
//! v4 uses three `gi` global regexes and `exec`-loops each in turn:
//!
//! * `DICE_PATTERN = /\b(\d+)?d(\d+)\b/gi` — "d6", "2d20"; count optional. v4
//!   enforces the bounds *in code*: sides in `[2, 1000]`, count in `[1, 100]`.
//! * `COIN_FLIP_PATTERN = /\bflip.{1,3}coin\b/gi` — 1–3 chars between "flip" and
//!   "coin", so "flip a coin" matches but "flip the coin" (4 chars: `_the_`)
//!   does NOT. A deliberate quirk — banked.
//! * `SPIN_BOTTLE_PATTERN = /\bspin\b.{0,50}\bbottle\b/gi` — up to 50 chars
//!   between (bounded to prevent ReDoS).
//!
//! Fidelity traps (all have tree precedent — followed here):
//!   * JS `\d` is ASCII → `[0-9]` (the `extract_novel_details` precedent).
//!   * JS `\b` (no `u` flag) is the ASCII word boundary over `[A-Za-z0-9_]` →
//!     `(?-u:\b)` (the `mentioned_characters` precedent). So a non-ASCII letter
//!     adjacent to a pattern still counts as a boundary from the pattern's side
//!     (e.g. `"éd6"`: `é` is two non-word UTF-8 bytes, so there IS a boundary
//!     before `d6` → it matches).
//!   * JS `.` (no `s` flag) excludes all four line terminators (`\n`, `\r`,
//!     U+2028, U+2029), not just `\n` as Rust's default `.` does → the
//!     `carina_parser` `[^\n\r\u{2028}\u{2029}]` class.
//!   * `gi` global-exec semantics: non-overlapping, leftmost, advancing past each
//!     match — matched by the `regex` crate's `captures_iter` / `find_iter`.
//!   * Conversion order is per-pattern-type in document order: **dice first,
//!     then coin, then bottle** (v4 runs the three loops in that sequence).

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::tools::rng::RngType;

/// JS `.` (no `s` flag): every char except the four line terminators.
const JS_DOT: &str = "[^\n\r\u{2028}\u{2029}]";

static DICE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?-u:\b)([0-9]+)?d([0-9]+)(?-u:\b)").unwrap());
static COIN_FLIP_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)(?-u:\b)flip{JS_DOT}{{1,3}}coin(?-u:\b)")).unwrap());
static SPIN_BOTTLE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?-u:\b)spin(?-u:\b){JS_DOT}{{0,50}}(?-u:\b)bottle(?-u:\b)"
    ))
    .unwrap()
});

/// The kind of a detected pattern (v4 `DetectedRngPattern.type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedRngType {
    Dice,
    FlipCoin,
    SpinTheBottle,
}

/// A detected RNG pattern (v4 `DetectedRngPattern`). Serializes with `sides`
/// omitted when absent (v4's optional `sides?`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DetectedRngPattern {
    #[serde(rename = "type")]
    pub type_: DetectedRngType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sides: Option<u32>,
    pub count: u32,
    #[serde(rename = "matchText")]
    pub match_text: String,
}

/// An RNG tool call derived from a detected pattern (v4 `RngToolCall`). `type`
/// serializes to v4's exact JSON (a number for dice, a string otherwise).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RngToolCall {
    #[serde(rename = "type")]
    pub type_: RngType,
    pub rolls: u32,
    #[serde(rename = "matchText")]
    pub match_text: String,
}

/// v4 `detectRngPatterns` — scan `content` for dice, coin, and bottle patterns.
/// The three loops run in order, so dice patterns precede coin, which precede
/// bottle, each in document order.
pub fn detect_rng_patterns(content: &str) -> Vec<DetectedRngPattern> {
    let mut patterns = Vec::new();

    // Dice — `\b(\d+)?d(\d+)\b`, count optional, bounds enforced in code.
    for caps in DICE_PATTERN.captures_iter(content) {
        let count: u64 = match caps.get(1) {
            // A digit string too long for u64 is far out of the [1,100] range, so
            // an overflow is treated as "too large" (rejected) — matching v4's
            // parseInt → a large Number failing the bound.
            Some(m) => m.as_str().parse().unwrap_or(u64::MAX),
            None => 1,
        };
        let sides: u64 = caps.get(2).unwrap().as_str().parse().unwrap_or(u64::MAX);
        if (2..=1000).contains(&sides) && (1..=100).contains(&count) {
            patterns.push(DetectedRngPattern {
                type_: DetectedRngType::Dice,
                sides: Some(sides as u32),
                count: count as u32,
                match_text: caps.get(0).unwrap().as_str().to_string(),
            });
        }
    }

    // Coin flips — `\bflip.{1,3}coin\b`.
    for m in COIN_FLIP_PATTERN.find_iter(content) {
        patterns.push(DetectedRngPattern {
            type_: DetectedRngType::FlipCoin,
            sides: None,
            count: 1,
            match_text: m.as_str().to_string(),
        });
    }

    // Spin the bottle — `\bspin\b.{0,50}\bbottle\b`.
    for m in SPIN_BOTTLE_PATTERN.find_iter(content) {
        patterns.push(DetectedRngPattern {
            type_: DetectedRngType::SpinTheBottle,
            sides: None,
            count: 1,
            match_text: m.as_str().to_string(),
        });
    }

    patterns
}

/// v4 `convertPatternsToToolCalls` — a detected pattern becomes an RNG tool call.
/// A dice pattern's type is its side count; coin/bottle keep their string type.
pub fn convert_patterns_to_tool_calls(patterns: &[DetectedRngPattern]) -> Vec<RngToolCall> {
    patterns
        .iter()
        .map(|pattern| match pattern.type_ {
            DetectedRngType::Dice => RngToolCall {
                // v4: `pattern.type === 'dice' && pattern.sides`. `sides` is always
                // Some here (dice always captures a side count).
                type_: RngType::Dice(pattern.sides.unwrap_or(0)),
                rolls: pattern.count,
                match_text: pattern.match_text.clone(),
            },
            DetectedRngType::FlipCoin => RngToolCall {
                type_: RngType::FlipCoin,
                rolls: pattern.count,
                match_text: pattern.match_text.clone(),
            },
            DetectedRngType::SpinTheBottle => RngToolCall {
                type_: RngType::SpinTheBottle,
                rolls: pattern.count,
                match_text: pattern.match_text.clone(),
            },
        })
        .collect()
}

/// v4 `detectAndConvertRngPatterns` — detect then convert in one call.
pub fn detect_and_convert_rng_patterns(content: &str) -> Vec<RngToolCall> {
    convert_patterns_to_tool_calls(&detect_rng_patterns(content))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(content: &str) -> Vec<String> {
        detect_and_convert_rng_patterns(content)
            .iter()
            .map(|c| serde_json::to_value(c.type_).unwrap().to_string())
            .collect()
    }

    #[test]
    fn plain_and_counted_dice() {
        let p = detect_rng_patterns("roll a d6 and 2d20");
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].sides, Some(6));
        assert_eq!(p[0].count, 1);
        assert_eq!(p[1].sides, Some(20));
        assert_eq!(p[1].count, 2);
    }

    #[test]
    fn dice_bounds() {
        assert!(detect_rng_patterns("d1").is_empty()); // sides < 2
        assert!(detect_rng_patterns("d1001").is_empty()); // sides > 1000
        assert!(detect_rng_patterns("101d6").is_empty()); // count > 100
        assert_eq!(detect_rng_patterns("100d1000").len(), 1); // at the bounds
    }

    #[test]
    fn coin_quirk() {
        assert_eq!(detect_rng_patterns("flip a coin").len(), 1);
        assert_eq!(detect_rng_patterns("flip coin").len(), 1); // " " = 1 char
        assert!(detect_rng_patterns("flip the coin").is_empty()); // " the " = 5 chars
    }

    #[test]
    fn convert_order_dice_then_coin() {
        // "Flip a coin then roll d20." → dice loop finds d20 first, coin second.
        assert_eq!(
            types("Flip a coin then roll d20."),
            vec!["20", "\"flip_coin\""]
        );
    }
}
