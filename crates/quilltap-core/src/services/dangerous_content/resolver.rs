//! Dangerous-content settings resolver (v4
//! `lib/services/dangerous-content/resolver.service.ts`).
//!
//! Resolves the effective [`DangerousContentSettings`] from the global chat
//! settings plus an optional per-chat view. Three per-chat short-circuits win
//! over the global setting: moderation-exempt chat types (Help / Brahma), the
//! operator's Uncensored assertion, and the operator's Vouched Safe. Otherwise
//! the global settings win, falling back to the default.
//!
//! The single decision point keeps the override cheap for callers: anything that
//! already gates on `settings.mode` picks up the override for free.

use serde_json::Value;

use crate::chat_predicates::is_moderation_exempt_chat_type;
use crate::db::chat_settings::DangerousContentSettings;

use super::chat_override::{get_concierge_state, ConciergeState};

/// Where the resolved settings came from (v4
/// `ResolvedDangerousContentSettings.source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DangerSource {
    Global,
    Default,
    ChatVouched,
    ChatUncensored,
    ChatTypeExempt,
}

impl DangerSource {
    /// The v4 wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            DangerSource::Global => "global",
            DangerSource::Default => "default",
            DangerSource::ChatVouched => "chat-vouched",
            DangerSource::ChatUncensored => "chat-uncensored",
            DangerSource::ChatTypeExempt => "chat-type-exempt",
        }
    }
}

/// The resolved settings + their provenance (v4
/// `ResolvedDangerousContentSettings`).
pub struct ResolvedDangerousContentSettings {
    pub settings: DangerousContentSettings,
    pub source: DangerSource,
}

/// v4 `DEFAULT_DANGEROUS_CONTENT_SETTINGS` — the settings when nothing is
/// configured.
pub fn default_dangerous_content_settings() -> DangerousContentSettings {
    DangerousContentSettings {
        mode: "OFF".to_string(),
        threshold: 0.7,
        scan_text_chat: true,
        scan_image_prompts: true,
        scan_image_generation: false,
        uncensored_text_profile_id: None,
        uncensored_image_profile_id: None,
        display_mode: "SHOW".to_string(),
        show_warning_badges: true,
        custom_classification_prompt: None,
    }
}

/// v4 `VOUCHED_SAFE_DANGEROUS_CONTENT_SETTINGS` — the settings forced when the
/// operator has vouched a chat safe (or the chat type is moderation-exempt).
/// Everything the Concierge would normally do is disabled, while still returning
/// a concrete shape so callers don't special-case it. Deliberately carries no
/// uncensored profile IDs — a vouched-safe chat rides the ordinary providers.
pub fn vouched_safe_dangerous_content_settings() -> DangerousContentSettings {
    DangerousContentSettings {
        mode: "OFF".to_string(),
        threshold: 1.0,
        scan_text_chat: false,
        scan_image_prompts: false,
        scan_image_generation: false,
        uncensored_text_profile_id: None,
        uncensored_image_profile_id: None,
        display_mode: "SHOW".to_string(),
        show_warning_badges: false,
        custom_classification_prompt: None,
    }
}

