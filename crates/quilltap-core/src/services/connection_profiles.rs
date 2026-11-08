//! The per-profile image cap's accessors — P4.D362 (v4 `d439f6dc9`,
//! `lib/llm/connection-profile-utils.ts:154` / `:169` and the two constants of
//! `lib/schemas/profile.types.ts:43-45`).
//!
//! `maxImagesPerRequest` is the most images a vision-capable profile is sent in
//! one request (some models refuse more than four outright). v4 reads it ONLY
//! through `resolveMaxImagesPerRequest`, and validates a request body's value
//! through `parseMaxImagesPerRequest`; these are their twins. The callers that
//! enforce the cap (the image transport budget, the Lantern walk, the
//! failover re-adapt) are P4.D367's.

use serde_json::Value;

/// v4 `DEFAULT_MAX_IMAGES_PER_REQUEST` — images sent to a vision profile in
/// one request when it names no cap of its own.
pub const DEFAULT_MAX_IMAGES_PER_REQUEST: i64 = 4;
/// v4 `MAX_IMAGES_PER_REQUEST_CEILING` — the highest cap a profile may store.
pub const MAX_IMAGES_PER_REQUEST_CEILING: i64 = 50;

/// The JS `Number.isInteger(n) && 1 <= n <= 50` test, on an `f64`.
fn in_range_integer(n: f64) -> Option<i64> {
    if n.is_finite()
        && n.fract() == 0.0
        && (1.0..=MAX_IMAGES_PER_REQUEST_CEILING as f64).contains(&n)
    {
        Some(n as i64)
    } else {
        None
    }
}

/// v4 `resolveMaxImagesPerRequest(profile)`: the profile's
/// `maxImagesPerRequest` when it is a JSON NUMBER holding an integer in
/// `1..=50`; otherwise (absent, `null`, a string, a fraction, out of range — a
/// row or bundle older than the column, a hand-edited archive) the default
/// `4`, never "no cap", since an uncapped request is what some models refuse.
pub fn resolve_max_images_per_request(profile: &Value) -> i64 {
    profile
        .get("maxImagesPerRequest")
        .and_then(Value::as_f64)
        .and_then(in_range_integer)
        .unwrap_or(DEFAULT_MAX_IMAGES_PER_REQUEST)
}

/// v4 `parseMaxImagesPerRequest(value)`: a non-blank string goes through JS
/// `Number(value)` (`"0x10"` → 16, `" 4 "` → 4, `"4e0"` → 4); then the result
/// must be a number that is an integer in `1..=50`, else `None`. A blank
/// string, a boolean, an array, an object or `null` is `None`.
pub fn parse_max_images_per_request(value: &Value) -> Option<i64> {
    let n = match value {
        Value::String(s) if !crate::jsstr::js_trim(s).is_empty() => {
            crate::jsnum::number_from_str(s)
        }
        Value::Number(n) => n.as_f64()?,
        _ => return None,
    };
    in_range_integer(n)
}

/// v4 `ConnectionProfileSchema.maxImagesPerRequest`
/// (`z.number().int().min(1).max(50).default(4)`) over a SEEDED record value —
/// the `.qtap` import and the backup restore hand the seeded value to v4's
/// create, whose Zod parse REFUSES the whole profile on anything else. `Ok` is
/// the value to write; `Err` is v4's `ZodError.message` (the pretty issue
/// array — measured at the pin for `null`, `0`, `51`, `"4"`, `4.5`).
pub fn validate_max_images_per_request(value: &Value) -> Result<i64, String> {
    use crate::api::zod_issues::{zod_error_message, ZodIssue};
    let path = vec![Value::from("maxImagesPerRequest")];
    let issue = match value {
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(f64::NAN);
            if !(f.is_finite() && f.fract() == 0.0) {
                ZodIssue::invalid_int_type(path, Some(value))
            } else if f < 1.0 {
                ZodIssue::too_small_number(Value::from(1), path)
            } else if f > MAX_IMAGES_PER_REQUEST_CEILING as f64 {
                ZodIssue::too_big_number(Value::from(MAX_IMAGES_PER_REQUEST_CEILING), path)
            } else {
                return Ok(f as i64);
            }
        }
        other => ZodIssue::invalid_type("number", path, Some(other)),
    };
    Err(zod_error_message(&[issue]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolve_reads_only_in_range_integers() {
        for (v, want) in [
            (json!({}), 4),
            (json!({"maxImagesPerRequest": null}), 4),
            (json!({"maxImagesPerRequest": 1}), 1),
            (json!({"maxImagesPerRequest": 50}), 50),
            (json!({"maxImagesPerRequest": 51}), 4),
            (json!({"maxImagesPerRequest": 4.5}), 4),
            (json!({"maxImagesPerRequest": 6.0}), 6),
            (json!({"maxImagesPerRequest": "6"}), 4),
        ] {
            assert_eq!(resolve_max_images_per_request(&v), want, "{v}");
        }
    }

    #[test]
    fn validate_matches_v4s_zod_messages() {
        assert_eq!(validate_max_images_per_request(&json!(4)), Ok(4));
        let msg = validate_max_images_per_request(&json!(0)).unwrap_err();
        assert!(
            msg.contains("Too small: expected number to be >=1"),
            "{msg}"
        );
        let msg = validate_max_images_per_request(&json!(4.5)).unwrap_err();
        assert!(msg.contains("expected int, received number"), "{msg}");
        let msg = validate_max_images_per_request(&json!(null)).unwrap_err();
        assert!(msg.contains("expected number, received null"), "{msg}");
    }

    #[test]
    fn parse_follows_js_number() {
        for (v, want) in [
            (json!("0x10"), Some(16)),
            (json!(" 4 "), Some(4)),
            (json!("4e0"), Some(4)),
            (json!(""), None),
            (json!("  "), None),
            (json!("x"), None),
            (json!(true), None),
            (json!([4]), None),
            (json!(0), None),
            (json!(null), None),
        ] {
            assert_eq!(parse_max_images_per_request(&v), want, "{v}");
        }
    }
}
