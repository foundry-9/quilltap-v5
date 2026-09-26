//! Dangerous-content settings resolver (v4
//! `lib/services/dangerous-content/resolver.service.ts`, at `4d370a90f` — #75).
//!
//! Resolves the effective [`DangerousContentSettings`] from the global chat
//! settings plus an optional chat whose Concierge state shapes the result, so
//! callers that gate behaviour on `settings.mode` pick the state up for free:
//!
//!   - exempt chat type (help, brahma) → [`locked_dangerous_content_settings`]
//!   - Locked      → [`locked_dangerous_content_settings`]
//!   - Unmoderated → the *global* settings (so the configured uncensored
//!     profile IDs ride through) with `mode: "AUTO_ROUTE"` forced and every
//!     scan off — the verdict is already in, so there is nothing to classify.
//!     Forcing AUTO_ROUTE even under a global `OFF` is deliberate: asking for
//!     the uncensored desk on one chat should not first require flipping a
//!     global switch.
//!   - Moderated   → the global settings (or the defaults).

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
    ChatLocked,
    ChatUnmoderated,
    ChatTypeExempt,
}

impl DangerSource {
    /// The v4 wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            DangerSource::Global => "global",
            DangerSource::Default => "default",
            DangerSource::ChatLocked => "chat-locked",
            DangerSource::ChatUnmoderated => "chat-unmoderated",
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
        auto_switch_after_refusals: crate::db::chat_settings::DEFAULT_AUTO_SWITCH_AFTER_REFUSALS,
    }
}

/// v4 `LOCKED_DANGEROUS_CONTENT_SETTINGS` (renamed from
/// `VOUCHED_SAFE_DANGEROUS_CONTENT_SETTINGS` at `4d370a90f`, same values) —
/// the settings forced on a Locked chat, and on the chat types the Concierge
/// has no standing on (Help Chat, Brahma Console). Everything the Concierge
/// would normally do is disabled — no scans, no reroute, no auto-switch — while
/// still returning a concrete shape so callers don't special-case it.
/// Deliberately carries no uncensored profile IDs: a Locked chat rides the
/// ordinary providers only.
pub fn locked_dangerous_content_settings() -> DangerousContentSettings {
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
        // v4 `49059fb14`: a Locked (then vouched-safe) chat never auto-switches.
        auto_switch_after_refusals: 0,
    }
}

