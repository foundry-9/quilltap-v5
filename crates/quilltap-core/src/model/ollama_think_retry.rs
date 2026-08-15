//! The Ollama retry-without-`think` (P4.D78, v4 `d9c5a1c7`).
//!
//! Some models refuse the `think` request parameter outright — Ollama answers
//! non-ok with a body like `"<model>" does not support disabling thinking` on
//! models whose template it cannot drive either way. v4's plugin wraps BOTH
//! `sendMessage` and `streamMessage` in the same salvage: on a non-ok response
//! whose body mentions "think", `delete requestBody.think` and re-send ONCE,
//! then fail with the ordinary error if the retry is non-ok too. That keeps such
//! models working with the default profile instead of failing the whole call.
//!
//! v5 applies it at the transport seam (the two provider compositions), which is
//! where `!response.ok` lives here. Three conditions, each a literal reading of
//! v4's:
//!
//! - **`!response.ok`** → the transport reported a NON-2xx, i.e.
//!   [`TransportError::status`] is `Some`. A network failure, timeout or abort
//!   carries `None` and never reaches v4's branch either (there `fetch` throws
//!   past the `if (!response.ok)`).
//! - **`isThinkRejection(errorText)`** → `/think/i` over the error text. v5's
//!   transport renders a non-2xx as `HTTP <status>: <body>`; the fixed prefix
//!   cannot contain "think", so matching the whole message is the same predicate
//!   over the same bytes.
//! - **`'think' in requestBody`** → the built body still carries the key, so the
//!   retry can only ever happen once (the second body has no `think` to delete).
//!
//! The retry body is v4's `JSON.stringify` after `delete`: the same object with
//! one key removed and every other key in its original position (hence
//! `shift_remove`, not `Map::remove`, which under `preserve_order` swaps).
//!
//! This is wall-clock/IO behavior no NDJSON corpus can observe end to end (the
//! P4.15 falsifiability ruling), so it is proven at two tiers: the fake-transport
//! quartet in the two provider compositions' unit tests, and the
//! `ollama_think_retry_tier3` arm driving v4's REAL plugin with `fetch` mocked
//! below it.

use crate::model::transport::{TransportError, TransportRequest};

/// v4 `isThinkRejection`: whether an Ollama error body is complaining about the
/// `think` request parameter (`/think/i`).
pub fn is_think_rejection(error_text: &str) -> bool {
    error_text.to_lowercase().contains("think")
}

/// v4's `delete requestBody.think` + re-`JSON.stringify`. `None` when the body
/// is not a JSON object or carries no `think` key (v4's `'think' in requestBody`
/// guard — the retry is one-shot by construction).
pub fn body_without_think(body: &[u8]) -> Option<Vec<u8>> {
    let mut value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let map = value.as_object_mut()?;
    map.shift_remove("think")?;
    serde_json::to_vec(&value).ok()
}

/// The whole predicate: given the request that failed and the transport error it
/// failed with, the request to re-send ONCE — or `None` to surface the original
/// failure unchanged.
///
/// Only `OLLAMA` is eligible: v4's salvage lives in the Ollama plugin alone, and
/// no other provider puts a `think` key on its body.
pub fn think_retry_request(
    provider: &str,
    request: &TransportRequest,
    error: &TransportError,
) -> Option<TransportRequest> {
    if provider != "OLLAMA" || error.status.is_none() || !is_think_rejection(&error.message) {
        return None;
    }
    let body = body_without_think(&request.body)?;
    let mut retry = request.clone();
    retry.body = body;
    Some(retry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(body: &str) -> TransportRequest {
        TransportRequest {
            provider: "OLLAMA".into(),
            method: "POST".into(),
            url: "http://localhost:11434/api/chat".into(),
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
            api_key: String::new(),
        }
    }

    fn err(status: Option<u16>, message: &str) -> TransportError {
        TransportError {
            message: message.into(),
            status,
        }
    }

    #[test]
    fn delete_preserves_the_remaining_key_order() {
        let out = body_without_think(br#"{"model":"m","messages":[],"stream":false,"think":true,"options":{"temperature":0.7}}"#).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            r#"{"model":"m","messages":[],"stream":false,"options":{"temperature":0.7}}"#
        );
    }

    #[test]
    fn a_body_without_think_cannot_retry() {
        assert!(body_without_think(br#"{"model":"m"}"#).is_none());
    }

    #[test]
    fn rejection_predicate_is_case_insensitive() {
        assert!(is_think_rejection(
            r#"{"error":"\"qwen3:8b\" does not support disabling thinking"}"#
        ));
        assert!(is_think_rejection("Think is unsupported"));
        assert!(!is_think_rejection(
            r#"{"error":"model \"nope\" not found"}"#
        ));
    }

    #[test]
    fn only_a_non_ok_status_on_ollama_with_think_retries() {
        let with_think = req(r#"{"model":"m","think":false}"#);
        let without = req(r#"{"model":"m"}"#);
        let think_err = err(Some(400), "HTTP 400: does not support thinking");

        assert!(think_retry_request("OLLAMA", &with_think, &think_err).is_some());
        // A think-unrelated error never retries.
        assert!(think_retry_request(
            "OLLAMA",
            &with_think,
            &err(Some(404), r#"HTTP 404: model "nope" not found"#)
        )
        .is_none());
        // A network failure (no status) is not `!response.ok`.
        assert!(
            think_retry_request("OLLAMA", &with_think, &err(None, "connection refused")).is_none()
        );
        // A body already stripped of `think` is the second attempt — no third.
        assert!(think_retry_request("OLLAMA", &without, &think_err).is_none());
        // No other provider is eligible.
        assert!(think_retry_request("OPENAI", &with_think, &think_err).is_none());
    }
}
