//! The Concierge policy resolver (v4
//! `lib/services/dangerous-content/resolver.service.ts`, rewritten at
//! `3b463d6b1` — #76).
//!
//! Turns the global `conciergeSettings` and one chat's Concierge state into the
//! effective policy for that chat. Callers ask the policy the question they
//! mean — "may a refusal be rerouted?" ([`ResolvedConciergePolicy::failover_allowed`]),
//! "does this chat go straight to the uncensored desk?"
//! ([`ResolvedConciergePolicy::route_direct`]), "may the classifier pre-screen
//! this?" (`pre_screen.*`) — rather than reading a mode.
//!
//! Replaces `resolveDangerousContentSettings` and the retired
//! OFF / DETECT_ONLY / AUTO_ROUTE mode. Pure — no I/O.

use serde::Serialize;
use serde_json::Value;

use crate::chat_predicates::is_moderation_exempt_chat_type;
use crate::db::chat_settings::{
    ConciergeDisplaySettings, ConciergePreScreenSettings, ConciergeSettings,
};

use super::chat_override::{get_concierge_state, ConciergeState};

/// v4 `DEFAULT_AUTO_SWITCH_AFTER_REFUSALS` — stated moderation refusals on a
/// Moderated chat before the Concierge switches it to Unmoderated, when the
/// setting is absent. Mirrors the schema default.
pub const DEFAULT_AUTO_SWITCH_AFTER_REFUSALS: i64 = 2;

/// v4 `DEFAULT_CONCIERGE_SETTINGS` — the global Concierge settings when none
/// are stored. Mirrors the schema defaults, with every optional key present
/// (`null`), which is ALSO the byte shape the repository's default row writes.
pub fn default_concierge_settings() -> ConciergeSettings {
    ConciergeSettings {
        enabled: true,
        uncensored_text_profile_id: None,
        uncensored_image_profile_id: None,
        uncensored_vision_profile_id: None,
        image_prompt_profile_id: None,
        auto_switch_after_refusals: DEFAULT_AUTO_SWITCH_AFTER_REFUSALS,
        new_chats_start_as: "moderated".to_string(),
        display: ConciergeDisplaySettings {
            mode: "SHOW".to_string(),
            show_warning_badges: true,
        },
        pre_screen: ConciergePreScreenSettings {
            enabled: false,
            threshold: 0.7,
            scan_text_chat: true,
            scan_image_prompts: true,
            scan_image_generation: false,
            custom_classification_prompt: None,
            summary_classification: false,
        },
    }
}

/// v4 `ResolvedPreScreen` — the effective pre-screen for one chat. Every flag
/// is off unless the classifier may run.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPreScreen {
    pub enabled: bool,
    #[serde(serialize_with = "serialize_js_number")]
    pub threshold: f64,
    pub scan_text_chat: bool,
    pub scan_image_prompts: bool,
    pub scan_image_generation: bool,
    pub custom_classification_prompt: Option<String>,
}

/// v4 `ResolvedConciergeDesk` — the uncensored desk as this chat may use it.
/// All `None` when the chat may not reach it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConciergeDesk {
    pub text_profile_id: Option<String>,
    pub image_profile_id: Option<String>,
    pub vision_profile_id: Option<String>,
    pub image_prompt_profile_id: Option<String>,
}

/// v4 `ConciergePolicySource` — where the policy came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConciergePolicySource {
    Global,
    Default,
    ChatLocked,
    ChatUnmoderated,
    ChatTypeExempt,
    OffDuty,
}

impl ConciergePolicySource {
    /// The v4 wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            ConciergePolicySource::Global => "global",
            ConciergePolicySource::Default => "default",
            ConciergePolicySource::ChatLocked => "chat-locked",
            ConciergePolicySource::ChatUnmoderated => "chat-unmoderated",
            ConciergePolicySource::ChatTypeExempt => "chat-type-exempt",
            ConciergePolicySource::OffDuty => "off-duty",
        }
    }
}

