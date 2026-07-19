//! JS number semantics — the companion to [`crate::jsstr`].
//!
//! Two residents: [`to_fixed`], reproducing `Number.prototype.toFixed` as V8
//! implements it (V8's rounding differs from Rust's float formatter in two ways
//! that matter for byte-exact equivalence, and both are handled here), and
//! [`number_from_str`], the canonical JS `Number(string)` (the spec's
//! StringToNumber) the tool-argument seams share (P4.6bd — lifted from
//! `tools::llm_number`, the most fully documented of the duplicated ports;
//! `tools::text_block_parser` and `model::tool_wire::text_parsers` consume it,
//! the latter through an explicit narrowing wrapper).

/// Format `x` with exactly `digits` fractional digits, matching V8's
/// `Number.prototype.toFixed(digits)`.
///
/// V8 rounds half **away from zero** on the f64's *exact* value — e.g.
/// `(2.5).toFixed(0) === "3"`, `(-2.5).toFixed(0) === "-3"`,
/// `(0.125).toFixed(2) === "0.13"` — which differs from Rust's formatter
/// (round half to **even**, so `format!("{:.0}", 2.5_f64)` is `"2"`). The
/// rounding is computed exactly from the IEEE-754 mantissa/exponent with `u128`
/// arithmetic, so float-representation quirks fall out for free:
/// `(0.15).toFixed(1) === "0.1"` (0.15 is just under), `(2.675).toFixed(2) ===
/// "2.67"` (2.675 is just under).
///
/// The sign is emitted iff `x < 0` strictly, so a magnitude that rounds to zero
/// keeps its minus (`(-0.004).toFixed(2) === "-0.00"`) while `-0.0` does not
/// (`(-0).toFixed(2) === "0.00"`).
///
/// Domain: finite `x` with `|x| < 1e21` and `digits <= 19` — the range the v4
/// display formatters use (they pass `digits ∈ 0..=4`). V8 special-cases
/// `|x| >= 1e21` by switching to `Number.toString`; that is outside this domain
/// and reached by no call site. `NaN`/`±Infinity` map to `"NaN"`/`"Infinity"`/
/// `"-Infinity"` as V8 does.
pub fn to_fixed(x: f64, digits: u32) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
    }

    let negative = x < 0.0;
    let m = round_scaled(x.abs(), digits);

    // Render the integer `m` with a decimal point `digits` places from the right.
    let mut s = m.to_string();
    let body = if digits == 0 {
        s
    } else {
        let d = digits as usize;
        if s.len() <= d {
            // Left-pad to d+1 chars so there is a leading integer digit ("0").
            s = format!("{}{s}", "0".repeat(d + 1 - s.len()));
        }
        let point = s.len() - d;
        format!("{}.{}", &s[..point], &s[point..])
    };

    if negative {
        format!("-{body}")
    } else {
        body
    }
}

/// `round_half_away(a * 10^f)` as an exact integer, for finite `a >= 0`.
///
/// The exact value of a normal f64 is `mantissa * 2^exp` with a 53-bit mantissa,
/// so `a * 10^f = mantissa * 5^f * 2^(exp + f)`. When the power of two is
/// non-negative the product is an exact integer; otherwise we divide by
/// `2^s` and round the remainder, ties going up (which is "away from zero" for
/// the non-negative `a`). `mantissa < 2^53` and `5^f < 2^45` for `f <= 19`, so
/// the numerator stays within `u128`.
fn round_scaled(a: f64, f: u32) -> u128 {
    if a == 0.0 {
        return 0;
    }
    let bits = a.to_bits();
    let exp_field = ((bits >> 52) & 0x7ff) as i64;
    let mant_field = (bits & 0x000f_ffff_ffff_ffff) as u128;
    let (mantissa, exp) = if exp_field == 0 {
        // Subnormal: value = mant_field * 2^(-1074). (Never reached by the
        // formatters; handled for totality.)
        (mant_field, -1074_i64)
    } else {
        // Normal: value = (mant_field | 1<<52) * 2^(exp_field - 1075).
        (mant_field | (1u128 << 52), exp_field - 1075)
    };

    let n = mantissa * 5u128.pow(f); // exact; within u128 for f <= 19
    let p = exp + f as i64;
    if p >= 0 {
        n << (p as u32)
    } else {
        let s = (-p) as u32;
        if s >= 128 {
            // 2^s exceeds both the numerator and u128: quotient 0, and 2*n is
            // still below 2^s, so it rounds down to 0.
            return 0;
        }
        let divisor = 1u128 << s;
        let q = n >> s;
        let rem = n & (divisor - 1);
        // Tie (rem*2 == divisor) rounds up — away from zero for a >= 0.
        if (rem << 1) >= divisor {
            q + 1
        } else {
            q
        }
    }
}

/// JS `Number(s)` for a string — the spec's StringToNumber. The CANONICAL copy
/// (P4.6bd): `tools::llm_number` re-exports it as `js_number_from_str`, and the
/// tool-wire text parsers wrap it.
///
/// Shared with the annotations failure-echo path, which genuinely does compute a
/// JS `Number(x)` (for its error message, NOT for validation — do not mistake it
/// for the lenient-number seam). The string arm is the only subtle part of
/// `Number()`, so it is the only part worth having once; the `true`→1 / `null`→0
/// arms are three obvious lines that belong at the one call site that needs
/// them.
///
/// Trims JS whitespace (a wider set than Rust's — see [`crate::jsstr`]), then:
/// empty → 0; `Infinity`/`+Infinity`/`-Infinity` (case-SENSITIVE) → ±∞;
/// `0x`/`0o`/`0b` integer literals (unsigned only) → their value; a decimal
/// literal → its value; anything else → NaN.
///
/// The explicit grammar check is what keeps Rust's `parse::<f64>` from
/// accepting spellings JS rejects (`inf`, `infinity`, `nan`, `+nan`).
pub fn number_from_str(s: &str) -> f64 {
    let t = crate::jsstr::js_trim(s);
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
/// literals beyond ~2^128; every field the lenient-number seam guards is bounded
/// far below that (the widest is `max(1000)`), so such a value is rejected by
/// its bound either way.
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
