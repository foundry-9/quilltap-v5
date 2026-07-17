//! The JS value coercions Pascal's execution core runs on.
//!
//! `coerceParam` reaches for `Number(value)` and `String(value)`, `formatValue`
//! for `Number(v.toPrecision(4))`, and several error messages for
//! `JSON.stringify(value)`. The arguments come from a model's tool call, so the
//! exotic corners are not hypothetical: a model passing `"12"`, `""`, `true`, or
//! `[5]` all reach these functions and JS has an answer for every one.
//!
//! # Why these live under `pascal/` rather than in `jsnum.rs` / `jsstr.rs`
//!
//! [`number_to_string`] and [`to_precision`] are general JS primitives and
//! `jsnum.rs` is where they belong. They are here because `jsnum.rs` is in no
//! lane's Ownership table this round while the sibling P4.d5 lane is porting
//! `llmNumber` — a neighbouring "coerce a JS number" concern — and an undeclared
//! same-file collision is exactly what that table exists to prevent. **Lifting
//! these into `jsnum.rs` (and de-duplicating [`round_scaled_signed`] against its
//! private `round_scaled`) is a named follow-up for after the round unifies**,
//! not a permanent home.
//!
//! Note this is deliberately NOT the sibling lane's `llmNumber`: v4 keeps
//! Pascal's own `coerceParam` separate, and the two must not share an
//! implementation.

use serde_json::Value;

use crate::jsstr::js_trim;

/// v4/JS `String(value)` — `ToString` for the values a JSON tool argument can
/// hold.
pub fn to_js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) => number_to_string(f),
            None => n.to_string(),
        },
        Value::Bool(true) => "true".into(),
        Value::Bool(false) => "false".into(),
        Value::Null => "null".into(),
        // `String([1,2])` is `"1,2"`; a null/undefined element renders empty.
        Value::Array(a) => a
            .iter()
            .map(|e| match e {
                Value::Null => String::new(),
                other => to_js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// v4/JS `Number(value)` — `ToNumber`. Returns `f64::NAN` where JS yields `NaN`,
/// which the callers test with `Number.isFinite`.
pub fn to_number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::Null => 0.0,
        Value::String(s) => string_to_number(s),
        // ToPrimitive on an array is its join; `Number([])` is 0, `Number([5])`
        // is 5, `Number([1,2])` is NaN.
        Value::Array(_) => string_to_number(&to_js_string(v)),
        Value::Object(_) => f64::NAN,
    }
}

/// JS `Number(string)` — the `StringNumericLiteral` grammar, which is NOT
/// `f64::from_str`: it takes `""` as 0, accepts `Infinity` and the `0x`/`0o`/`0b`
/// radix forms, and rejects the trailing garbage `from_str` would also reject
/// but also the `1_000`/`inf`/`nan` spellings Rust accepts.
fn string_to_number(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.0;
    }

    // Rust accepts spellings JS does not; reject them before `from_str` sees them.
    let lower = t.to_ascii_lowercase();
    if lower.contains('_')
        || lower == "inf"
        || lower == "-inf"
        || lower == "+inf"
        || lower.contains("nan")
    {
        return f64::NAN;
    }

    let (sign, body) = match t.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, t.strip_prefix('+').unwrap_or(t)),
    };

    if body == "Infinity" {
        return sign * f64::INFINITY;
    }

    // The radix forms are unsigned in the grammar: `Number('-0x10')` is NaN.
    if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        return radix_to_number(hex, 16, sign);
    }
    if let Some(oct) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
        return radix_to_number(oct, 8, sign);
    }
    if let Some(bin) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
        return radix_to_number(bin, 2, sign);
    }

    // `Infinity` aside, the decimal grammar and Rust's agree on what is legal.
    t.parse::<f64>().unwrap_or(f64::NAN)
}

fn radix_to_number(digits: &str, radix: u32, sign: f64) -> f64 {
    if sign < 0.0 || digits.is_empty() {
        return f64::NAN;
    }
    match u128::from_str_radix(digits, radix) {
        Ok(v) => v as f64,
        Err(_) => f64::NAN,
    }
}