impl Serialize for ConciergePolicySource {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// v4 `ResolvedConciergePolicy` — the Concierge's effective policy for one
/// chat (or, with no chat, for the global Moderated default). Serializes in
/// v4's interface order (the resolver family diffs it whole).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConciergePolicy {
    /// The Concierge is on duty (global switch on) and the chat type is not
    /// exempt.
    pub on_duty: bool,
    /// The chat's Concierge state; Moderated when no chat was supplied.
    #[serde(serialize_with = "serialize_state")]
    pub state: ConciergeState,
    /// A refusal on content grounds may be retried on the uncensored desk. On
    /// duty and not Locked. (An Unmoderated chat already routes direct; the
    /// failover stays open as its safety net for any call that still reached
    /// an ordinary provider — a continue turn, a cheap-LLM task on a profile
    /// that was not swapped.)
    pub failover_allowed: bool,
    /// The chat goes straight to the uncensored desk, with candid prompts. On
    /// duty and Unmoderated.
    pub route_direct: bool,
    /// The classifier pre-screen. On duty, Moderated, and pre-screen enabled.
    pub pre_screen: ResolvedPreScreen,
    /// The background summary classifier and its sweep. On duty, Moderated,
    /// and opted in.
    pub summary_classification: bool,
    /// Stated refusals before the Concierge switches the chat. 0 unless on
    /// duty and Moderated.
    pub auto_switch_after_refusals: i64,
    /// Who stands at the uncensored desk for this chat.
    pub desk: ResolvedConciergeDesk,
    /// How flagged content looks.
    pub display: ConciergeDisplaySettings,
    /// The state new chats start in (from the global settings).
    pub new_chats_start_as: String,
    /// Where the policy came from.
    pub source: ConciergePolicySource,
}

fn serialize_state<S: serde::Serializer>(state: &ConciergeState, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(state.as_str())
}

fn serialize_js_number<S: serde::Serializer>(n: &f64, s: S) -> Result<S::Ok, S::Error> {
    crate::db::js_number_to_json(*n).serialize(s)
}

/// v4 `NO_PRE_SCREEN`.
fn no_pre_screen() -> ResolvedPreScreen {
    ResolvedPreScreen {
        enabled: false,
        threshold: 1.0,
        scan_text_chat: false,
        scan_image_prompts: false,
        scan_image_generation: false,
        custom_classification_prompt: None,
    }
}

/// A key of the stored object, typed, falling back when absent or `null`
/// (v4's spread overlays a stored `null` too; the typed consumers read it as
/// the default — the only reachable `null`s are the optional ids and prompt,
/// whose default IS `null`).
fn bool_at(o: &serde_json::Map<String, Value>, key: &str, d: bool) -> bool {
    o.get(key).and_then(Value::as_bool).unwrap_or(d)
}
fn opt_string_at(
    o: &serde_json::Map<String, Value>,
    key: &str,
    d: Option<String>,
) -> Option<String> {
    match o.get(key) {
        None => d,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => None,
    }
}
fn string_at(o: &serde_json::Map<String, Value>, key: &str, d: &str) -> String {
    o.get(key).and_then(Value::as_str).unwrap_or(d).to_string()
}

/// v4 `readConciergeSettings(globalSettings)` — the stored global Concierge
/// settings with any gap (a row written before a field existed) filled from
/// [`default_concierge_settings`]: `{ ...DEFAULT, ...stored }` with `display`
/// and `preScreen` merged one level (`...(stored.display ?? {})`). A falsy or
/// absent `conciergeSettings` answers the defaults.
///
/// `global_settings` is the chat-settings row (v4's `ConciergeSettingsCarrier`
/// — anything with a `conciergeSettings` key). v4's spread keeps unknown extra
/// keys, but the only stored objects it ever sees have been through the
/// repository's Zod parse, which strips them — so the typed result loses
/// nothing a v4 reader could observe.
pub fn read_concierge_settings(global_settings: Option<&Value>) -> ConciergeSettings {
    read_stored_concierge_settings(global_settings.and_then(|g| g.get("conciergeSettings")))
}

