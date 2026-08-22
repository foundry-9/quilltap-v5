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
//!     Anthropic profiles default to off. This one really is a property of the
//!     provider: it holds whether or not thinking is on.
//!   - A model that will run a **thinking** turn. Two providers are on record
//!     breaking on a prefill only while thinking — Ollama never opens the
//!     reasoning block behind a prefilled turn, so `message.thinking` comes
//!     back empty (v4 bug 68), and DeepSeek 400s demanding the
//!     `reasoning_content` that produced an assistant turn a synthetic prefill
//!     never had (v4 bug 85). They are also the population that needs the
//!     anchor least: a model spending tokens working out whose turn it is does
//!     not need `[Name]` put in its mouth. The question is asked per profile
//!     through [`crate::services::thinking_turn`], not per provider, so a
//!     thinking-off Ollama or DeepSeek profile keeps the stronger anchor.
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

/// Providers whose models cannot take an assistant prefill at all, thinking or
/// not (v4 `PREFILL_HOSTILE_PROVIDERS`). Anthropic is the only genuine member:
/// 4.6+ hard-rejects an assistant tail. Resist adding a provider here because
/// one of its *models* misbehaves — that is what `runs_thinking_turn` is for
/// (v4 bug 85).
const PREFILL_HOSTILE_PROVIDERS: [&str; 1] = ["ANTHROPIC"];

/// v4 `defaultMultiCharacterPrefill`: the value a newly created profile should
/// start with. Off for Anthropic (4.6+ hard-rejects an assistant tail) and off
/// for any profile that will run a thinking turn (v4 bugs 68 and 85); on
/// everywhere else — the historic behaviour and the stronger anchor for the
/// weak models that need one.
///
/// `runs_thinking_turn` is the answer from
/// [`crate::services::thinking_turn::profile_runs_thinking_turn`] — the caller
/// supplies it because working it out needs the provider registry (in v4, the
/// plugin, which the browser cannot reach). v4 defaults the argument to
/// `false` ("omit it and only the provider rule applies"); Rust has no default
/// args, so every call site passes it explicitly.
///
/// v4's provider guard is `if (!provider) return true`, which is JS falsiness:
/// `null`, `undefined` **and the empty string** all take the true branch.
/// `None` and `Some("")` both model that here — though the empty string is
/// measurably UNOBSERVABLE (mutation-tested): `"".to_uppercase()` is not in
/// the hostile set either, so both routes answer `true`. The filter stays
/// because it is what v4 wrote, not because the corpus can tell.
pub fn default_multi_character_prefill(provider: Option<&str>, runs_thinking_turn: bool) -> bool {
    if runs_thinking_turn {
        return false;
    }
    let Some(p) = provider.filter(|s| !s.is_empty()) else {
        return true;
    };
    let upper = p.to_uppercase();
    !PREFILL_HOSTILE_PROVIDERS.contains(&upper.as_str())
}

/// v4 `profileUsesNamePrefill`: whether this profile anchors a multi-character
/// turn with the `[Name]` prefill. An explicit stored choice always wins —
/// including a stored `true` on a thinking model, because the tri-state exists
/// so the user may overrule us (the editor warns, never vetoes) — and a
/// null/absent one falls back to the default above.
///
/// `stored` is the tri-state column: `Some(b)` is v4's `typeof … === 'boolean'`
/// branch, `None` covers both SQL NULL and an absent key.
pub fn profile_uses_name_prefill(
    provider: Option<&str>,
    stored: Option<bool>,
    runs_thinking_turn: bool,
) -> bool {
    match stored {
        Some(b) => b,
        None => default_multi_character_prefill(provider, runs_thinking_turn),
    }
}

/// [`profile_uses_name_prefill`] over a net-read connection-profile object (the
/// shape the services carry). A non-boolean `multiCharacterPrefill` cell falls
/// through to the default exactly as v4's `typeof` guard does.
pub fn profile_uses_name_prefill_value(profile: &Value, runs_thinking_turn: bool) -> bool {
    profile_uses_name_prefill(
        profile.get("provider").and_then(Value::as_str),
        profile
            .get("multiCharacterPrefill")
            .and_then(Value::as_bool),
        runs_thinking_turn,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_value_helper_reads_both_fields() {
        assert!(!profile_uses_name_prefill_value(
            &json!({ "provider": "ANTHROPIC" }),
            false
        ));
        assert!(profile_uses_name_prefill_value(
            &json!({
                "provider": "ANTHROPIC",
                "multiCharacterPrefill": true
            }),
            false
        ));
        assert!(!profile_uses_name_prefill_value(
            &json!({
                "provider": "OPENAI",
                "multiCharacterPrefill": false
            }),
            false
        ));
        // An explicit JSON null is "never chosen" — the provider default.
        assert!(profile_uses_name_prefill_value(
            &json!({
                "provider": "OPENAI",
                "multiCharacterPrefill": Value::Null
            }),
            false
        ));
        // A non-boolean cell takes v4's `typeof` fall-through, not a coercion.
        assert!(!profile_uses_name_prefill_value(
            &json!({
                "provider": "ANTHROPIC",
                "multiCharacterPrefill": 1
            }),
            false
        ));
        // No provider key at all — v4's `!provider` branch.
        assert!(profile_uses_name_prefill_value(&json!({}), false));
    }

    /// v4 bug 85: the thinking answer is checked FIRST in the default — false
    /// for every provider, the falsy ones included — but a stored boolean
    /// still outranks it (the tri-state exists so the user may overrule us).
    #[test]
    fn a_thinking_profile_defaults_off_but_a_stored_true_wins() {
        for provider in [Some("DEEPSEEK"), Some("OLLAMA"), Some("OPENAI"), None] {
            assert!(!default_multi_character_prefill(provider, true));
        }
        // A thinking-capable provider that is NOT thinking keeps the prefill —
        // bug 68's objection preserved rather than re-incurred.
        assert!(default_multi_character_prefill(Some("DEEPSEEK"), false));
        assert!(default_multi_character_prefill(Some("OLLAMA"), false));
        // Anthropic stays off either way — the one genuine provider rule.
        assert!(!default_multi_character_prefill(Some("ANTHROPIC"), false));
        assert!(!default_multi_character_prefill(Some("ANTHROPIC"), true));
        assert!(profile_uses_name_prefill_value(
            &json!({ "provider": "DEEPSEEK", "multiCharacterPrefill": true }),
            true
        ));
        assert!(!profile_uses_name_prefill_value(
            &json!({ "provider": "DEEPSEEK" }),
            true
        ));
    }
}
