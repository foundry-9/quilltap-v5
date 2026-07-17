//! Port of v4's `lib/pascal/dice.ts` — the single source of truth for random
//! rolls and `NdS±M` notation.
//!
//! Pascal's table deals in two kinds of chance: the free-form numeric ranges of
//! a custom tool's `roll` object, and honest dice. Both land here, as does the
//! `rng` tool and the prose scanner that spots "2d6" in a message. One parser,
//! one roller, one source of randomness.
//!
//! Randomness is CSPRNG-derived with rejection sampling, never a fast PRNG — a
//! roll is a persisted, tamper-evident fact, so it is worth the few extra bytes.
//!
//! The roller half was formerly private to v5's `tools::rng` (v4:
//! `lib/tools/handlers/rng-handler.ts`); the notation half is new. Before it
//! existed, the only thing resembling a parser was a prose regex that captured
//! count and sides and silently dropped the modifier — "3d6+2" rolled 3d6 and
//! threw the +2 away.
//!
//! ## The randomness seam
//!
//! v4 draws from `crypto.randomBytes` (a CSPRNG) via `secureRandomInt`'s
//! rejection-sampling loop (rejects the top slice of the byte range to avoid
//! modulo bias). Because rejection sampling consumes a *variable* number of
//! bytes, the exact algorithm — how many bytes per draw, how the little-endian
//! integer is assembled, and the rejection condition — is what makes two
//! implementations consume an identical byte stream identically. So the byte
//! source is injected as a [`RandomBytes`] trait: production draws from the OS
//! CSPRNG ([`OsRandomBytes`]); the differential feeds both v4 (a `randomBytes`
//! mock) and the Rust port ([`FixedBytes`]) the same committed byte sequence, and
//! byte-exact algorithm fidelity is itself part of what the differential proves.
//! The seam moved here with the crypto it guards, mirroring v4's own move; it is
//! re-exported from [`crate::tools::rng`] so both spellings resolve.
//!
//! ## The two regexes are NOT interchangeable
//!
//! `DICE_NOTATION_SCAN` (v4 `dice.ts:69`) forbids whitespace around the sign;
//! `DICE_NOTATION_STRICT` (v4 `dice.ts:71`) allows it. See each constant.
//!
//! ## JS → Rust regex fidelity
//!
//! * JS `\d` is ASCII → `[0-9]` (the `extract_novel_details` precedent).
//! * JS `\b` (no `u` flag) is the ASCII word boundary over `[A-Za-z0-9_]` →
//!   `(?-u:\b)` (the `mentioned_characters` precedent).
//! * JS `\s` is a wider set than Rust's `\s` (it includes U+FEFF and excludes
//!   U+0085) → [`crate::jsstr::JS_WS_CLASS`] (the `carina_parser` precedent).
//! * `gi` global-exec semantics: non-overlapping, leftmost-first, advancing past
//!   each match — matched by the `regex` crate's `captures_iter`.

use std::sync::LazyLock;

use regex::Regex;

use crate::jsstr::JS_WS_CLASS;

/// Smallest legal die. A one-sided die is not a die.
pub const MIN_DIE_SIDES: u32 = 2;
/// Largest legal die. Mirrors the `rng` tool's long-standing bound.
pub const MAX_DIE_SIDES: u32 = 1000;
/// Fewest dice in a roll.
pub const MIN_DICE_COUNT: u32 = 1;
/// Most dice in a roll. Mirrors the `rng` tool's long-standing bound.
pub const MAX_DICE_COUNT: u32 = 100;
/// Bound on the flat modifier, kept symmetric and well clear of precision
/// trouble.
pub const MAX_DICE_MODIFIER: i64 = 1000;

/// A parsed `NdS±M` roll specification (v4 `DiceNotation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiceNotation {
    /// How many dice. `d20` implies 1.
    pub count: u32,
    /// Sides per die.
    pub sides: u32,
    /// Flat modifier applied to the total. 0 when the notation carried none.
    pub modifier: i64,
}

/// One hit from [`scan_dice_notation`] (v4 `ScannedDiceNotation`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedDiceNotation {
    pub notation: DiceNotation,
    /// The exact substring that matched, for echoing back to the user.
    pub match_text: String,
}

/// The outcome of rolling a [`DiceNotation`] (v4 `DiceRollResult`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiceRollResult {
    pub count: u32,
    pub sides: u32,
    pub modifier: i64,
    /// Each die's face, in roll order.
    pub results: Vec<i64>,
    /// Sum of the faces, before the modifier.
    pub subtotal: i64,
    /// `subtotal + modifier` — the number that counts.
    pub total: i64,
}

