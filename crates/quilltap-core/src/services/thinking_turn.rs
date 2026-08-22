//! "Will this profile run a thinking turn?" — the one question, one answer
//! (v4 `lib/llm/thinking-turn.ts`, `97d2fcb5`).
//!
//! Thinking is not just a display feature. It changes what a request may look
//! like, and two providers are on record refusing an assistant `[Name]` prefill
//! *only* while thinking:
//!
//!   - **Ollama** opens a thinking model's reasoning block from the chat
//!     template at the start of the assistant turn, so a prefill means the
//!     block is never opened and `message.thinking` comes back empty (v4 bug
//!     68);
//!   - **DeepSeek** 400s with *"The `reasoning_content` in the thinking mode
//!     must be passed back to the API"* on a request that ends with an
//!     assistant turn it never produced the reasoning for (v4 bug 85).
//!
//! Both were discovered the hard way because the multi-character prefill
//! default was provider-shaped when the property it was trying to capture is
//! *model*-shaped. This module supplies the model-shaped answer that
//! [`crate::services::multi_character_prefill`] now asks for.
//!
//! Two facts make it up, because providers differ on where the truth lives:
//!
//!   1. **The profile's explicit choice** — a `thinkingTurnRule` declared by
//!      the provider plugin names the `parameters` key that switches thinking
//!      on or off, and which values mean which. Only the plugin knows its own
//!      option keys; the host reading them by hand is exactly what bug 68
//!      refused.
//!   2. **The model's own habit** — `thinksByDefault`, consulted when the
//!      profile says nothing. `deepseek-v4-flash` reasons with
//!      `parameters: '{}'`; Anthropic and Ollama thinking are opt-in.
//!
//! The rule is declarative rather than a predicate function on purpose: the
//! connection-profile editor needs the same answer in the browser, where a
//! server-side plugin closure cannot be called. A rule serialises out through
//! `/api/v1/providers`; a closure does not. [`evaluate_thinking_turn`] below is
//! pure and is the shared evaluator for both sides — never re-derive it.
//!
//! The rule and model-facts TYPES live in
//! [`crate::provider_manifest`] (v4 keeps them in `@quilltap/plugin-types`,
//! whose v5 analog is the manifest substrate); this module re-exports them so
//! call sites read as v4's do.

use serde_json::Value;

use crate::provider_manifest::Registry;
pub use crate::provider_manifest::{ThinkingModelFacts, ThinkingTurnRule};

/// Whether a connection profile on this provider will run a thinking turn
/// (v4 `providerRegistry.profileRunsThinkingTurn`, bug 85).
///
/// Joins the two halves the plugin owns — its declared `thinkingTurnRule`
/// (which `parameters` key switches reasoning on or off) and its model
/// catalogue's `thinksByDefault` flag — and hands both to the shared
/// [`evaluate_thinking_turn`], which is also what the profile editor runs in
/// the browser. Never re-derive the answer at a call site (bug 85).
///
/// v4's guards are JS falsiness: `!providerName` and a falsy `modelName` both
/// include the EMPTY STRING, so `Some("")` takes the same branch as
/// `None` here. The provider lookup is v4's exact-case `Map.get`.
pub fn profile_runs_thinking_turn(
    registry: &Registry,
    provider: Option<&str>,
    model_name: Option<&str>,
    parameters: Option<&Value>,
) -> bool {
    let Some(provider) = provider.filter(|s| !s.is_empty()) else {
        return false;
    };
    let Some(plugin) = registry.get_provider(provider) else {
        return false;
    };
    let model = model_name
        .filter(|s| !s.is_empty())
        .and_then(|m| registry.model_thinking_facts(provider, m));
    evaluate_thinking_turn(
        plugin.thinking_turn_rule.as_ref(),
        parameters,
        model.as_ref(),
    )
}

/// A profile option is "unset" when it is absent, null, or the empty string —
/// the last being how every enum field in the options schema spells
/// "(model default)". (v4 `isUnset`; "absent" is the `None` arm here since
/// JSON cannot carry `undefined`.)
fn is_unset(value: Option<&Value>) -> bool {
    match value {
        None => true,
        Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(_) => false,
    }
}

/// JS strict equality (`===`) over the scalar domain a rule value may hold
/// (`string | number | boolean`). Cross-type comparisons are always `false`
/// (no coercion); numbers compare as JS numbers do — `1 === 1.0` — where a
/// naive `serde_json::Value` equality would split the integer and float
/// representations JSON round-trips happen to produce.
fn js_strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(fx), Some(fy)) => fx == fy,
            _ => false,
        },
        // A rule value cannot be an object/array (the declared type is
        // scalar), and JS `===` on references would be identity anyway.
        _ => false,
    }
}

