//! Port of v4's `lib/tools/llm-number.ts` — lenient numbers for LLM-supplied
//! tool arguments.
//!
//! Models routinely quote their numbers — `{"type": "6"}` rather than
//! `{"type": 6}` — because a tool call is JSON they wrote by hand, and the habit
//! varies by provider, by model, and by mood. A bare `z.number()` rejects the
//! quoted form outright, so the roll never happens and the character is told
//! their perfectly sensible request was invalid.
//!
//! [`llm_number`] is v4's `z.preprocess` step: a numeric-looking string is
//! converted before validation. It is **deliberately narrower than
//! `z.coerce.number()`**, which runs everything through `Number()` and therefore
//! quietly turns `true` into 1, and `null` and `[]` into 0 — **trading a
//! rejected call for a wrong one, which is the worse failure**. Only strings are
//! touched; every other type falls through to the validator untouched and is
//! rejected on its merits.
//!
//! Bounds still apply AFTER conversion, so `"1001"` fails a `max(1000)` exactly
//! as `1001` does, and `"6.5"` fails an `.int()` exactly as `6.5` does. The
//! derived JSON Schema is unchanged — the model is still told `integer`; this
//! only forgives it for not having listened.
//!
//! ## The three semantics (each is load-bearing)
//!
//! 1. **Non-string → returned untouched.** The `z.coerce.number()` divergence
//!    above.
//! 2. **Trimmed-empty → the ORIGINAL string**, not `0`. `Number('')` is 0; an
//!    empty string is a missing value, not a zero, so it is left for the
//!    validator to refuse.
//! 3. **Non-finite → the ORIGINAL string**, so the error message names what the
//!    model actually sent rather than `NaN`.
//!
//! ## `Number()`, not `parseInt`
//!
//! The conversion is JS `Number(string)` (the spec's StringToNumber), which is
//! not `parseInt` and not Rust's `str::parse::<f64>`:
//!   * `"0x10"` → 16 (`parse::<f64>` errors; `parseInt(s, 10)` would give 0);
//!   * `" 5 "` → 5, `"1e3"` → 1000, `"1."` → 1, `".5"` → 0.5;
//!   * `"5px"` → NaN (`parseInt` would give 5 — the leading-prefix behaviour
//!     `Number` does not have);
//!   * `"Infinity"` → Infinity, which semantic 3 then REJECTS (not finite);
//!   * `"inf"` / `"nan"` → NaN, though Rust's `parse::<f64>` accepts both
//!     spellings — hence the explicit grammar check below rather than a bare
//!     `parse`.
//!
//! ## `llmNumber` REPLACES the value — it does not merely permit it
//!
//! v4's `z.preprocess` sits inside the schema, so a successful `safeParse`
//! yields the CONVERTED value: after `{"maxResults": "3"}` parses, the handler's
//! `input.maxResults` is the number 3, not the string. So every v5 read of a
//! guarded field must go through this seam too, not just its validator — a site
//! that validates leniently and then reads with `as_i64()` would silently fall
//! back to its default. The conversion lands via [`js_number_to_json`], so an
//! integral value becomes a JSON integer (`3`, not `3.0`) exactly as
//! `JSON.stringify` renders a JS number — which also keeps `as_i64()` working at
//! the read sites.

use std::borrow::Cow;

use serde_json::Value;

use crate::db::js_number_to_json;
use crate::jsstr::js_trim;

/// v4's `llmNumber(inner)` preprocess step: the value the inner schema sees.
///
/// Returns the input untouched unless it is a string that converts to a finite
/// number, in which case it becomes that number. See the module docs for why
/// each of the three arms is what it is.
pub fn llm_number(value: &Value) -> Cow<'_, Value> {
    // 1. Non-strings fall through untouched — the deliberate divergence from
    //    z.coerce.number(), which would turn `true` into 1 and `null`/`[]` into
    //    0 (a wrong answer where a rejection was correct).
    let Value::String(s) = value else {
        return Cow::Borrowed(value);
    };

    let trimmed = js_trim(s);
    // 2. '' would become 0 under Number(); an empty string is a missing value,
    //    not a zero, so leave it for the validator to refuse.
    if trimmed.is_empty() {
        return Cow::Borrowed(value);
    }

    // 3. Number('nonsense') is NaN — hand the original back so the error message
    //    names what the model actually sent.
    let parsed = js_number_from_str(trimmed);
    if !parsed.is_finite() {
        return Cow::Borrowed(value);
    }
    Cow::Owned(js_number_to_json(parsed))
}

