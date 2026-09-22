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

/// v4 zod 4's issue objects — the ONE home ([`crate::api::zod_issues::ZodIssue`],
/// P4.101).
///
/// This module rendered its own three variants until P4.101 measured them
/// against the home's (and against the checkout's real `zod`, 4.5.4 then and
/// 4.6.5 since P4.D211, whose re-measurement moved no issue byte): the key
/// order is IDENTICAL, which it must be — v4's LoRA envelope is
/// `loraSchema.safeParse`'s own issues, the same Zod as every other route's, so
/// a difference would have been a finding. The alias keeps the local spelling
/// `validate_profile_loras`' callers already name.
pub use crate::api::zod_issues::ZodIssue as LoraZodIssue;

use crate::api::zod_issues::{key, zod_issue_lines, ZodIssue};

/// `z.array(ImageLoraSpecSchema)` over the raw `loras` value. Zod collects
/// every element's issues rather than stopping at the first, and each element's
/// object check collects every key's — so a two-bad-entry list answers with two
/// issues, in element order.
fn parse_lora_list(raw: &Value) -> Vec<LoraZodIssue> {
    let Some(list) = raw.as_array() else {
        return vec![ZodIssue::invalid_type("array", Vec::new(), Some(raw))];
    };
    let mut issues = Vec::new();
    for (i, entry) in list.iter().enumerate() {
        let idx = json!(i);
        let Some(obj) = entry.as_object() else {
            issues.push(ZodIssue::invalid_type("object", vec![idx], Some(entry)));
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
    let at = |k: &str| vec![idx.clone(), key(k)];

    // source: z.string().trim().min(1, 'LoRA source is required')
    match obj.get("source") {
        Some(Value::String(s)) => {
            // `.trim()` is a transform that runs BEFORE `.min(1)`, so a
            // whitespace-only source is a length failure, not a type one.
            if crate::jsstr::js_trim(s).is_empty() {
                issues.push(ZodIssue::too_small_string_message(
                    json!(1),
                    at("source"),
                    "LoRA source is required",
                ));
            }
        }
        other => issues.push(ZodIssue::invalid_type("string", at("source"), other)),
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
                issues.push(ZodIssue::too_small_number_message(
                    json!(0),
                    at("scale"),
                    "LoRA scale cannot be negative",
                ));
            } else if v > 10.0 {
                issues.push(ZodIssue::too_big_number_message(
                    json!(10),
                    at("scale"),
                    "LoRA scale cannot exceed 10",
                ));
            }
        }
        // An explicit `undefined` cannot reach here through JSON; an explicit
        // null is a type failure, not an omission.
        Some(other) => issues.push(ZodIssue::invalid_type("number", at("scale"), Some(other))),
    }

    for k in ["triggerPhrase", "label"] {
        match obj.get(k) {
            None | Some(Value::String(_)) => {}
            Some(other) => issues.push(ZodIssue::invalid_type("string", at(k), Some(other))),
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
///
/// Kept as this file's callers' spelling; the home
/// ([`crate::api::zod_issues::zod_issue_details`]) owns the bytes (P4.101).
pub fn lora_issue_details(issues: &[LoraZodIssue]) -> Value {
    crate::api::zod_issues::zod_issue_details(issues)
}

/// The joined `path: message` strings v4 puts in the warn line's `issues`
/// field (`loraError.issues.map(i => \`${i.path.join('.')}: ${i.message}\`)`).
/// A log-only projection; the response never carries it.
pub fn lora_issue_log_lines(issues: &[LoraZodIssue]) -> Vec<String> {
    zod_issue_lines(issues, ": ")
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
