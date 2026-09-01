//! Write-side guard for the `parameters.loras` key on an image profile
//! (v4 `lib/image-gen/lora-validation.ts`, `84f33ce94`).
//!
//! `parameters` is an opaque JSON bag, which is exactly what makes LoRAs fit
//! without a migration — and exactly why nothing else would notice a malformed
//! list going in. This validates before the write, so a bad list is a 400 with
//! nothing stored, never a profile that saves cleanly and then fails at
//! generation time (the P4.55 / P4.D120 guard-order lesson).
//!
//! Bounds here are deliberately global and permissive: per-model caps and scale
//! ranges belong to the editor and the plugin, and a profile may legitimately
//! be edited before a model is chosen. **There is no cap check on the write
//! path** — an over-cap list is kept by the ABSENCE of a guard, so narrowing
//! the model and widening it again loses nothing. Do not add one "for
//! symmetry"; v4's own e2e narrowing case pins both rows surviving.
//!
//! The refusal body is v4's Zod ENVELOPE — `validationError(err)` →
//! `{ error: 'Validation error', details: err.issues }` at 400 — never a
//! bespoke sentence, so the issues are reproduced object-for-object here
//! (the `api::settings` `ZodIssue` precedent, widened for numeric array
//! indices in `path`).

use serde_json::{json, Map, Value};

/// The reserved key under which the adapter list lives inside `parameters`
/// (v4 `IMAGE_PROFILE_LORAS_KEY`).
pub const IMAGE_PROFILE_LORAS_KEY: &str = "loras";

/// v4 zod 4's issue objects, in the key order `JSON.stringify` emits. Untagged
/// variants rather than one struct with optional keys: `Option` skipping cannot
/// reorder, and `invalid_type` puts `expected` first while the size issues put
/// `origin` first.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum LoraZodIssue {
    InvalidType {
        expected: &'static str,
        code: &'static str,
        path: Vec<Value>,
        message: String,
    },
    TooSmall {
        origin: &'static str,
        code: &'static str,
        minimum: Value,
        inclusive: bool,
        path: Vec<Value>,
        message: &'static str,
    },
    TooBig {
        origin: &'static str,
        code: &'static str,
        maximum: Value,
        inclusive: bool,
        path: Vec<Value>,
        message: &'static str,
    },
}