/// JS `Number(s)` for a string — the spec's StringToNumber.
///
/// Shared with the annotations failure-echo path, which genuinely does compute a
/// JS `Number(x)` (for its error message, NOT for validation — do not mistake it
/// for this seam). The string arm is the only subtle part of `Number()`, so it
/// is the only part worth having once; the `true`→1 / `null`→0 arms are three
/// obvious lines that belong at the one call site that needs them.
///
/// Trims JS whitespace (a wider set than Rust's — see [`crate::jsstr`]), then:
/// empty → 0; `Infinity`/`+Infinity`/`-Infinity` (case-SENSITIVE) → ±∞;
/// `0x`/`0o`/`0b` integer literals (unsigned only) → their value; a decimal
/// literal → its value; anything else → NaN.
///
/// The explicit grammar check is what keeps Rust's `parse::<f64>` from
/// accepting spellings JS rejects (`inf`, `infinity`, `nan`, `+nan`).
pub fn js_number_from_str(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.0;
    }

    // StrNumericLiteral :: Infinity — case-sensitive; `Number('infinity')` is NaN.
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }

    // NonDecimalIntegerLiteral — no sign is permitted: `Number('-0x10')` is NaN.
    for (prefix_lower, prefix_upper, radix) in
        [("0x", "0X", 16u32), ("0o", "0O", 8), ("0b", "0B", 2)]
    {
        if let Some(digits) = t
            .strip_prefix(prefix_lower)
            .or(t.strip_prefix(prefix_upper))
        {
            return parse_radix(digits, radix);
        }
    }

    if !is_js_decimal_literal(t) {
        return f64::NAN;
    }
    // The grammar above is a subset of what Rust's parser accepts, and Rust
    // rounds to nearest as the spec requires.
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// The value of a non-decimal integer literal's digits, or NaN when the digit
/// string is empty or contains a digit illegal for the radix.
///
/// Accumulates in `u128` and falls back to `f64` accumulation past its range.
/// The fallback can round differently from the spec's exact-then-round rule for
/// literals beyond ~2^128; every field this seam guards is bounded far below
/// that (the widest is `max(1000)`), so such a value is rejected by its bound
/// either way.
fn parse_radix(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    let mut acc: u128 = 0;
    let mut overflowed: Option<f64> = None;
    for c in digits.chars() {
        let Some(d) = c.to_digit(radix) else {
            return f64::NAN;
        };
        match overflowed {
            Some(v) => overflowed = Some(v * radix as f64 + d as f64),
            None => match acc
                .checked_mul(radix as u128)
                .and_then(|a| a.checked_add(d as u128))
            {
                Some(next) => acc = next,
                None => overflowed = Some(acc as f64 * radix as f64 + d as f64),
            },
        }
    }
    overflowed.unwrap_or(acc as f64)
}

/// Whether `t` is a JS `StrDecimalLiteral`:
/// `[+-]? ( digits ('.' digits?)? | '.' digits ) ([eE] [+-]? digits)?`
///
/// Rust's `parse::<f64>` also accepts `inf`, `infinity`, and `nan` (all
/// case-insensitively), which `Number()` does not; this gate excludes them.
fn is_js_decimal_literal(t: &str) -> bool {
    let b = t.as_bytes();
    let mut i = 0usize;

    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }

    let int_digits = take_digits(b, &mut i);
    let mut frac_digits = 0usize;
    let mut has_dot = false;
    if i < b.len() && b[i] == b'.' {
        has_dot = true;
        i += 1;
        frac_digits = take_digits(b, &mut i);
    }
    // Need at least one digit overall; a bare '.' or '' is not a literal. With a
    // dot, either side may be empty ('1.' and '.5' are both legal) but not both.
    if int_digits == 0 && (!has_dot || frac_digits == 0) {
        return false;
    }

    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        if take_digits(b, &mut i) == 0 {
            return false;
        }
    }

    i == b.len()
}

