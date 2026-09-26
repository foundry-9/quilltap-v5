//! The retired Concierge settings, translated into `conciergeSettings` (v4
//! `lib/services/dangerous-content/legacy-concierge-settings.ts`, NEW at
//! `3b463d6b1`, #76).
//!
//! Before 4.10 the Concierge's settings lived in three places:
//! `chat_settings.dangerousContentSettings` (the OFF / DETECT_ONLY /
//! AUTO_ROUTE mode, the scans, the threshold, the two uncensored profiles,
//! display), `chat_settings.uncensoredImageDescriptionProfileId` (the vision
//! fallback) and `cheapLLMSettings.imagePromptProfileId` (the image-prompt
//! crafter). v4's migration (`add-concierge-settings-v1`) and restore of a
//! pre-4.10 backup both translate them with this ONE mapping; v5 runs no
//! migrations (E.2), so its two consumers are restore and the backup UUID
//! remap.
//!
//! The mode translation is the deliberate behaviour change of record:
//!
//!   - `OFF`          → off duty (`enabled: false`), pre-screen and summary
//!     classification off — unless the user has an Unmoderated chat, in
//!     which case the Concierge stays on duty (pre-screen still off) so that
//!     chat keeps its uncensored desk;
//!   - `DETECT_ONLY`  → on duty with the pre-screen and summary
//!     classification on. It gains failover, which it never had;
//!   - `AUTO_ROUTE`   → on duty with the pre-screen and summary
//!     classification on;
//!   - any other string (or none) reads as `OFF`.
//!
//! The legacy keys are left on the record for the repository's schema to
//! strip; this module never removes them.

use serde_json::{Map, Value};

use crate::db::chat_settings::{
    ConciergeDisplaySettings, ConciergePreScreenSettings, ConciergeSettings,
};

/// v4 `LegacyConciergeSources`. The three fields are read off a raw record
/// (a backup's settings row, or the parsed legacy columns); a non-object
/// `dangerousContentSettings` / `cheapLLMSettings` reads as absent, as v4's
/// property reads on a primitive do.
pub struct LegacyConciergeSources<'a> {
    pub dangerous_content_settings: Option<&'a Value>,
    pub uncensored_image_description_profile_id: Option<&'a Value>,
    pub cheap_llm_settings: Option<&'a Value>,
    /// Whether the user has any chat set Unmoderated.
    pub has_unmoderated_chats: bool,
}

const DISPLAY_MODES: [&str; 3] = ["SHOW", "BLUR", "COLLAPSE"];

/// A property of a legacy object, `undefined` when the carrier is not an
/// object (v4 reads `dc.mode` off whatever `?? {}` produced).
fn prop<'a>(carrier: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    carrier.and_then(Value::as_object).and_then(|o| o.get(key))
}

/// v4 `clampThreshold`: a number in `[0, 1]` survives, anything else is 0.7.
fn clamp_threshold(value: Option<&Value>) -> f64 {
    match value.and_then(Value::as_f64) {
        Some(n) if (0.0..=1.0).contains(&n) => n,
        _ => 0.7,
    }
}

/// v4 `clampAutoSwitch`: an integer in `[0, 10]` survives, anything else is 2.
fn clamp_auto_switch(value: Option<&Value>) -> i64 {
    match value.and_then(Value::as_f64) {
        Some(n) if n.fract() == 0.0 && (0.0..=10.0).contains(&n) => n as i64,
        _ => 2,
    }
}

/// v4 `bool(value, fallback)`: only a real boolean overrides the fallback.
fn bool_or(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}

/// v4 `idOrNull`: a non-empty string, else `null`.
fn id_or_null(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// v4 `mapLegacyConciergeSettings(sources)` — the translation, rule by rule.
/// The result serializes in v4's `MigratedConciergeSettings` key order (the
/// order the migration's `JSON.stringify` writes: `customClassificationPrompt`
/// BEFORE `summaryClassification`, every key present).
pub fn map_legacy_concierge_settings(sources: &LegacyConciergeSources<'_>) -> ConciergeSettings {
    // `sources.dangerousContentSettings ?? {}` — a `null` reads as the empty
    // object, so every field falls to its default.
    let dc = sources.dangerous_content_settings.filter(|v| !v.is_null());
    // Any other string — or none — reads as OFF.
    let mode = prop(dc, "mode")
        .and_then(Value::as_str)
        .filter(|m| matches!(*m, "DETECT_ONLY" | "AUTO_ROUTE"))
        .unwrap_or("OFF");
    let classifier_was_on = mode != "OFF";
    let enabled = classifier_was_on || sources.has_unmoderated_chats;

    let display_mode = match prop(dc, "displayMode").and_then(Value::as_str) {
        Some(m) if DISPLAY_MODES.contains(&m) => m,
        _ => "SHOW",
    };
    let custom_classification_prompt = match prop(dc, "customClassificationPrompt") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };

    ConciergeSettings {
        enabled,
        uncensored_text_profile_id: id_or_null(prop(dc, "uncensoredTextProfileId")),
        uncensored_image_profile_id: id_or_null(prop(dc, "uncensoredImageProfileId")),
        uncensored_vision_profile_id: id_or_null(sources.uncensored_image_description_profile_id),
        // `sources.cheapLLMSettings?.imagePromptProfileId`.
        image_prompt_profile_id: id_or_null(prop(
            sources.cheap_llm_settings.filter(|v| !v.is_null()),
            "imagePromptProfileId",
        )),
        auto_switch_after_refusals: clamp_auto_switch(prop(dc, "autoSwitchAfterRefusals")),
        new_chats_start_as: "moderated".to_string(),
        display: ConciergeDisplaySettings {
            mode: display_mode.to_string(),
            show_warning_badges: bool_or(prop(dc, "showWarningBadges"), true),
        },
        pre_screen: ConciergePreScreenSettings {
            enabled: classifier_was_on,
            threshold: clamp_threshold(prop(dc, "threshold")),
            scan_text_chat: bool_or(prop(dc, "scanTextChat"), true),
            scan_image_prompts: bool_or(prop(dc, "scanImagePrompts"), true),
            scan_image_generation: bool_or(prop(dc, "scanImageGeneration"), false),
            custom_classification_prompt,
            summary_classification: classifier_was_on,
        },
    }
}