/// v4 `util.parsedType` — the received-type word in an `invalid_type` message.
fn parsed_type(v: Option<&Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

fn invalid_type(expected: &'static str, path: Vec<Value>, got: Option<&Value>) -> LoraZodIssue {
    LoraZodIssue::InvalidType {
        expected,
        code: "invalid_type",
        path,
        message: format!(
            "Invalid input: expected {expected}, received {}",
            parsed_type(got)
        ),
    }
}

/// `z.array(ImageLoraSpecSchema)` over the raw `loras` value. Zod collects
/// every element's issues rather than stopping at the first, and each element's
/// object check collects every key's — so a two-bad-entry list answers with two
/// issues, in element order.
fn parse_lora_list(raw: &Value) -> Vec<LoraZodIssue> {
    let Some(list) = raw.as_array() else {
        return vec![invalid_type("array", Vec::new(), Some(raw))];
    };
    let mut issues = Vec::new();
    for (i, entry) in list.iter().enumerate() {
        let idx = json!(i);
        let Some(obj) = entry.as_object() else {
            issues.push(invalid_type("object", vec![idx], Some(entry)));
            continue;
        };
        parse_lora_spec(obj, &idx, &mut issues);
    }
    issues
}

/// One `ImageLoraSpecSchema` element. Key order is the schema's declaration
/// order — `source`, `scale`, `triggerPhrase`, `label` — which is also the
/// order Zod reports issues in.
fn parse_lora_spec(obj: &Map<String, Value>, idx: &Value, issues: &mut Vec<LoraZodIssue>) {
    let at = |key: &str| vec![idx.clone(), Value::String(key.to_string())];

    // source: z.string().trim().min(1, 'LoRA source is required')
    match obj.get("source") {
        Some(Value::String(s)) => {
            // `.trim()` is a transform that runs BEFORE `.min(1)`, so a
            // whitespace-only source is a length failure, not a type one.
            if crate::jsstr::js_trim(s).is_empty() {
                issues.push(LoraZodIssue::TooSmall {
                    origin: "string",
                    code: "too_small",
                    minimum: json!(1),
                    inclusive: true,
                    path: at("source"),
                    message: "LoRA source is required",
                });
            }
        }
        other => issues.push(invalid_type("string", at("source"), other)),
    }

    // scale: z.number().finite(...).min(0, …).max(10, …).optional()
    //
    // `.finite()` has no reachable arm through JSON: neither NaN nor Infinity
    // survives `JSON.parse`, and zod 4's `z.number()` rejects them anyway. The
    // two bound checks are independent — a value can only miss one.
    match obj.get("scale") {
        None => {}
        Some(Value::Number(n)) => {
            let v = n.as_f64().unwrap_or(f64::NAN);
            if v < 0.0 {
                issues.push(LoraZodIssue::TooSmall {
                    origin: "number",
                    code: "too_small",
                    minimum: json!(0),
                    inclusive: true,
                    path: at("scale"),
                    message: "LoRA scale cannot be negative",
                });
            } else if v > 10.0 {
                issues.push(LoraZodIssue::TooBig {
                    origin: "number",
                    code: "too_big",
                    maximum: json!(10),
                    inclusive: true,
                    path: at("scale"),
                    message: "LoRA scale cannot exceed 10",
                });
            }
        }
        // An explicit `undefined` cannot reach here through JSON; an explicit
        // null is a type failure, not an omission.
        Some(other) => issues.push(invalid_type("number", at("scale"), Some(other))),
    }

    for key in ["triggerPhrase", "label"] {
        match obj.get(key) {
            None | Some(Value::String(_)) => {}
            Some(other) => issues.push(invalid_type("string", at(key), Some(other))),
        }
    }
}

/// Validate the `loras` key of an incoming `parameters` bag
/// (v4 `validateProfileLoras`).
///
/// `None` when there is nothing to complain about — the bag is not an object
/// (**the caller's own "parameters must be an object" check owns that**, so
/// this must stay AFTER it), the key is absent, or every entry parses.
/// `Some(issues)` otherwise, for the caller to hand to the Zod envelope.
pub fn validate_profile_loras(parameters: &Value) -> Option<Vec<LoraZodIssue>> {
    // v4 `typeof parameters !== 'object' || parameters === null ||
    // Array.isArray(parameters)`.
    let obj = parameters.as_object()?;
    let raw = obj.get(IMAGE_PROFILE_LORAS_KEY)?;
    let issues = parse_lora_list(raw);
    if issues.is_empty() {
        None
    } else {
        Some(issues)
    }
}

/// The `details` array of v4's `validationError(err)` body, ready for the
/// [`crate::api::types::Response`] carry.
pub fn lora_issue_details(issues: &[LoraZodIssue]) -> Value {
    serde_json::to_value(issues).unwrap_or(Value::Null)
}

/// The joined `path: message` strings v4 puts in the warn line's `issues`
/// field (`loraError.issues.map(i => \`${i.path.join('.')}: ${i.message}\`)`).
/// A log-only projection; the response never carries it.
pub fn lora_issue_log_lines(issues: &[LoraZodIssue]) -> Vec<String> {
    issues
        .iter()
        .map(|i| {
            let (path, message) = match i {
                LoraZodIssue::InvalidType { path, message, .. } => (path, message.as_str()),
                LoraZodIssue::TooSmall { path, message, .. } => (path, *message),
                LoraZodIssue::TooBig { path, message, .. } => (path, *message),
            };
            // `Array.prototype.join` stringifies each element; the numeric
            // indices render bare.
            let joined = path
                .iter()
                .map(|p| match p {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join(".");
            format!("{joined}: {message}")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_object_and_absent_key_are_silent() {
        assert!(validate_profile_loras(&json!(null)).is_none());
        assert!(validate_profile_loras(&json!([1, 2])).is_none());
        assert!(validate_profile_loras(&json!("x")).is_none());
        assert!(validate_profile_loras(&json!({})).is_none());
        assert!(validate_profile_loras(&json!({ "quality": "hd" })).is_none());
    }

    #[test]
    fn a_well_formed_list_passes_and_over_cap_is_not_checked() {
        let bag = json!({ "loras": [
            { "source": "a/1" }, { "source": "a/2" }, { "source": "a/3" }, { "source": "a/4" },
            { "source": "a/5", "scale": 10, "triggerPhrase": "m", "label": "L" },
        ]});
        assert!(validate_profile_loras(&bag).is_none());
    }

    #[test]
    fn log_lines_render_numeric_indices_bare() {
        let issues = validate_profile_loras(&json!({ "loras": [{ "source": "" }] })).unwrap();
        assert_eq!(
            lora_issue_log_lines(&issues),
            vec!["0.source: LoRA source is required"]
        );
    }
}