/// Dice notation as it appears loose in prose: "d20", "2d6", "3d6+2", "2d10-1"
/// (v4 `DICE_NOTATION_SCAN`, `/\b(\d+)?d(\d+)(?:([+-])(\d+))?\b/gi`).
///
/// Whitespace around the sign is deliberately NOT allowed. "2d6 - 1 apple"
/// should read as a 2d6 roll near an unrelated subtraction, not as 2d6-1; real
/// dice notation is written closed up. This keeps the scanner's false-positive
/// rate where it has always been while letting the closed-up form carry its
/// modifier through.
///
/// The trailing `(?-u:\b)` sits AFTER the optional modifier group, so it applies
/// after the modifier digits when the group participates and after the sides
/// digits when it does not — the leftmost-first engine tries the modifier first
/// and falls back to the bare form when the boundary fails ("3d6+2x" → "3d6").
static DICE_NOTATION_SCAN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?-u:\b)([0-9]+)?d([0-9]+)(?:([+-])([0-9]+))?(?-u:\b)").unwrap()
});

/// Anchored form for parsing a string that must be notation and nothing else
/// (v4 `DICE_NOTATION_STRICT`, `/^\s*(\d+)?d(\d+)(?:\s*([+-])\s*(\d+))?\s*$/i`).
/// Whitespace around the sign IS allowed here, because the whole string must be
/// the notation — there is no surrounding prose to be ambiguous with.
static DICE_NOTATION_STRICT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)^{JS_WS_CLASS}*([0-9]+)?d([0-9]+)(?:{JS_WS_CLASS}*([+-]){JS_WS_CLASS}*([0-9]+))?{JS_WS_CLASS}*$"
    ))
    .unwrap()
});

/// JS `Number.isInteger` — a finite, integral f64.
fn is_integer(v: f64) -> bool {
    v.is_finite() && v.fract() == 0.0
}

/// v4 `parseInt(s, 10)` over an all-digits capture: the decimal value as an
/// f64 (v4's numbers ARE f64, so a digit string past 2^53 rounds identically,
/// and one past the f64 range becomes `Infinity` on both sides — either way far
/// outside every bound below, so [`within_bounds`] rejects it).
fn parse_int_10(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(f64::INFINITY)
}

/// v4 `withinBounds` — true when `count`/`sides`/`modifier` are all integers
/// within bounds.
fn within_bounds(count: f64, sides: f64, modifier: f64) -> bool {
    is_integer(count)
        && is_integer(sides)
        && is_integer(modifier)
        && count >= MIN_DICE_COUNT as f64
        && count <= MAX_DICE_COUNT as f64
        && sides >= MIN_DIE_SIDES as f64
        && sides <= MAX_DIE_SIDES as f64
        && modifier.abs() <= MAX_DICE_MODIFIER as f64
}

/// v4 `fromCaptures` — build a [`DiceNotation`] from regex captures, or `None`
/// when out of bounds.
///
/// Out-of-bounds notation is SKIPPED, never clamped: a "500d6" in prose is not
/// a request for 100d6, it is not a request this system honours at all.
fn from_captures(
    raw_count: Option<&str>,
    raw_sides: &str,
    sign: Option<&str>,
    raw_modifier: Option<&str>,
) -> Option<DiceNotation> {
    let count = raw_count.map(parse_int_10).unwrap_or(1.0);
    let sides = parse_int_10(raw_sides);
    let magnitude = raw_modifier.map(parse_int_10).unwrap_or(0.0);
    let modifier = if sign == Some("-") {
        -magnitude
    } else {
        magnitude
    };

    if !within_bounds(count, sides, modifier) {
        return None;
    }
    // within_bounds proved all three are integers inside these ranges, so the
    // casts are exact.
    Some(DiceNotation {
        count: count as u32,
        sides: sides as u32,
        modifier: modifier as i64,
    })
}

/// v4 `parseDiceNotation` — parse a complete dice-notation string ("3d6+2").
/// `None` when the string is not notation, or when its numbers fall outside the
/// supported bounds.
///
/// Strict: the whole string must be the notation. Use [`scan_dice_notation`] to
/// find notation embedded in prose.
pub fn parse_dice_notation(notation: &str) -> Option<DiceNotation> {
    let caps = DICE_NOTATION_STRICT.captures(notation)?;
    from_captures(
        caps.get(1).map(|m| m.as_str()),
        caps.get(2).unwrap().as_str(),
        caps.get(3).map(|m| m.as_str()),
        caps.get(4).map(|m| m.as_str()),
    )
}

