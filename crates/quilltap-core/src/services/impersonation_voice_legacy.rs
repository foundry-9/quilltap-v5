//! Translating the retired `impersonationVoiceRewrite` boolean into
//! `impersonationVoiceMode` (v4 `lib/chat/impersonation-voice-legacy.ts`, NEW
//! at `07b8f0209` — P4.D251).
//!
//! During 4.10 development the impersonated-line rehearsal was a single on/off
//! toggle, and "on" meant the model was called the moment the review dialog
//! opened. It became three states (`off` / `ask` / `always`), and an operator
//! who had switched it on is translated to `ask`: the dialog still opens, but
//! nothing is sent to a model until they ask for a restatement. That is the
//! deliberate behaviour change of record — the old eager call spent a request
//! on every impersonated line, including the ones never meant to be restated.
//!
//! One pure translation serves both v5 paths that still meet the old shape:
//! the boot ensure that re-homes v4's `impersonation-voice-mode-v1` migration
//! ([`crate::db::chat_settings_impersonation_voice_mode_repair`]) and a backup
//! restore taken during 4.10 development
//! ([`crate::services::backup::restore`]).
//!
//! ## The value twin
//!
//! Both functions take a [`serde_json::Value`]: the restore's bag IS a `Value`,
//! and the ensure reads an INTEGER-or-NULL cell it maps onto one. v4's scalar
//! rule is `value === true || value === 1 ? 'ask' : 'off'`, and JS has ONE
//! number type — so a JSON number translates to `'ask'` when its `f64` is
//! exactly `1.0`, whichever of serde's integer / float variants parsed it
//! (`1` and `1.0` are different `Number`s to serde and the same number to
//! JS; measured, the oracle's `one_point_zero` row). Strings (`'1'`, `'true'`)
//! are `'off'` on both sides — `===` never coerces.
//!
//! Pure — no I/O.

use std::borrow::Cow;

use serde_json::{Map, Value};

use crate::db::chat_settings::ImpersonationVoiceMode;

/// The retired column's key, as the restore's bag and the ensure's row name it.
pub const LEGACY_KEY: &str = "impersonationVoiceRewrite";
/// The column that replaced it.
pub const MODE_KEY: &str = "impersonationVoiceMode";

/// v4 `impersonationVoiceModeFromLegacy(value)`: `true` / `1` → `'ask'`;
/// anything else (`false`, `0`, `null`, `undefined`, a string, …) → `'off'`.
///
/// An absent value (`undefined`) is passed as [`Value::Null`] — both are
/// `'off'`, so the distinction never matters here.
pub fn impersonation_voice_mode_from_legacy(value: &Value) -> ImpersonationVoiceMode {
    let on = match value {
        Value::Bool(b) => *b,
        // JS `=== 1`: one number type, so `1` and `1.0` are the same value.
        Value::Number(n) => n.as_f64() == Some(1.0),
        _ => false,
    };
    if on {
        ImpersonationVoiceMode::Ask
    } else {
        ImpersonationVoiceMode::Off
    }
}

