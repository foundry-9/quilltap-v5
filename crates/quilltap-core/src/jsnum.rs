//! JS number-formatting semantics — the companion to [`crate::jsstr`].
//!
//! Currently this is [`to_fixed`], reproducing `Number.prototype.toFixed` as V8
//! implements it. V8's rounding differs from Rust's float formatter in two ways
//! that matter for byte-exact equivalence, and both are handled here.

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