/// v4 `scanDiceNotation` — find every dice notation embedded in a block of
/// prose, in order.
///
/// Out-of-bounds matches (d1, 500d6) are skipped rather than clamped — the same
/// behaviour the prose scanner has always had. (v4 resets the global regex's
/// `lastIndex` here; `captures_iter` carries no cross-call state, so the Rust
/// port has no equivalent chore.)
pub fn scan_dice_notation(content: &str) -> Vec<ScannedDiceNotation> {
    let mut found = Vec::new();
    for caps in DICE_NOTATION_SCAN.captures_iter(content) {
        if let Some(notation) = from_captures(
            caps.get(1).map(|m| m.as_str()),
            caps.get(2).unwrap().as_str(),
            caps.get(3).map(|m| m.as_str()),
            caps.get(4).map(|m| m.as_str()),
        ) {
            found.push(ScannedDiceNotation {
                notation,
                match_text: caps.get(0).unwrap().as_str().to_string(),
            });
        }
    }
    found
}

/// v4 `formatDiceNotation` — render a notation back to its canonical string
/// ("3d6+2"); a zero modifier renders bare ("3d6").
pub fn format_dice_notation(n: &DiceNotation) -> String {
    let base = format!("{}d{}", n.count, n.sides);
    if n.modifier == 0 {
        return base;
    }
    let sign = if n.modifier > 0 { '+' } else { '-' };
    format!("{base}{sign}{}", n.modifier.abs())
}

// ---------------------------------------------------------------------------
// The randomness seam.
// ---------------------------------------------------------------------------

/// A source of raw random bytes — the injected seam over v4's `crypto.randomBytes`.
pub trait RandomBytes {
    /// Return exactly `n` bytes (v4 `randomBytes(n)`).
    fn random_bytes(&mut self, n: usize) -> Vec<u8>;
}

/// Production impl: the OS CSPRNG (the `getrandom` syscall — the same source
/// `crypto.randomBytes` draws from).
pub struct OsRandomBytes;
impl RandomBytes for OsRandomBytes {
    fn random_bytes(&mut self, n: usize) -> Vec<u8> {
        let mut buf = vec![0u8; n];
        getrandom::getrandom(&mut buf).expect("OS CSPRNG");
        buf
    }
}

/// A fixed, replayable byte stream — the differential's committed sequence
/// (mirrors v4's `randomBytes` mock). Panics if the stream is exhausted, so a
/// corpus byte-shortage surfaces loudly rather than silently diverging.
pub struct FixedBytes {
    bytes: Vec<u8>,
    cursor: usize,
}
impl FixedBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes, cursor: 0 }
    }
    /// The number of bytes consumed so far (a differential can assert the whole
    /// committed sequence was drawn — catching an unexpected extra consumer).
    pub fn consumed(&self) -> usize {
        self.cursor
    }
}
impl RandomBytes for FixedBytes {
    fn random_bytes(&mut self, n: usize) -> Vec<u8> {
        let end = self.cursor + n;
        assert!(
            end <= self.bytes.len(),
            "FixedBytes exhausted: needed {n} at cursor {} of {}",
            self.cursor,
            self.bytes.len()
        );
        let out = self.bytes[self.cursor..end].to_vec();
        self.cursor = end;
        out
    }
}

// ---------------------------------------------------------------------------
// Secure integer (rejection sampling).
// ---------------------------------------------------------------------------

/// v4 `secureRandomInt(max)` — a uniform integer in `[1, max]` via rejection
/// sampling (rejects `value >= limit` to avoid modulo bias). Byte-for-byte
/// faithful: `bytesNeeded = ceil(log2(max+1)/8) || 1`, the integer assembled
/// little-endian (`sum(byte[i] * 256^i)`), rejecting the top `maxValue % max`
/// slice. Reproduced in `u128` so the `256^bytesNeeded` products are exact (v4
/// computes them as f64, exact for the byte widths dice/coin/spin reach).
pub fn secure_random_int(max: u64, rng: &mut dyn RandomBytes) -> u64 {
    if max < 1 {
        return 1;
    }
    let bytes_needed: usize = {
        // Math.ceil(Math.log2(max + 1) / 8) || 1 — the boundary crossings are at
        // exact powers of two (log2 is exact there), so f64 is safe.
        let b = ((max as f64 + 1.0).log2() / 8.0).ceil();
        if b >= 1.0 {
            b as usize
        } else {
            1
        }
    };
    let max_value: u128 = 256u128.pow(bytes_needed as u32);
    let limit = max_value - (max_value % max as u128);
    loop {
        let bytes = rng.random_bytes(bytes_needed);
        let mut value: u128 = 0;
        for (i, byte) in bytes.iter().enumerate() {
            value += (*byte as u128) * 256u128.pow(i as u32);
        }
        if value < limit {
            return ((value % max as u128) + 1) as u64;
        }
    }
}