/// v4 `withImpersonationVoiceModeFromLegacy(settings)`, for a restore.
///
/// Returns [`Cow::Borrowed`] — v4's "same reference" — when the record has no
/// `impersonationVoiceRewrite` KEY (`'impersonationVoiceRewrite' in settings`:
/// key PRESENCE, so an explicit `undefined` under the key still translates;
/// JSON has no `undefined`, and a present `null` is the nearest a bag can
/// carry — it translates to `'off'` on both sides). Otherwise
/// [`Cow::Owned`]: the old key is dropped and `impersonationVoiceMode` set to
/// the record's own mode when it has one that is not `null` (JS `??` — a
/// `null` mode falls to the legacy value; an existing `'always'` wins), else
/// the legacy translation.
///
/// Key order follows v4's `{ ...rest, impersonationVoiceMode }`: a mode key
/// the record already carried KEEPS its slot (a JS object literal re-assigning
/// an existing property does not move it); a mode the record lacked is
/// APPENDED last — after `createdAt`, wherever the retired key sat. Measured
/// at the pin (the oracle's `key_true_mode_always` vs `key_true_no_mode`
/// rows); `preserve_order`'s `Map::insert` has exactly those two behaviours.
///
/// A non-object record is returned borrowed (v4 types the input as an object).
pub fn with_impersonation_voice_mode_from_legacy(settings: &Value) -> Cow<'_, Value> {
    let Some(obj) = settings.as_object() else {
        return Cow::Borrowed(settings);
    };
    if !obj.contains_key(LEGACY_KEY) {
        return Cow::Borrowed(settings);
    }
    let mut out: Map<String, Value> = obj.clone();
    // `swap_remove` would move the LAST key into the removed slot
    // (`serde-json-map-remove-is-swap-remove`); `shift_remove` keeps order.
    let legacy = out.shift_remove(LEGACY_KEY).unwrap_or(Value::Null);
    let mode = match out.get(MODE_KEY) {
        Some(existing) if !existing.is_null() => existing.clone(),
        _ => Value::String(
            impersonation_voice_mode_from_legacy(&legacy)
                .as_str()
                .to_string(),
        ),
    };
    out.insert(MODE_KEY.to_string(), mode);
    Cow::Owned(Value::Object(out))
}

#[cfg(test)]
mod tests {
    //! v4 `__tests__/unit/migrations/impersonation-voice-mode.test.ts`'s
    //! "impersonation-voice legacy translation" describe, by name (the
    //! `impersonation_voice_legacy_equivalence` family runs a wider corpus
    //! against v4's real module).
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_the_stored_boolean_or_integer() {
        for (v, want) in [
            (json!(true), ImpersonationVoiceMode::Ask),
            (json!(1), ImpersonationVoiceMode::Ask),
            (json!(1.0), ImpersonationVoiceMode::Ask),
            (json!(false), ImpersonationVoiceMode::Off),
            (json!(0), ImpersonationVoiceMode::Off),
            (json!(null), ImpersonationVoiceMode::Off),
            (json!(2), ImpersonationVoiceMode::Off),
            (json!("1"), ImpersonationVoiceMode::Off),
            (json!("true"), ImpersonationVoiceMode::Off),
        ] {
            assert_eq!(impersonation_voice_mode_from_legacy(&v), want, "{v}");
        }
    }

    #[test]
    fn translates_a_restored_record_and_drops_the_old_key_appending_the_mode() {
        let input = json!({ "id": "cs-1", "impersonationVoiceRewrite": true, "createdAt": "t" });
        let out = with_impersonation_voice_mode_from_legacy(&input);
        assert!(matches!(out, Cow::Owned(_)));
        assert_eq!(
            serde_json::to_string(&*out).unwrap(),
            r#"{"id":"cs-1","createdAt":"t","impersonationVoiceMode":"ask"}"#
        );
    }

    #[test]
    fn keeps_an_explicit_mode_over_the_old_key_in_its_own_slot() {
        let input = json!({
            "id": "cs-1", "impersonationVoiceRewrite": true,
            "impersonationVoiceMode": "always", "createdAt": "t"
        });
        let out = with_impersonation_voice_mode_from_legacy(&input);
        assert_eq!(
            serde_json::to_string(&*out).unwrap(),
            r#"{"id":"cs-1","impersonationVoiceMode":"always","createdAt":"t"}"#
        );
    }

    #[test]
    fn a_null_mode_falls_to_the_legacy_value() {
        let input = json!({
            "id": "cs-1", "impersonationVoiceRewrite": false,
            "impersonationVoiceMode": null, "createdAt": "t"
        });
        let out = with_impersonation_voice_mode_from_legacy(&input);
        assert_eq!(out["impersonationVoiceMode"], json!("off"));
    }

    #[test]
    fn returns_a_current_record_untouched() {
        let current = json!({ "id": "cs-1", "impersonationVoiceMode": "off" });
        let out = with_impersonation_voice_mode_from_legacy(&current);
        assert!(
            matches!(out, Cow::Borrowed(_)),
            "v4 returns the same reference"
        );
        assert_eq!(*out, current);
        assert!(matches!(
            with_impersonation_voice_mode_from_legacy(&json!("not an object")),
            Cow::Borrowed(_)
        ));
    }
}