/// v4 `withConciergeSettingsFromLegacy(settings, hasUnmoderatedChats)`: a
/// settings record that already carries a TRUTHY `conciergeSettings` is
/// returned as it is; otherwise the record gains `conciergeSettings`
/// translated from its own legacy keys. v4 spreads (`{ ...settings,
/// conciergeSettings }`), so a present-but-`null` key keeps its position and
/// an absent one is appended — `preserve_order`'s `insert` does the same.
/// A non-object record is returned unchanged (v4 types it as an object).
pub fn with_concierge_settings_from_legacy(settings: &Value, has_unmoderated_chats: bool) -> Value {
    let Some(obj) = settings.as_object() else {
        return settings.clone();
    };
    if crate::api::system_qtap::js_truthy(obj.get("conciergeSettings")) {
        return settings.clone();
    }
    let migrated = map_legacy_concierge_settings(&LegacyConciergeSources {
        dangerous_content_settings: obj.get("dangerousContentSettings"),
        uncensored_image_description_profile_id: obj.get("uncensoredImageDescriptionProfileId"),
        cheap_llm_settings: obj.get("cheapLLMSettings"),
        has_unmoderated_chats,
    });
    let mut out: Map<String, Value> = obj.clone();
    out.insert("conciergeSettings".into(), migrated.to_value());
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    //! v4 `migrations/__tests__/add-concierge-settings.test.ts`'s
    //! `mapLegacyConciergeSettings` describe, by name (the
    //! `danger_resolver_equivalence` family runs the same rows against v4's
    //! real module).
    use super::*;
    use serde_json::json;

    fn map(dc: Value, has_unmoderated: bool) -> ConciergeSettings {
        map_legacy_concierge_settings(&LegacyConciergeSources {
            dangerous_content_settings: Some(&dc),
            uncensored_image_description_profile_id: None,
            cheap_llm_settings: None,
            has_unmoderated_chats: has_unmoderated,
        })
    }

    #[test]
    fn off_goes_off_duty_with_pre_screen_and_summary_classification_off() {
        let m = map(json!({ "mode": "OFF" }), false);
        assert!(!m.enabled);
        assert!(!m.pre_screen.enabled);
        assert!(!m.pre_screen.summary_classification);
    }

    #[test]
    fn detect_only_and_auto_route_go_on_duty_with_pre_screen_and_summary_classification() {
        for mode in ["DETECT_ONLY", "AUTO_ROUTE"] {
            let m = map(json!({ "mode": mode }), false);
            assert!(m.enabled, "{mode}");
            assert!(m.pre_screen.enabled, "{mode}");
            assert!(m.pre_screen.summary_classification, "{mode}");
        }
    }

    #[test]
    fn off_with_an_unmoderated_chat_stays_on_duty_with_the_pre_screen_off() {
        let m = map(json!({ "mode": "OFF" }), true);
        assert!(m.enabled);
        assert!(!m.pre_screen.enabled);
        assert!(!m.pre_screen.summary_classification);
    }

    #[test]
    fn an_unknown_mode_reads_as_off() {
        assert!(!map(json!({ "mode": "SOMETIMES" }), false).enabled);
    }

    #[test]
    fn the_migrated_key_order_puts_the_prompt_before_summary_classification() {
        let text = serde_json::to_string(&map(json!({}), false).to_value()).unwrap();
        assert_eq!(
            text,
            "{\"enabled\":false,\"uncensoredTextProfileId\":null,\"uncensoredImageProfileId\":null,\
             \"uncensoredVisionProfileId\":null,\"imagePromptProfileId\":null,\
             \"autoSwitchAfterRefusals\":2,\"newChatsStartAs\":\"moderated\",\
             \"display\":{\"mode\":\"SHOW\",\"showWarningBadges\":true},\
             \"preScreen\":{\"enabled\":false,\"threshold\":0.7,\"scanTextChat\":true,\
             \"scanImagePrompts\":true,\"scanImageGeneration\":false,\
             \"customClassificationPrompt\":null,\"summaryClassification\":false}}"
        );
    }

    #[test]
    fn an_existing_truthy_concierge_settings_wins() {
        let s = json!({ "conciergeSettings": { "enabled": false }, "dangerousContentSettings": { "mode": "AUTO_ROUTE" } });
        assert_eq!(with_concierge_settings_from_legacy(&s, true), s);
    }

    #[test]
    fn a_null_concierge_settings_is_translated_in_place() {
        let s = json!({ "a": 1, "conciergeSettings": null, "z": 2, "dangerousContentSettings": { "mode": "AUTO_ROUTE" } });
        let out = with_concierge_settings_from_legacy(&s, false);
        let keys: Vec<&str> = out
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["a", "conciergeSettings", "z", "dangerousContentSettings"]
        );
        assert_eq!(out["conciergeSettings"]["enabled"], json!(true));
    }
}