/// v4/JS `JSON.stringify(value)`, used inside several run-error sentences.
pub fn json_stringify(v: &Value) -> String {
    match v {
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.is_finite() => number_to_string(f),
            // JSON.stringify(Infinity) is "null"; unreachable from a JSON parse.
            Some(_) => "null".into(),
            None => n.to_string(),
        },
        Value::Array(a) => {
            let parts: Vec<String> = a.iter().map(json_stringify).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Object(o) => {
            let parts: Vec<String> = o
                .iter()
                .map(|(k, val)| format!("{}:{}", Value::String(k.clone()), json_stringify(val)))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        other => other.to_string(),
    }
}

/// JS `String(number)` — ECMA-262 `Number::toString`.
///
/// Rust's `{:e}` already produces the same shortest round-tripping digits V8
/// does; what differs is the FORMATTING around them, and only at the two
/// thresholds: JS switches to exponential at `>= 1e21` and below `1e-6`, where
/// Rust's `{}` would spell the number out in full.
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x == 0.0 {
        // Covers -0.0, which JS renders "0".
        return "0".into();
    }
    if x < 0.0 {
        return format!("-{}", number_to_string(-x));
    }
    if x.is_infinite() {
        return "Infinity".into();
    }

    // "1.2345e3" / "1e-7" — the shortest digits that round-trip.
    let repr = format!("{x:e}");
    let (mant, exp) = repr.split_once('e').expect("{:e} always emits an exponent");
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    // ECMA's `n`: the value is `0.digits * 10^n`.
    let n = exp.parse::<i32>().expect("{:e} emits a decimal exponent") + 1;

    if k <= n && n <= 21 {
        return format!("{digits}{}", "0".repeat((n - k) as usize));
    }
    if 0 < n && n <= 21 {
        return format!("{}.{}", &digits[..n as usize], &digits[n as usize..]);
    }
    if -6 < n && n <= 0 {
        return format!("0.{}{digits}", "0".repeat(-n as usize));
    }

    let e = n - 1;
    let sign = if e >= 0 { "+" } else { "-" };
    if k == 1 {
        format!("{digits}e{sign}{}", e.abs())
    } else {
        format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e.abs())
    }
}

/// JS `Number.prototype.toPrecision(p)` — ECMA-262 20.1.3.5.
///
/// Rounds from the f64's EXACT value, ties away from zero, which is why this
/// cannot go via the shortest repr: double-rounding would make `(2.675)` read as
/// `2.68` where V8 says `2.67` (2.675 is just under). Same reasoning, and the
/// same `u128` technique, as `jsnum::to_fixed`.
///
/// Domain: finite `x` and `2 <= p <= 21`, with `|x|` inside the range the
/// exact arithmetic covers (`10^-40 < |x| < 10^40` comfortably). `formatValue`
/// is the only caller and passes `p = 4`; it reaches this only for NON-integer
/// values, which are bounded by `2^52` since every larger double is an integer.
pub fn to_precision(x: f64, p: u32) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity" } else { "Infinity" }.into();
    }

    let negative = x < 0.0;
    let a = x.abs();

    let (digits, e) = if a == 0.0 {
        ("0".repeat(p as usize), 0i32)
    } else {
        // Estimate the decimal exponent, then correct it: a p-digit rounding can
        // carry (9.99 -> 10.0), and log10 can land a hair off at the boundaries.
        let mut e = a.log10().floor() as i32;
        let mut m = round_scaled_signed(a, p as i32 - 1 - e);
        let lo = 10u128.pow(p - 1);
        let hi = 10u128.pow(p);
        if m >= hi {
            e += 1;
            m = round_scaled_signed(a, p as i32 - 1 - e);
        } else if m < lo {
            e -= 1;
            m = round_scaled_signed(a, p as i32 - 1 - e);
        }
        (m.to_string(), e)
    };

    let body = if e < -6 || e >= p as i32 {
        let sign = if e >= 0 { "+" } else { "-" };
        let head = if p == 1 {
            digits.clone()
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        format!("{head}e{sign}{}", e.abs())
    } else if e == p as i32 - 1 {
        digits
    } else if e >= 0 {
        format!(
            "{}.{}",
            &digits[..(e + 1) as usize],
            &digits[(e + 1) as usize..]
        )
    } else {
        format!("0.{}{digits}", "0".repeat((-(e + 1)) as usize))
    };

    if negative {
        format!("-{body}")
    } else {
        body
    }
}

