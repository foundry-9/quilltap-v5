//! Sampling knobs from a connection profile's `parameters` blob (v4
//! `lib/llm/sampling-params.ts`, `d89babc4`).
//!
//! The profile editor stores the three shared sampling controls under snake_case
//! keys — `temperature`, `max_tokens`, `top_p` — while `LLMParams` names them
//! `temperature` / `maxTokens` / `topP`. Call sites that spread the blob straight
//! onto an `LLMParams` object therefore silently drop two of the three:
//! `parameters.maxTokens` and `parameters.topP` do not exist, so the provider
//! falls back to its own defaults and the profile's Max Tokens and Top P are
//! never sent. This module is the one place that bridges the two spellings, so a
//! profile means the same thing on every path that reads it.
//!
//! Not to be confused with
//! [`resolve_max_tokens`](crate::context_budget::resolve_max_tokens) (v4
//! `resolveMaxTokens` in model-context-data.ts): that one answers "how much
//! output should the *context budget* reserve" and deliberately ignores
//! `parameters.max_tokens`. This one is the per-request generation cap that goes
//! on the wire.

use serde_json::Value;

/// The sampling knobs an `LLMParams` object expects, in its own spelling
/// (v4 `SamplingParams`). Each field is `None` exactly when the profile set no
/// usable value for it — nothing here invents a number.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SamplingParams {
    pub temperature: Option<f64>,
    pub max_tokens: Option<f64>,
    pub top_p: Option<f64>,
}

/// Read one numeric knob under its canonical snake_case key, tolerating the
/// camelCase spelling in case a profile was seeded or imported with one
/// (v4 `readNumber`).
///
/// Strings are accepted because JSON blobs edited by hand often carry them, and
/// they go through JS `Number(value)` semantics (v5
/// [`jsnum::number_from_str`](crate::jsnum::number_from_str)) rather than Rust's
/// stricter `parse` — `" 8192 "` and `"0x20"` are numbers to v4.
///
/// A key whose value is present but NOT finite (a bare object, `null`, `"warm"`,
/// `Infinity`) does not stop the search: v4 keeps walking the remaining
/// spellings, so a blob carrying `max_tokens: "warm"` alongside a good
/// `maxTokens` still resolves.
fn read_number(params: Option<&Value>, keys: &[&str]) -> Option<f64> {
    let obj = params?;
    for key in keys {
        let n = match obj.get(*key) {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
            Some(Value::String(s)) => crate::jsnum::number_from_str(s),
            _ => f64::NAN,
        };
        if n.is_finite() {
            return Some(n);
        }
    }
    None
}

/// Extract `temperature` / `max_tokens` / `top_p` from a profile's parameters
/// blob in the shape the per-request params want (v4 `resolveSamplingParams`).
///
/// Absent knobs come back `None` so the caller — or the provider — keeps
/// whatever default it had.
///
/// A non-object blob (v4's `params` typed as a record but reachable with an
/// array or a scalar through an unchecked cast) yields nothing: `Value::get`
/// answers `None` for every string key on a non-object, which is exactly what
/// JS property access on a number or a string does here.
pub fn resolve_sampling_params(params: Option<&Value>) -> SamplingParams {
    SamplingParams {
        temperature: read_number(params, &["temperature"]),
        max_tokens: read_number(params, &["max_tokens", "maxTokens"]),
        top_p: read_number(params, &["top_p", "topP"]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn r(v: Value) -> SamplingParams {
        resolve_sampling_params(Some(&v))
    }

    #[test]
    fn reads_the_snake_case_keys_the_profile_editor_writes() {
        assert_eq!(
            r(json!({ "temperature": 1, "max_tokens": 16384, "top_p": 0.95 })),
            SamplingParams {
                temperature: Some(1.0),
                max_tokens: Some(16384.0),
                top_p: Some(0.95),
            }
        );
    }

    #[test]
    fn tolerates_camel_case_on_an_imported_blob() {
        assert_eq!(
            r(json!({ "temperature": 0.7, "maxTokens": 4096, "topP": 0.9 })),
            SamplingParams {
                temperature: Some(0.7),
                max_tokens: Some(4096.0),
                top_p: Some(0.9),
            }
        );
    }

    #[test]
    fn prefers_the_canonical_snake_case_key_when_both_are_present() {
        assert_eq!(
            r(json!({ "max_tokens": 16384, "maxTokens": 4096, "top_p": 0.95, "topP": 0.5 })),
            SamplingParams {
                temperature: None,
                max_tokens: Some(16384.0),
                top_p: Some(0.95),
            }
        );
    }

    #[test]
    fn falls_through_an_unparseable_canonical_key_to_the_camel_case_one() {
        // v4's loop `continue`s past a non-finite value rather than giving up.
        assert_eq!(
            r(json!({ "max_tokens": "warm", "maxTokens": 4096 })).max_tokens,
            Some(4096.0)
        );
    }

    #[test]
    fn parses_numeric_strings() {
        assert_eq!(
            r(json!({ "temperature": "0.8", "max_tokens": "8192", "top_p": "1" })),
            SamplingParams {
                temperature: Some(0.8),
                max_tokens: Some(8192.0),
                top_p: Some(1.0),
            }
        );
    }

    #[test]
    fn leaves_absent_knobs_unset() {
        assert_eq!(
            r(json!({ "temperature": 0.7 })),
            SamplingParams {
                temperature: Some(0.7),
                max_tokens: None,
                top_p: None,
            }
        );
        assert_eq!(r(json!({})), SamplingParams::default());
        assert_eq!(resolve_sampling_params(None), SamplingParams::default());
    }

    #[test]
    fn ignores_unparseable_values() {
        assert_eq!(
            r(json!({ "temperature": "warm", "max_tokens": null, "top_p": {} })),
            SamplingParams::default()
        );
    }

    #[test]
    fn keeps_a_deliberate_zero() {
        assert_eq!(
            r(json!({ "temperature": 0, "top_p": 0 })),
            SamplingParams {
                temperature: Some(0.0),
                max_tokens: None,
                top_p: Some(0.0),
            }
        );
    }

    #[test]
    fn ignores_the_non_sampling_keys_that_share_the_blob() {
        assert_eq!(
            r(json!({
                "temperature": 1,
                "max_tokens": 16384,
                "top_p": 0.95,
                "enable_thinking": true,
                "request_timeout_seconds": 900,
                "num_ctx": 65536,
            })),
            SamplingParams {
                temperature: Some(1.0),
                max_tokens: Some(16384.0),
                top_p: Some(0.95),
            }
        );
    }
}
