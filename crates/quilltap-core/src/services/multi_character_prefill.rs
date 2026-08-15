//! Multi-character turn anchoring — the `[Name]` assistant prefill switch
//! (v4 `lib/llm/multi-character-prefill.ts`, `23af7146`).
//!
//! In a multi-character chat every reply is anchored to the character whose
//! turn it is, by one of two routes:
//!
//!   - **prefill** — the request ends with an assistant message containing
//!     `[Character Name]`, so the model structurally continues only that
//!     character's line;
//!   - **prose** — an instruction is appended to the system message instead,
//!     leaving the conversation ending on a user message.
//!
//! Which route suits a profile is a property of the model on the other end,
//! not of the provider, so it lives on the connection profile as
//! `multiCharacterPrefill`. Reasons to turn it off:
//!
//!   - Anthropic 4.6+ **rejects** a request that ends with an assistant message
//!     ("This model does not support assistant message prefill"), which is why
//!     Anthropic profiles default to off.
//!   - Ollama opens a thinking model's reasoning block from the chat template
//!     at the start of the assistant turn. A prefill means the block is never
//!     opened, so `message.thinking` comes back empty however the profile's
//!     Enable Thinking box is set (v4 bug 68).
//!   - Some models visibly spend their reply working out whether `[Name]` was
//!     an instruction to them or a previous speaker's slip.
//!
//! This module is the single source of truth for both the default and the
//! resolution. **Never read the stored field directly** — a null there means
//! "never chosen" (a row older than
//! `add-profile-multi-character-prefill-field-v1`, or a profile imported from a
//! pre-4.9 bundle), and only [`profile_uses_name_prefill`] knows what that
//! means.

use serde_json::Value;

/// Providers whose models cannot take an assistant prefill at all (v4
/// `PREFILL_HOSTILE_PROVIDERS`).
const PREFILL_HOSTILE_PROVIDERS: [&str; 1] = ["ANTHROPIC"];

/// v4 `defaultMultiCharacterPrefill`: the value a newly created profile should
/// start with, given its provider. Off for Anthropic (4.6+ hard-rejects an
/// assistant tail), on everywhere else — the historic behaviour.
///
/// v4's guard is `if (!provider) return true`, which is JS falsiness: `null`,
/// `undefined` **and the empty string** all take the true branch. `None` and
/// `Some("")` both model that here — though the empty string is measurably
/// UNOBSERVABLE (mutation-tested): `"".to_uppercase()` is not in the hostile
/// set either, so both routes answer `true`. The filter stays because it is
/// what v4 wrote, not because the corpus can tell.
pub fn default_multi_character_prefill(provider: Option<&str>) -> bool {
    let Some(p) = provider.filter(|s| !s.is_empty()) else {
        return true;
    };
    let upper = p.to_uppercase();
    !PREFILL_HOSTILE_PROVIDERS.contains(&upper.as_str())
}

/// v4 `profileUsesNamePrefill`: whether this profile anchors a multi-character
/// turn with the `[Name]` prefill. An explicit stored choice always wins; a
/// null/absent one falls back to the provider default.
///
/// `stored` is the tri-state column: `Some(b)` is v4's `typeof … === 'boolean'`
/// branch, `None` covers both SQL NULL and an absent key.
pub fn profile_uses_name_prefill(provider: Option<&str>, stored: Option<bool>) -> bool {
    match stored {
        Some(b) => b,
        None => default_multi_character_prefill(provider),
    }
}

/// [`profile_uses_name_prefill`] over a net-read connection-profile object (the
/// shape the services carry). A non-boolean `multiCharacterPrefill` cell falls
/// through to the provider default exactly as v4's `typeof` guard does.
pub fn profile_uses_name_prefill_value(profile: &Value) -> bool {
    profile_uses_name_prefill(
        profile.get("provider").and_then(Value::as_str),
        profile
            .get("multiCharacterPrefill")
            .and_then(Value::as_bool),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_value_helper_reads_both_fields() {
        assert!(!profile_uses_name_prefill_value(
            &json!({ "provider": "ANTHROPIC" })
        ));
        assert!(profile_uses_name_prefill_value(&json!({
            "provider": "ANTHROPIC",
            "multiCharacterPrefill": true
        })));
        assert!(!profile_uses_name_prefill_value(&json!({
            "provider": "OPENAI",
            "multiCharacterPrefill": false
        })));
        // An explicit JSON null is "never chosen" — the provider default.
        assert!(profile_uses_name_prefill_value(&json!({
            "provider": "OPENAI",
            "multiCharacterPrefill": Value::Null
        })));
        // A non-boolean cell takes v4's `typeof` fall-through, not a coercion.
        assert!(!profile_uses_name_prefill_value(&json!({
            "provider": "ANTHROPIC",
            "multiCharacterPrefill": 1
        })));
        // No provider key at all — v4's `!provider` branch.
        assert!(profile_uses_name_prefill_value(&json!({})));
    }
}