/// Whether a profile with these inputs will run a thinking turn (v4
/// `evaluateThinkingTurn`).
///
/// An explicit choice in the profile's parameters always wins — with
/// `disabledValues` checked BEFORE `enabledValues`, exactly as v4 orders the
/// two tests; absent one, the model's own habit decides; absent that, the
/// answer is no. Pure — safe on the client, in a heal, and in the request
/// path alike.
///
/// `parameters` is the profile's stored bag as JSON; a non-object value
/// contributes no option (the callers parse-or-`{}` exactly as v4's do).
pub fn evaluate_thinking_turn(
    rule: Option<&ThinkingTurnRule>,
    parameters: Option<&Value>,
    model: Option<&ThinkingModelFacts>,
) -> bool {
    if let Some(rule) = rule {
        let value = parameters
            .and_then(Value::as_object)
            .and_then(|o| o.get(&rule.option_key));
        if !is_unset(value) {
            let value = value.expect("checked set above");
            if rule
                .disabled_values
                .as_deref()
                .is_some_and(|vs| vs.iter().any(|v| js_strict_eq(v, value)))
            {
                return false;
            }
            if rule
                .enabled_values
                .as_deref()
                .is_some_and(|vs| vs.iter().any(|v| js_strict_eq(v, value)))
            {
                return true;
            }
        }
    }

    model.is_some_and(|m| m.thinks_by_default == Some(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deepseek_rule() -> ThinkingTurnRule {
        ThinkingTurnRule {
            option_key: "thinking".into(),
            enabled_values: Some(vec![json!("enabled")]),
            disabled_values: Some(vec![json!("disabled")]),
        }
    }

    #[test]
    fn explicit_choice_outranks_the_model_habit() {
        let rule = deepseek_rule();
        let thinks = ThinkingModelFacts {
            supports_thinking: Some(true),
            thinks_by_default: Some(true),
        };
        assert!(!evaluate_thinking_turn(
            Some(&rule),
            Some(&json!({ "thinking": "disabled" })),
            Some(&thinks),
        ));
        assert!(evaluate_thinking_turn(
            Some(&rule),
            Some(&json!({ "thinking": "enabled" })),
            None,
        ));
    }

    /// v4 checks `disabledValues` BEFORE `enabledValues` — a value both lists
    /// claim answers `false`.
    #[test]
    fn disabled_is_checked_before_enabled() {
        let rule = ThinkingTurnRule {
            option_key: "t".into(),
            enabled_values: Some(vec![json!("both")]),
            disabled_values: Some(vec![json!("both")]),
        };
        assert!(!evaluate_thinking_turn(
            Some(&rule),
            Some(&json!({ "t": "both" })),
            Some(&ThinkingModelFacts {
                supports_thinking: None,
                thinks_by_default: Some(true),
            }),
        ));
    }

    /// The empty string is "(model default)" — unset, so the model habit
    /// decides.
    #[test]
    fn empty_string_is_unset() {
        let rule = deepseek_rule();
        let thinks = ThinkingModelFacts {
            supports_thinking: None,
            thinks_by_default: Some(true),
        };
        assert!(evaluate_thinking_turn(
            Some(&rule),
            Some(&json!({ "thinking": "" })),
            Some(&thinks),
        ));
        assert!(!evaluate_thinking_turn(
            Some(&rule),
            Some(&json!({ "thinking": "" })),
            None,
        ));
    }

    /// `supportsThinking` alone is capability, not habit — it never turns the
    /// answer on.
    #[test]
    fn supports_thinking_alone_is_not_a_habit() {
        assert!(!evaluate_thinking_turn(
            None,
            None,
            Some(&ThinkingModelFacts {
                supports_thinking: Some(true),
                thinks_by_default: None,
            }),
        ));
    }

    /// The join over the built-in registry: the two declaring plugins at
    /// `12fe3e6f`, exactly as v4's `profileRunsThinkingTurn` resolves them.
    #[test]
    fn the_registry_join_answers_per_profile() {
        let reg = Registry::built_in();
        // A default-state DeepSeek profile on a V4 model thinks (bug 85's
        // repro shape: `parameters: {}` still returns reasoning_content).
        assert!(profile_runs_thinking_turn(
            reg,
            Some("DEEPSEEK"),
            Some("deepseek-v4-flash"),
            Some(&json!({})),
        ));
        // An explicit disabled outranks the model habit.
        assert!(!profile_runs_thinking_turn(
            reg,
            Some("DEEPSEEK"),
            Some("deepseek-v4-flash"),
            Some(&json!({ "thinking": "disabled" })),
        ));
        // An uncatalogued id contributes no habit.
        assert!(!profile_runs_thinking_turn(
            reg,
            Some("DEEPSEEK"),
            Some("deepseek-experimental"),
            Some(&json!({})),
        ));
        // Ollama: opt-in only — no thinksByDefault models.
        assert!(profile_runs_thinking_turn(
            reg,
            Some("OLLAMA"),
            Some("qwen3:8b"),
            Some(&json!({ "enable_thinking": true })),
        ));
        assert!(!profile_runs_thinking_turn(
            reg,
            Some("OLLAMA"),
            Some("qwen3:8b"),
            Some(&json!({})),
        ));
        // Unknown / falsy providers answer false (v4's `!providerName` is JS
        // falsiness, so the empty string rides the same branch).
        assert!(!profile_runs_thinking_turn(
            reg,
            Some("NOT_A_PROVIDER"),
            None,
            None
        ));
        assert!(!profile_runs_thinking_turn(reg, Some(""), None, None));
        assert!(!profile_runs_thinking_turn(reg, None, None, None));
    }

    /// JS `===` on numbers: `1 === 1.0`, but never across types.
    #[test]
    fn rule_values_compare_as_js_strict_equality() {
        let rule = ThinkingTurnRule {
            option_key: "level".into(),
            enabled_values: Some(vec![json!(1)]),
            disabled_values: None,
        };
        assert!(evaluate_thinking_turn(
            Some(&rule),
            Some(&json!({ "level": 1.0 })),
            None,
        ));
        assert!(!evaluate_thinking_turn(
            Some(&rule),
            Some(&json!({ "level": "1" })),
            None,
        ));
        assert!(!evaluate_thinking_turn(
            Some(&rule),
            Some(&json!({ "level": true })),
            None,
        ));
    }
}
