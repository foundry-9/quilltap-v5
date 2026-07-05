//! Dangerous-content settings resolver (v4
//! `lib/services/dangerous-content/resolver.service.ts`).
//!
//! Resolves the effective [`DangerousContentSettings`] from the global chat
//! settings plus an optional per-chat view. Two per-chat short-circuits collapse
//! the settings to the off-duty shape regardless of the global setting:
//! moderation-exempt chat types (Help / Brahma) and an operator off-duty flip.
//! Otherwise the global settings win, falling back to the default.
//!
//! The single decision point keeps the override cheap for callers: anything that
//! already gates on `settings.mode != "OFF"` picks up the override for free.

use serde_json::Value;

use crate::chat_predicates::is_moderation_exempt_chat_type;
use crate::db::chat_settings::DangerousContentSettings;

use super::chat_override::is_concierge_off_duty;

/// Where the resolved settings came from (v4
/// `ResolvedDangerousContentSettings.source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DangerSource {
    Global,
    Default,
    ChatOffDuty,
    ChatTypeExempt,
}

impl DangerSource {
    /// The v4 wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            DangerSource::Global => "global",
            DangerSource::Default => "default",
            DangerSource::ChatOffDuty => "chat-off-duty",
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

/// v4 `OFF_DUTY_DANGEROUS_CONTENT_SETTINGS` — the settings forced when the
/// operator flipped a chat off-duty (or the chat type is moderation-exempt).
/// Everything the Concierge would normally do is disabled, while still returning
/// a concrete shape so callers don't special-case it.
pub fn off_duty_dangerous_content_settings() -> DangerousContentSettings {
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
                settings: off_duty_dangerous_content_settings(),
                source: DangerSource::ChatTypeExempt,
            };
        }
    }

    if chat.is_some() && is_concierge_off_duty(chat) {
        return ResolvedDangerousContentSettings {
            settings: off_duty_dangerous_content_settings(),
            source: DangerSource::ChatOffDuty,
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
    fn help_chat_is_exempt_before_off_duty() {
        // A help chat resolves to chat-type-exempt even with a global mode.
        let global = default_dangerous_content_settings();
        let chat = json!({ "chatType": "help", "conciergeOverride": "OFF" });
        let r = resolve_dangerous_content_settings(Some(global), Some(&chat));
        assert_eq!(r.source, DangerSource::ChatTypeExempt);
        assert_eq!(r.settings.mode, "OFF");
        assert!(!r.settings.show_warning_badges);
    }

    #[test]
    fn off_duty_collapses() {
        let mut global = default_dangerous_content_settings();
        global.mode = "AUTO_ROUTE".to_string();
        let chat = json!({ "chatType": "salon", "conciergeOverride": "OFF" });
        let r = resolve_dangerous_content_settings(Some(global), Some(&chat));
        assert_eq!(r.source, DangerSource::ChatOffDuty);
        assert_eq!(r.settings.mode, "OFF");
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
