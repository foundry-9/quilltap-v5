//! Port of v4's `lib/llm/extract-finish-reason.ts` — best-effort extraction of a
//! provider's reported finish reason from the raw response object on the final
//! `done` stream chunk.
//!
//! The shape differs per provider; we sniff the well-known ones and fall back to
//! `None` for anything unrecognized. The result is the raw provider string —
//! callers must not assume any normalization.
//!
//! The input is an arbitrary JSON value (v4's `unknown`). Faithful checks:
//!   * only a non-null JSON **object** is inspected (JS `typeof raw === 'object'`
//!     excludes primitives; `null` is excluded explicitly);
//!   * each candidate field must be a JSON **string** to be returned (JS
//!     `typeof … === 'string'`), otherwise the search continues;
//!   * provider probes run in v4's exact order — OpenAI-style `choices[0]
//!     .finish_reason`, Anthropic `stop_reason`, Google `candidates[0]
//!     .finishReason`, then the Responses-API `status`.

use serde_json::Value;

/// Extract a provider's finish reason from a raw JSON response value, or `None`.
///
/// Mirrors v4's `extractFinishReason(raw: unknown): string | null`.
pub fn extract_finish_reason(raw: &Value) -> Option<String> {
    // `!raw || typeof raw !== 'object'` — only inspect a JSON object. In JS an
    // array is also `typeof 'object'`, but the field lookups below never hit on
    // a bare array, so treating only maps as objects is observationally
    // identical here.
    let obj = raw.as_object()?;

    // OpenAI Chat Completions, Z.AI, OpenRouter chat, OpenAI-compatible:
    // choices[0].finish_reason (a string).
    if let Some(choices) = obj.get("choices").and_then(Value::as_array) {
        if let Some(first) = choices.first() {
            if let Some(fr) = first.get("finish_reason").and_then(Value::as_str) {
                return Some(fr.to_string());
            }
        }
    }

    // Anthropic: stop_reason (a string).
    if let Some(sr) = obj.get("stop_reason").and_then(Value::as_str) {
        return Some(sr.to_string());
    }

    // Google: candidates[0].finishReason (a string).
    if let Some(candidates) = obj.get("candidates").and_then(Value::as_array) {
        if let Some(first) = candidates.first() {
            if let Some(fr) = first.get("finishReason").and_then(Value::as_str) {
                return Some(fr.to_string());
            }
        }
    }

    // OpenAI Responses API, Grok Responses: status (a string).
    if let Some(status) = obj.get("status").and_then(Value::as_str) {
        return Some(status.to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openai_choices() {
        assert_eq!(
            extract_finish_reason(&json!({"choices": [{"finish_reason": "stop"}]})),
            Some("stop".to_string())
        );
    }

    #[test]
    fn anthropic_stop_reason() {
        assert_eq!(
            extract_finish_reason(&json!({"stop_reason": "end_turn"})),
            Some("end_turn".to_string())
        );
    }

    #[test]
    fn google_candidates() {
        assert_eq!(
            extract_finish_reason(&json!({"candidates": [{"finishReason": "STOP"}]})),
            Some("STOP".to_string())
        );
    }

    #[test]
    fn responses_status() {
        assert_eq!(
            extract_finish_reason(&json!({"status": "completed"})),
            Some("completed".to_string())
        );
    }

    #[test]
    fn unrecognized_and_non_object() {
        assert_eq!(extract_finish_reason(&json!(null)), None);
        assert_eq!(extract_finish_reason(&json!("stop")), None);
        assert_eq!(extract_finish_reason(&json!({"foo": "bar"})), None);
        // non-string finish_reason is ignored, falls through
        assert_eq!(
            extract_finish_reason(&json!({"choices": [{"finish_reason": 3}]})),
            None
        );
    }
}