/// Advance `i` past ASCII digits, returning how many were consumed.
fn take_digits(b: &[u8], i: &mut usize) -> usize {
    let start = *i;
    while *i < b.len() && b[*i].is_ascii_digit() {
        *i += 1;
    }
    *i - start
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn num(v: &Value) -> Option<f64> {
        llm_number(v).as_f64()
    }

    #[test]
    fn non_strings_pass_through_untouched() {
        // THE divergence from z.coerce.number(): these must reach the validator
        // unchanged and be rejected on their merits, not silently become 1 / 0.
        for v in [json!(true), json!(false), json!(null), json!([]), json!({})] {
            assert_eq!(*llm_number(&v), v, "{v} must pass through untouched");
        }
        // A real number is already a number.
        assert_eq!(*llm_number(&json!(6)), json!(6));
        assert_eq!(*llm_number(&json!(6.5)), json!(6.5));
    }

    #[test]
    fn quoted_numbers_convert() {
        assert_eq!(num(&json!("6")), Some(6.0));
        assert_eq!(num(&json!(" 5 ")), Some(5.0));
        assert_eq!(num(&json!("1e3")), Some(1000.0));
        assert_eq!(num(&json!("-1")), Some(-1.0));
        assert_eq!(num(&json!("6.5")), Some(6.5));
        // Number(), not parseInt.
        assert_eq!(num(&json!("0x10")), Some(16.0));
        assert_eq!(num(&json!("0o17")), Some(15.0));
        assert_eq!(num(&json!("0b101")), Some(5.0));
    }

    #[test]
    fn the_original_string_comes_back_when_it_is_not_a_number() {
        // Trimmed-empty is a missing value, not a zero.
        assert_eq!(*llm_number(&json!("")), json!(""));
        assert_eq!(*llm_number(&json!("   ")), json!("   "));
        // NaN → the original, so the error names what the model sent.
        assert_eq!(*llm_number(&json!("5px")), json!("5px"));
        assert_eq!(*llm_number(&json!("nonsense")), json!("nonsense"));
        // Not finite → the original.
        assert_eq!(*llm_number(&json!("Infinity")), json!("Infinity"));
        assert_eq!(*llm_number(&json!("-Infinity")), json!("-Infinity"));
        assert_eq!(*llm_number(&json!("1e999")), json!("1e999"));
    }

    #[test]
    fn js_number_rejects_rust_only_spellings() {
        // Rust's parse::<f64> accepts all of these; JS Number() does not.
        for s in ["inf", "infinity", "INFINITY", "nan", "NaN", "+nan", "-inf"] {
            assert!(
                js_number_from_str(s).is_nan(),
                "Number({s:?}) must be NaN, not a Rust-parsed float"
            );
        }
        // But the exact JS spelling IS Infinity.
        assert_eq!(js_number_from_str("Infinity"), f64::INFINITY);
        assert_eq!(js_number_from_str("-Infinity"), f64::NEG_INFINITY);
    }

    #[test]
    fn js_number_decimal_grammar_edges() {
        assert_eq!(js_number_from_str("1."), 1.0);
        assert_eq!(js_number_from_str(".5"), 0.5);
        assert_eq!(js_number_from_str("+.5"), 0.5);
        assert_eq!(js_number_from_str(""), 0.0);
        assert!(js_number_from_str(".").is_nan());
        assert!(js_number_from_str("1e").is_nan());
        assert!(js_number_from_str("1_0").is_nan()); // separators are source-only
        assert!(js_number_from_str("- 1").is_nan());
        assert!(js_number_from_str("-0x10").is_nan()); // no sign on non-decimal
        assert!(js_number_from_str("0x").is_nan()); // no digits
        assert!(js_number_from_str("0xZZ").is_nan());
    }
}