/// v4 `resolveDangerousContentSettings`.
///
/// When `chat` is supplied and carries an operator override, the returned
/// settings reflect it regardless of the global setting:
///
///   - Vouched Safe collapses to `mode: "OFF"` with every scan disabled.
///   - Uncensored spreads the *global* settings (so the configured uncensored
///     profile IDs ride through) and forces `mode: "AUTO_ROUTE"` with every scan
///     disabled — the operator has already returned the verdict, so there is
///     nothing left to classify. Forcing AUTO_ROUTE even under a global `OFF` is
///     deliberate: asking for uncensored routing on one chat should not first
///     require flipping a global switch. (Flagged, by contrast, continues to
///     obey the global mode.)
///
/// `global_settings` is the global chat settings' `dangerousContentSettings`
/// sub-object (v4 reads `globalSettings?.dangerousContentSettings`; the caller
/// extracts it, passing `None` when the chat settings row or the sub-object is
/// absent). `chat` is an optional per-chat view carrying `conciergeOverride` /
/// `chatType`.
pub fn resolve_dangerous_content_settings(
    global_settings: Option<DangerousContentSettings>,
    chat: Option<&Value>,
) -> ResolvedDangerousContentSettings {
    // Help Chats and the Brahma Console are never moderated — the Concierge has
    // no standing on those surfaces at all, regardless of the global setting.
    if let Some(chat) = chat {
        let chat_type = chat.get("chatType").and_then(Value::as_str);
        if is_moderation_exempt_chat_type(chat_type) {
            return ResolvedDangerousContentSettings {
                settings: vouched_safe_dangerous_content_settings(),
                source: DangerSource::ChatTypeExempt,
            };
        }
    }

    // The uncensored arm is checked BEFORE the vouched one and AFTER the
    // exempt one — v4's branch order, which its own test pins ("moderation-exempt
    // chat types win over the uncensored override").
    if chat.is_some() && get_concierge_state(chat) == ConciergeState::Uncensored {
        // v4 spreads `globalSettings?.dangerousContentSettings ??
        // DEFAULT_DANGEROUS_CONTENT_SETTINGS`; v5's narrower signature already
        // carries that sub-object, so the fallback is the default struct.
        let global = global_settings.unwrap_or_else(default_dangerous_content_settings);
        return ResolvedDangerousContentSettings {
            settings: DangerousContentSettings {
                // ...global carries uncensoredImageProfileId / uncensoredTextProfileId
                mode: "AUTO_ROUTE".to_string(), // the operator has already returned the verdict
                threshold: 1.0,                 // nothing left to classify
                scan_text_chat: false,
                scan_image_prompts: false,
                scan_image_generation: false,
                show_warning_badges: false,
                ..global
            },
            source: DangerSource::ChatUncensored,
        };
    }

    if chat.is_some() && get_concierge_state(chat) == ConciergeState::Vouched {
        return ResolvedDangerousContentSettings {
            settings: vouched_safe_dangerous_content_settings(),
            source: DangerSource::ChatVouched,
        };
    }

    if let Some(settings) = global_settings {
        return ResolvedDangerousContentSettings {
            settings,
            source: DangerSource::Global,
        };
    }

    ResolvedDangerousContentSettings {
        settings: default_dangerous_content_settings(),
        source: DangerSource::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn help_chat_is_exempt_before_vouched() {
        // A help chat resolves to chat-type-exempt even with a global mode.
        let global = default_dangerous_content_settings();
        let chat = json!({ "chatType": "help", "conciergeOverride": "OFF" });
        let r = resolve_dangerous_content_settings(Some(global), Some(&chat));
        assert_eq!(r.source, DangerSource::ChatTypeExempt);
        assert_eq!(r.settings.mode, "OFF");
        assert!(!r.settings.show_warning_badges);
    }

    #[test]
    fn vouched_collapses() {
        let mut global = default_dangerous_content_settings();
        global.mode = "AUTO_ROUTE".to_string();
        let chat = json!({ "chatType": "salon", "conciergeOverride": "OFF" });
        let r = resolve_dangerous_content_settings(Some(global), Some(&chat));
        assert_eq!(r.source, DangerSource::ChatVouched);
        assert_eq!(r.settings.mode, "OFF");
    }

    #[test]
    fn uncensored_forces_auto_route_under_a_global_off_and_carries_the_profile_ids() {
        let mut global = default_dangerous_content_settings();
        global.mode = "OFF".to_string();
        global.scan_image_generation = true;
        global.uncensored_text_profile_id = Some("11111111-1111-4111-8111-111111111111".into());
        global.uncensored_image_profile_id = Some("22222222-2222-4222-8222-222222222222".into());
        let chat = json!({ "chatType": "salon", "conciergeOverride": "UNCENSORED" });
        let r = resolve_dangerous_content_settings(Some(global), Some(&chat));
        assert_eq!(r.source, DangerSource::ChatUncensored);
        assert_eq!(r.settings.mode, "AUTO_ROUTE");
        assert_eq!(r.settings.threshold, 1.0);
        assert!(!r.settings.scan_text_chat);
        assert!(!r.settings.scan_image_prompts);
        assert!(!r.settings.scan_image_generation);
        assert!(!r.settings.show_warning_badges);
        assert_eq!(
            r.settings.uncensored_text_profile_id.as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
        assert_eq!(
            r.settings.uncensored_image_profile_id.as_deref(),
            Some("22222222-2222-4222-8222-222222222222")
        );
    }

    #[test]
    fn uncensored_spreads_the_defaults_when_no_global_is_configured() {
        let chat = json!({ "conciergeOverride": "UNCENSORED" });
        let r = resolve_dangerous_content_settings(None, Some(&chat));
        assert_eq!(r.source, DangerSource::ChatUncensored);
        assert_eq!(r.settings.mode, "AUTO_ROUTE");
        assert!(!r.settings.scan_text_chat);
    }

    #[test]
    fn exempt_chat_types_win_over_the_uncensored_override() {
        let mut global = default_dangerous_content_settings();
        global.mode = "OFF".to_string();
        let chat = json!({ "chatType": "brahma", "conciergeOverride": "UNCENSORED" });
        let r = resolve_dangerous_content_settings(Some(global), Some(&chat));
        assert_eq!(r.source, DangerSource::ChatTypeExempt);
        assert_eq!(r.settings.mode, "OFF");
        assert_eq!(r.settings.threshold, 1.0);
    }

    #[test]
    fn global_wins_when_present() {
        let mut global = default_dangerous_content_settings();
        global.mode = "AUTO_ROUTE".to_string();
        let chat = json!({ "chatType": "salon", "conciergeOverride": null });
        let r = resolve_dangerous_content_settings(Some(global), Some(&chat));
        assert_eq!(r.source, DangerSource::Global);
        assert_eq!(r.settings.mode, "AUTO_ROUTE");
    }

    #[test]
    fn default_when_no_settings() {
        let r = resolve_dangerous_content_settings(None, None);
        assert_eq!(r.source, DangerSource::Default);
        assert_eq!(r.settings.mode, "OFF");
        assert_eq!(r.settings.threshold, 0.7);
    }
}