/// `round_half_away(a * 10^f)` as an exact integer, for finite `a > 0` and a
/// signed `f`.
///
/// The exact value of an f64 is `mantissa * 2^exp`, so `a * 10^f` is
/// `mantissa * 5^f * 2^(exp+f)` when `f >= 0`, and `mantissa * 2^(exp+f) / 5^-f`
/// when it is negative. Both stay inside `u128` across this function's
/// documented domain.
fn round_scaled_signed(a: f64, f: i32) -> u128 {
    if a == 0.0 {
        return 0;
    }
    let bits = a.to_bits();
    let exp_field = ((bits >> 52) & 0x7ff) as i64;
    let mant_field = (bits & 0x000f_ffff_ffff_ffff) as u128;
    let (mantissa, exp) = if exp_field == 0 {
        (mant_field, -1074_i64)
    } else {
        (mant_field | (1u128 << 52), exp_field - 1075)
    };

    // Fold 10^f into a numerator/denominator pair and a power of two.
    let (num, den, p) = if f >= 0 {
        (mantissa * 5u128.pow(f as u32), 1u128, exp + f as i64)
    } else {
        (mantissa, 5u128.pow((-f) as u32), exp + f as i64)
    };

    let (n, d) = if p >= 0 {
        (num << (p as u32), den)
    } else {
        let s = (-p) as u32;
        if s >= 127 {
            return 0;
        }
        (num, den << s)
    };

    let q = n / d;
    let rem = n % d;
    // Tie rounds up — away from zero, since `a > 0`.
    if (rem << 1) >= d {
        q + 1
    } else {
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_to_string_matches_js_at_the_thresholds() {
        assert_eq!(number_to_string(1.0), "1");
        assert_eq!(number_to_string(-0.0), "0");
        assert_eq!(number_to_string(0.1), "0.1");
        assert_eq!(number_to_string(100.0), "100");
        assert_eq!(number_to_string(1.5), "1.5");
        // The two places JS leaves the plain spelling behind.
        assert_eq!(number_to_string(1e21), "1e+21");
        assert_eq!(number_to_string(1e20), "100000000000000000000");
        assert_eq!(number_to_string(1e-7), "1e-7");
        assert_eq!(number_to_string(1e-6), "0.000001");
        assert_eq!(number_to_string(1.5e-7), "1.5e-7");
    }

    #[test]
    fn to_precision_rounds_from_the_exact_value() {
        // The double-rounding traps: both literals are a hair UNDER the decimal
        // they look like, and V8 rounds down accordingly.
        assert_eq!(to_precision(2.675, 3), "2.67");
        assert_eq!(to_precision(0.15, 1), "0.1");
        assert_eq!(to_precision(1.005, 3), "1.00");
        // Ordinary rounding, and the carry that forces the exponent up.
        assert_eq!(to_precision(1.2345, 4), "1.234");
        assert_eq!(to_precision(9.999, 3), "10.0");
        assert_eq!(to_precision(123.456, 4), "123.5");
        assert_eq!(to_precision(0.000123456, 4), "0.0001235");
        // Outside the fixed window, toPrecision goes exponential.
        assert_eq!(to_precision(123456.0, 4), "1.235e+5");
        assert_eq!(to_precision(0.0000001234, 4), "1.234e-7");
        assert_eq!(to_precision(0.0, 4), "0.000");
        assert_eq!(to_precision(-2.675, 3), "-2.67");
    }

    #[test]
    fn to_number_follows_the_string_numeric_grammar() {
        assert_eq!(to_number(&Value::String("".into())), 0.0);
        assert_eq!(to_number(&Value::String("  12  ".into())), 12.0);
        assert_eq!(to_number(&Value::String("0x10".into())), 16.0);
        assert_eq!(to_number(&Value::Bool(true)), 1.0);
        assert_eq!(to_number(&Value::Null), 0.0);
        assert_eq!(to_number(&serde_json::json!([])), 0.0);
        assert_eq!(to_number(&serde_json::json!([5])), 5.0);
        assert!(to_number(&serde_json::json!([1, 2])).is_nan());
        assert!(to_number(&serde_json::json!({})).is_nan());
        // Rust would take these; JS does not.
        assert!(to_number(&Value::String("1_000".into())).is_nan());
        assert!(to_number(&Value::String("inf".into())).is_nan());
        assert_eq!(to_number(&Value::String("Infinity".into())), f64::INFINITY);
    }

    #[test]
    fn to_js_string_follows_to_string() {
        assert_eq!(to_js_string(&serde_json::json!([1, 2])), "1,2");
        assert_eq!(to_js_string(&serde_json::json!({})), "[object Object]");
        assert_eq!(to_js_string(&serde_json::json!(1.0)), "1");
        assert_eq!(to_js_string(&Value::Null), "null");
    }
}