/// [`read_concierge_settings`] for a caller holding only the stored
/// `conciergeSettings` value (v4 `readConciergeSettings({ conciergeSettings:
/// stored })`).
pub fn read_stored_concierge_settings(stored: Option<&Value>) -> ConciergeSettings {
    let defaults = default_concierge_settings();
    if !crate::api::system_qtap::js_truthy(stored) {
        return defaults;
    }
    let Some(o) = stored.and_then(Value::as_object) else {
        return defaults;
    };
    let empty = serde_json::Map::new();
    let display = o
        .get("display")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let pre = o
        .get("preScreen")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let d = &defaults;
    ConciergeSettings {
        enabled: bool_at(o, "enabled", d.enabled),
        uncensored_text_profile_id: opt_string_at(o, "uncensoredTextProfileId", None),
        uncensored_image_profile_id: opt_string_at(o, "uncensoredImageProfileId", None),
        uncensored_vision_profile_id: opt_string_at(o, "uncensoredVisionProfileId", None),
        image_prompt_profile_id: opt_string_at(o, "imagePromptProfileId", None),
        auto_switch_after_refusals: o
            .get("autoSwitchAfterRefusals")
            .and_then(Value::as_f64)
            .map(|n| n as i64)
            .unwrap_or(d.auto_switch_after_refusals),
        new_chats_start_as: string_at(o, "newChatsStartAs", &d.new_chats_start_as),
        display: ConciergeDisplaySettings {
            mode: string_at(display, "mode", &d.display.mode),
            show_warning_badges: bool_at(
                display,
                "showWarningBadges",
                d.display.show_warning_badges,
            ),
        },
        pre_screen: ConciergePreScreenSettings {
            enabled: bool_at(pre, "enabled", d.pre_screen.enabled),
            threshold: pre
                .get("threshold")
                .and_then(Value::as_f64)
                .unwrap_or(d.pre_screen.threshold),
            scan_text_chat: bool_at(pre, "scanTextChat", d.pre_screen.scan_text_chat),
            scan_image_prompts: bool_at(pre, "scanImagePrompts", d.pre_screen.scan_image_prompts),
            scan_image_generation: bool_at(
                pre,
                "scanImageGeneration",
                d.pre_screen.scan_image_generation,
            ),
            custom_classification_prompt: opt_string_at(pre, "customClassificationPrompt", None),
            summary_classification: bool_at(
                pre,
                "summaryClassification",
                d.pre_screen.summary_classification,
            ),
        },
    }
}

/// v4 `deskFrom`.
fn desk_from(settings: &ConciergeSettings) -> ResolvedConciergeDesk {
    ResolvedConciergeDesk {
        text_profile_id: settings.uncensored_text_profile_id.clone(),
        image_profile_id: settings.uncensored_image_profile_id.clone(),
        vision_profile_id: settings.uncensored_vision_profile_id.clone(),
        image_prompt_profile_id: settings.image_prompt_profile_id.clone(),
    }
}

/// v4 `resolveConfiguredConciergeDesk(globalSettings)` (NEW at `ce2f1dabf`,
/// #77) — the uncensored desk AS CONFIGURED, whatever the Concierge's duty or
/// the chat's state. Only for the operator's own explicit "Try uncensored"
/// ([`super::retry_uncensored`]), which is gated by the chat's state alone —
/// every automatic path reads [`resolve_concierge_settings`]`(…).desk`, which
/// is empty off duty and on Locked or exempt chats.
pub fn resolve_configured_concierge_desk(global_settings: Option<&Value>) -> ResolvedConciergeDesk {
    desk_from(&read_concierge_settings(global_settings))
}

/// v4 `preScreenFrom`. A disabled pre-screen still carries the threshold and
/// the prompt: the summary classifier reads them.
fn pre_screen_from(pre: &ConciergePreScreenSettings) -> ResolvedPreScreen {
    if !pre.enabled {
        return ResolvedPreScreen {
            threshold: pre.threshold,
            custom_classification_prompt: pre.custom_classification_prompt.clone(),
            ..no_pre_screen()
        };
    }
    ResolvedPreScreen {
        enabled: true,
        threshold: pre.threshold,
        scan_text_chat: pre.scan_text_chat,
        scan_image_prompts: pre.scan_image_prompts,
        scan_image_generation: pre.scan_image_generation,
        custom_classification_prompt: pre.custom_classification_prompt.clone(),
    }
}

/// v4 `resolveConciergeSettings(globalSettings, chat?)` — the effective policy,
/// in v4's branch order:
///
///   - exempt chat type (help, brahma) → nothing: the Concierge has no standing
///   - global `enabled: false`         → nothing: he is off duty
///   - Locked      → no failover, no pre-screen, no auto-switch, no desk (still
///     on duty, with the global display)
///   - Unmoderated → `route_direct`, failover as a safety net, no pre-screen,
///     no auto-switch, no warning badges
///   - Moderated   → failover, auto-switch, and the pre-screen if opted in
///
/// `global_settings` is the chat-settings row; `chat` an optional chat carrying
/// `conciergeMode` (or the derived `conciergeState`) and `chatType`.
pub fn resolve_concierge_settings(
    global_settings: Option<&Value>,
    chat: Option<&Value>,
) -> ResolvedConciergePolicy {
    resolve_stored_concierge_settings(
        global_settings.and_then(|g| g.get("conciergeSettings")),
        chat,
    )
}