/// v4 `rollDice` — `count` rolls of a `sides`-sided die; returns the rolls and
/// their sum.
pub fn roll_dice(sides: u64, count: u32, rng: &mut dyn RandomBytes) -> (Vec<i64>, i64) {
    let mut results = Vec::with_capacity(count as usize);
    let mut sum: i64 = 0;
    for _ in 0..count {
        let roll = secure_random_int(sides, rng) as i64;
        results.push(roll);
        sum += roll;
    }
    (results, sum)
}

/// v4 `flipCoin` — `count` flips; `secureRandomInt(2) === 1 ? 'heads' : 'tails'`.
///
/// Returns the bare strings v4 returns. The `rng` tool lifts them into its own
/// `RngResult` union at the call site: that keeps the dice module free of
/// tool-layer types, so Pascal's custom tools can roll through it without
/// dragging the `rng` tool along.
pub fn flip_coin(count: u32, rng: &mut dyn RandomBytes) -> Vec<String> {
    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let flip = secure_random_int(2, rng);
        results.push(if flip == 1 { "heads" } else { "tails" }.to_string());
    }
    results
}

/// v4 `rollNotation` — roll a parsed notation, applying its modifier to the
/// total. `subtotal` is the sum of the faces; `total` is `subtotal + modifier`.
pub fn roll_notation(notation: &DiceNotation, rng: &mut dyn RandomBytes) -> DiceRollResult {
    let (results, sum) = roll_dice(notation.sides as u64, notation.count, rng);
    DiceRollResult {
        count: notation.count,
        sides: notation.sides,
        modifier: notation.modifier,
        results,
        subtotal: sum,
        total: sum + notation.modifier,
    }
}

