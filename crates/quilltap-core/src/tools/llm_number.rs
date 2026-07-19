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

/// JS `Number(s)` for a string — the spec's StringToNumber. Since P4.6bd the
/// implementation is the canonical [`crate::jsnum::number_from_str`] (this
/// module was the source it was lifted from); the alias stays because this
/// name is what the 28-field lenient-number surface and the annotations
/// failure-echo path import.
pub use crate::jsnum::number_from_str as js_number_from_str;

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