/// v4 `resolveDangerousContentSettings(globalSettings, chat?)` at `4d370a90f`.
///
/// `global_settings` is the global chat settings' `dangerousContentSettings`
/// sub-object (v4 reads `globalSettings?.dangerousContentSettings`; the caller
/// extracts it, passing `None` when the chat settings row or the sub-object is
/// absent). `chat` is an optional chat carrying `conciergeMode` (or the
/// server-derived `conciergeState`) and `chatType`; the legacy pair is never
/// read. Order: exempt → Locked → Unmoderated → global / default (v4's own
/// test pins the first step: "moderation-exempt chat types win over the
/// Unmoderated state").
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
                settings: locked_dangerous_content_settings(),
                source: DangerSource::ChatTypeExempt,
            };
        }
    }

    // v4 `const state = chat ? getConciergeState(chat) : 'moderated'`.
    let state = if chat.is_some() {
        get_concierge_state(chat)
    } else {
        ConciergeState::Moderated
    };

    if state == ConciergeState::Locked {
        return ResolvedDangerousContentSettings {
            settings: locked_dangerous_content_settings(),
            source: DangerSource::ChatLocked,
        };
    }

    if state == ConciergeState::Unmoderated {
        // v4 spreads `globalSettings?.dangerousContentSettings ??
        // DEFAULT_DANGEROUS_CONTENT_SETTINGS`; v5's narrower signature already
        // carries that sub-object, so the fallback is the default struct.
        let global = global_settings.unwrap_or_else(default_dangerous_content_settings);
        return ResolvedDangerousContentSettings {
            settings: DangerousContentSettings {
                // ...global carries uncensoredImageProfileId / uncensoredTextProfileId
                mode: "AUTO_ROUTE".to_string(), // the verdict is already in
                threshold: 1.0,                 // nothing left to classify
                scan_text_chat: false,
                scan_image_prompts: false,
                scan_image_generation: false,
                show_warning_badges: false,
                ..global
            },
            source: DangerSource::ChatUnmoderated,
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
    //! v4 `resolver.test.ts` at `4d370a90f`, mirrored by name where the case
    //! is a pure resolver call (the `danger_resolver_equivalence` family runs
    //! the whole matrix against v4's real function).
    use super::*;
    use serde_json::json;

    fn global(mode: &str) -> DangerousContentSettings {
        DangerousContentSettings {
            mode: mode.to_string(),
            ..default_dangerous_content_settings()
        }
    }

    #[test]
    fn returns_locked_settings_and_source_chat_locked_for_a_locked_chat() {
        let chat = json!({ "chatType": "salon", "conciergeMode": "locked" });
        let r = resolve_dangerous_content_settings(Some(global("AUTO_ROUTE")), Some(&chat));
        assert_eq!(r.source, DangerSource::ChatLocked);
        assert_eq!(r.settings.mode, "OFF");
    }

    #[test]
    fn respects_global_settings_for_a_moderated_chat() {
        let chat = json!({ "chatType": "salon", "conciergeMode": "moderated" });
        let r = resolve_dangerous_content_settings(Some(global("DETECT_ONLY")), Some(&chat));
        assert_eq!(r.source, DangerSource::Global);
        assert_eq!(r.settings.mode, "DETECT_ONLY");
    }

    #[test]
    fn still_returns_locked_even_if_no_global_settings_were_configured() {
        let chat = json!({ "conciergeMode": "locked" });
        let r = resolve_dangerous_content_settings(None, Some(&chat));
        assert_eq!(r.source, DangerSource::ChatLocked);
    }

    #[test]
    fn locked_settings_have_mode_off_all_scans_disabled_and_the_auto_switch_off() {
        let s = locked_dangerous_content_settings();
        assert_eq!(s.mode, "OFF");
        assert_eq!(s.threshold, 1.0);
        assert!(!s.scan_text_chat && !s.scan_image_prompts && !s.scan_image_generation);
        assert!(!s.show_warning_badges);
        assert_eq!(s.auto_switch_after_refusals, 0);
        assert!(s.uncensored_text_profile_id.is_none() && s.uncensored_image_profile_id.is_none());
    }

    #[test]
    fn locked_wins_over_a_global_auto_route() {
        let mut g = global("AUTO_ROUTE");
        g.uncensored_text_profile_id = Some("prof-unc-1".into());
        let chat = json!({ "conciergeMode": "locked" });
        let r = resolve_dangerous_content_settings(Some(g), Some(&chat));
        assert_eq!(r.source, DangerSource::ChatLocked);
        assert!(r.settings.uncensored_text_profile_id.is_none());
    }

    #[test]
    fn ignores_the_legacy_concierge_override_column() {
        for over in ["OFF", "UNCENSORED"] {
            let chat =
                json!({ "chatType": "salon", "conciergeOverride": over, "isDangerousChat": true });
            let r = resolve_dangerous_content_settings(Some(global("DETECT_ONLY")), Some(&chat));
            assert_eq!(r.source, DangerSource::Global, "{over}");
        }
    }

    #[test]
    fn moderation_exempt_chat_types_win_over_the_unmoderated_state() {
        let chat = json!({ "chatType": "brahma", "conciergeMode": "unmoderated" });
        let r = resolve_dangerous_content_settings(Some(global("OFF")), Some(&chat));
        assert_eq!(r.source, DangerSource::ChatTypeExempt);
        assert_eq!(r.settings.mode, "OFF");
        assert_eq!(r.settings.threshold, 1.0);
    }

    #[test]
    fn unmoderated_forces_auto_route_under_a_global_off_and_carries_the_profile_ids() {
        let mut g = global("OFF");
        g.scan_image_generation = true;
        g.uncensored_text_profile_id = Some("11111111-1111-4111-8111-111111111111".into());
        g.uncensored_image_profile_id = Some("22222222-2222-4222-8222-222222222222".into());
        let chat = json!({ "chatType": "salon", "conciergeMode": "unmoderated" });
        let r = resolve_dangerous_content_settings(Some(g), Some(&chat));
        assert_eq!(r.source, DangerSource::ChatUnmoderated);
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
    fn default_when_no_settings() {
        let r = resolve_dangerous_content_settings(None, None);
        assert_eq!(r.source, DangerSource::Default);
        assert_eq!(r.settings.mode, "OFF");
        assert_eq!(r.settings.threshold, 0.7);
    }
}