/// v4 `formatDiceBreakdown` — a human-readable breakdown of a roll, e.g.
/// `3d6+2: [4, 2, 6] + 2 = 14`; a zero modifier renders `3d6: [4, 2, 6] = 12`.
///
/// This is what `{{dice}}` renders to in a custom tool's outcome message.
pub fn format_dice_breakdown(result: &DiceRollResult) -> String {
    let notation = format_dice_notation(&DiceNotation {
        count: result.count,
        sides: result.sides,
        modifier: result.modifier,
    });
    let faces = format!(
        "[{}]",
        result
            .results
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    if result.modifier == 0 {
        return format!("{notation}: {faces} = {}", result.total);
    }
    let sign = if result.modifier > 0 { '+' } else { '-' };
    format!(
        "{notation}: {faces} {sign} {} = {}",
        result.modifier.abs(),
        result.total
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(count: u32, sides: u32, modifier: i64) -> DiceNotation {
        DiceNotation {
            count,
            sides,
            modifier,
        }
    }

    #[test]
    fn parse_strict_forms() {
        assert_eq!(parse_dice_notation("d20"), Some(n(1, 20, 0)));
        assert_eq!(parse_dice_notation("3d6+2"), Some(n(3, 6, 2)));
        assert_eq!(parse_dice_notation("2d10-1"), Some(n(2, 10, -1)));
        // Whitespace around the sign IS allowed in the strict form.
        assert_eq!(parse_dice_notation("  3d6 + 2 "), Some(n(3, 6, 2)));
        // Anything beyond the notation fails the anchors.
        assert_eq!(parse_dice_notation("3d6+2 total"), None);
        assert_eq!(parse_dice_notation("nonsense"), None);
    }

    #[test]
    fn parse_skips_never_clamps() {
        assert_eq!(parse_dice_notation("d1"), None); // sides < 2
        assert_eq!(parse_dice_notation("d1001"), None); // sides > 1000
        assert_eq!(parse_dice_notation("101d6"), None); // count > 100
        assert_eq!(parse_dice_notation("1d6+1001"), None); // |modifier| > 1000
        assert_eq!(
            parse_dice_notation("100d1000+1000"),
            Some(n(100, 1000, 1000))
        );
        assert_eq!(parse_dice_notation("1d6-1000"), Some(n(1, 6, -1000)));
    }

    #[test]
    fn scan_forbids_whitespace_around_the_sign() {
        // The whole point of the two-regex split: a nearby subtraction is not a
        // modifier.
        let loose = scan_dice_notation("2d6 - 1 apple");
        assert_eq!(loose.len(), 1);
        assert_eq!(loose[0].notation, n(2, 6, 0));
        assert_eq!(loose[0].match_text, "2d6");

        let closed = scan_dice_notation("2d6-1");
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].notation, n(2, 6, -1));
        assert_eq!(closed[0].match_text, "2d6-1");
    }

    #[test]
    fn scan_trailing_boundary_falls_back_to_the_bare_form() {
        // "+2x" fails the trailing \b, so the leftmost-first engine drops the
        // modifier group rather than the whole match.
        let hits = scan_dice_notation("3d6+2x");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].notation, n(3, 6, 0));
        assert_eq!(hits[0].match_text, "3d6");
    }

    #[test]
    fn scan_in_order_and_skips_out_of_bounds() {
        let hits = scan_dice_notation("roll d6, then 2d20+3, ignore d1 and 500d6");
        let got: Vec<_> = hits
            .iter()
            .map(|h| (h.notation, h.match_text.as_str()))
            .collect();
        assert_eq!(got, vec![(n(1, 6, 0), "d6"), (n(2, 20, 3), "2d20+3")]);
    }

    #[test]
    fn format_notation_and_breakdown() {
        assert_eq!(format_dice_notation(&n(3, 6, 2)), "3d6+2");
        assert_eq!(format_dice_notation(&n(3, 6, 0)), "3d6");
        assert_eq!(format_dice_notation(&n(2, 10, -1)), "2d10-1");

        let modified = DiceRollResult {
            count: 3,
            sides: 6,
            modifier: 2,
            results: vec![4, 2, 6],
            subtotal: 12,
            total: 14,
        };
        assert_eq!(
            format_dice_breakdown(&modified),
            "3d6+2: [4, 2, 6] + 2 = 14"
        );
        let bare = DiceRollResult {
            modifier: 0,
            total: 12,
            ..modified.clone()
        };
        assert_eq!(format_dice_breakdown(&bare), "3d6: [4, 2, 6] = 12");
        let negative = DiceRollResult {
            modifier: -1,
            total: 11,
            ..modified.clone()
        };
        assert_eq!(
            format_dice_breakdown(&negative),
            "3d6-1: [4, 2, 6] - 1 = 11"
        );
    }

    #[test]
    fn roll_notation_applies_the_modifier() {
        // d6: bytesNeeded=1, limit=252 — 3→4, 1→2, 5→6; subtotal 12, +2 = 14.
        let mut rng = FixedBytes::new(vec![3, 1, 5]);
        let got = roll_notation(&n(3, 6, 2), &mut rng);
        assert_eq!(got.results, vec![4, 2, 6]);
        assert_eq!(got.subtotal, 12);
        assert_eq!(got.total, 14);
        assert_eq!(rng.consumed(), 3);
    }

    #[test]
    fn secure_random_int_d6_no_rejection() {
        // d6: bytesNeeded=1, limit=252. Bytes below 252 never reject.
        let mut rng = FixedBytes::new(vec![0x03, 0x04]);
        assert_eq!(secure_random_int(6, &mut rng), 4); // 3 % 6 + 1
        assert_eq!(secure_random_int(6, &mut rng), 5); // 4 % 6 + 1
        assert_eq!(rng.consumed(), 2);
    }

    #[test]
    fn secure_random_int_rejects_top_slice() {
        // d6: limit=252. 0xFF (255) >= 252 → reject and redraw; 0x00 → 1.
        let mut rng = FixedBytes::new(vec![0xFF, 0x00]);
        assert_eq!(secure_random_int(6, &mut rng), 1);
        assert_eq!(rng.consumed(), 2); // consumed the rejected byte + the accepted one
    }

    #[test]
    fn secure_random_int_coin_never_rejects() {
        // max=2: limit=256, so every byte is accepted; parity decides.
        let mut rng = FixedBytes::new(vec![0x00, 0x01, 0xFF]);
        assert_eq!(secure_random_int(2, &mut rng), 1); // 0 % 2 + 1
        assert_eq!(secure_random_int(2, &mut rng), 2); // 1 % 2 + 1
        assert_eq!(secure_random_int(2, &mut rng), 2); // 255 % 2 + 1
    }

    #[test]
    fn secure_random_int_two_byte_width() {
        // d1000: bytesNeeded=2 (ceil(log2(1001)/8)=2), little-endian assembly.
        let mut rng = FixedBytes::new(vec![0x10, 0x00]); // value = 16
        assert_eq!(secure_random_int(1000, &mut rng), 17); // 16 % 1000 + 1
        assert_eq!(rng.consumed(), 2);
    }

    #[test]
    fn flip_coin_returns_bare_strings() {
        let mut rng = FixedBytes::new(vec![0x00, 0x01]);
        assert_eq!(flip_coin(2, &mut rng), vec!["heads", "tails"]);
    }
}