/// [`resolve_concierge_settings`] for a caller holding only the stored
/// `conciergeSettings` value (v4 `resolveConciergeSettings({ conciergeSettings:
/// stored }, chat)` — `source` is `'global'` exactly when `stored` is truthy).
pub fn resolve_stored_concierge_settings(
    stored: Option<&Value>,
    chat: Option<&Value>,
) -> ResolvedConciergePolicy {
    let settings = read_stored_concierge_settings(stored);
    let state = if chat.is_some() {
        get_concierge_state(chat)
    } else {
        ConciergeState::Moderated
    };

    let inert = |source: ConciergePolicySource| ResolvedConciergePolicy {
        on_duty: false,
        state,
        failover_allowed: false,
        route_direct: false,
        pre_screen: no_pre_screen(),
        summary_classification: false,
        auto_switch_after_refusals: 0,
        desk: ResolvedConciergeDesk::default(),
        display: ConciergeDisplaySettings {
            mode: "SHOW".to_string(),
            show_warning_badges: false,
        },
        new_chats_start_as: settings.new_chats_start_as.clone(),
        source,
    };

    // Help Chats and the Brahma Console are never moderated — the Concierge has
    // no standing on those surfaces at all, regardless of the global setting.
    if let Some(chat) = chat {
        if is_moderation_exempt_chat_type(chat.get("chatType").and_then(Value::as_str)) {
            return inert(ConciergePolicySource::ChatTypeExempt);
        }
    }

    if !settings.enabled {
        return inert(ConciergePolicySource::OffDuty);
    }

    if state == ConciergeState::Locked {
        return ResolvedConciergePolicy {
            on_duty: true,
            display: settings.display.clone(),
            ..inert(ConciergePolicySource::ChatLocked)
        };
    }

    if state == ConciergeState::Unmoderated {
        return ResolvedConciergePolicy {
            on_duty: true,
            state,
            failover_allowed: true,
            route_direct: true,
            pre_screen: no_pre_screen(), // the verdict is already in
            summary_classification: false,
            auto_switch_after_refusals: 0,
            desk: desk_from(&settings),
            display: ConciergeDisplaySettings {
                show_warning_badges: false,
                ..settings.display.clone()
            },
            new_chats_start_as: settings.new_chats_start_as.clone(),
            source: ConciergePolicySource::ChatUnmoderated,
        };
    }

    let has_stored = crate::api::system_qtap::js_truthy(stored);
    ResolvedConciergePolicy {
        on_duty: true,
        state,
        failover_allowed: true,
        route_direct: false,
        pre_screen: pre_screen_from(&settings.pre_screen),
        summary_classification: settings.pre_screen.summary_classification,
        auto_switch_after_refusals: settings.auto_switch_after_refusals,
        desk: desk_from(&settings),
        display: settings.display.clone(),
        new_chats_start_as: settings.new_chats_start_as.clone(),
        source: if has_stored {
            ConciergePolicySource::Global
        } else {
            ConciergePolicySource::Default
        },
    }
}

/// A test-only policy in the retired mode's terms (P4.D227): `"AUTO_ROUTE"`
/// is a Moderated chat with the Concierge on duty (failover allowed) and the
/// given desk text profile; every other retired mode is a test that meant
/// "the Concierge will not reroute", which since `3b463d6b1` is OFF DUTY
/// (`DETECT_ONLY` now translates to on duty WITH failover — the behaviour
/// change of record — so a test that wanted it inert says so this way).
#[cfg(test)]
pub(crate) fn test_policy(mode: &str, text_profile_id: Option<&str>) -> ResolvedConciergePolicy {
    let mut stored = serde_json::json!({ "enabled": mode == "AUTO_ROUTE" });
    if let Some(id) = text_profile_id {
        stored["uncensoredTextProfileId"] = serde_json::json!(id);
    }
    resolve_stored_concierge_settings(Some(&stored), None)
}
