//! Per-chat Concierge override helpers (v4
//! `lib/services/dangerous-content/chat-override.ts`) — the single source of
//! truth for a chat's danger status.
//!
//! Danger lives in two stored fields, `isDangerousChat` (the classification
//! label) and `conciergeOverride` (`'OFF'` = the operator flipped the Concierge
//! off-duty). Neither field is meaningful on its own: off-duty *preserves* the
//! label (so the user can return to Safe or Flagged later) while suppressing
//! every Concierge effect. Because the two fields must always be read together,
//! derive everything from [`get_concierge_state`]:
//!
//!   - "Should the Concierge *act* right now?"  → [`is_chat_active_dangerous`]
//!   - "What state to *display* / manage?"       → [`get_concierge_state`]
//!
//! The port operates on a `serde_json::Value` chat row (the shape every ported
//! read yields), reading only `conciergeOverride` / `isDangerousChat`.

use serde_json::Value;

/// The canonical tri-state for a chat's Concierge status (v4 `ConciergeState`).
/// The string values are the wire contract for the manual-flip control, so they
/// must stay `'safe' | 'flagged' | 'off'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConciergeState {
    /// Not classified dangerous (and on-duty).
    Safe,
    /// Classified dangerous and on-duty: the Concierge acts.
    Flagged,
    /// Operator flipped the Concierge off-duty (`conciergeOverride === 'OFF'`).
    /// Wins over any classification; the label is preserved underneath.
    Off,
}

impl ConciergeState {
    /// The v4 wire string (`'safe' | 'flagged' | 'off'`).
    pub fn as_str(self) -> &'static str {
        match self {
            ConciergeState::Safe => "safe",
            ConciergeState::Flagged => "flagged",
            ConciergeState::Off => "off",
        }
    }
}

/// True iff the operator has flipped the Concierge off-duty for this chat
/// (v4 `isConciergeOffDuty`). When true, the Concierge takes no action
/// regardless of `isDangerousChat` or the global moderation mode.
///
/// `None` chat → `false` (v4's `if (!chat) return false`).
pub fn is_concierge_off_duty(chat: Option<&Value>) -> bool {
    match chat {
        None => false,
        Some(chat) => chat.get("conciergeOverride").and_then(Value::as_str) == Some("OFF"),
    }
}

/// THE canonical derivation of a chat's Concierge status from its two stored
/// fields (v4 `getConciergeState`). Off-duty wins over the classification label.
pub fn get_concierge_state(chat: Option<&Value>) -> ConciergeState {
    if is_concierge_off_duty(chat) {
        return ConciergeState::Off;
    }
    let flagged = chat
        .and_then(|c| c.get("isDangerousChat"))
        .and_then(Value::as_bool)
        == Some(true);
    if flagged {
        ConciergeState::Flagged
    } else {
        ConciergeState::Safe
    }
}

/// True iff this chat should be treated as dangerous *right now* (v4
/// `isChatActiveDangerous`) — both flagged by classification and not currently
/// Off-duty. Use in place of `chat.isDangerousChat === true` everywhere the
/// Concierge would otherwise reroute, sanitize, or pick an uncensored model.
pub fn is_chat_active_dangerous(chat: Option<&Value>) -> bool {
    get_concierge_state(chat) == ConciergeState::Flagged
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn off_duty_wins_over_flag() {
        let chat = json!({ "conciergeOverride": "OFF", "isDangerousChat": true });
        assert!(is_concierge_off_duty(Some(&chat)));
        assert_eq!(get_concierge_state(Some(&chat)), ConciergeState::Off);
        assert!(!is_chat_active_dangerous(Some(&chat)));
    }

    #[test]
    fn flagged_on_duty() {
        let chat = json!({ "conciergeOverride": null, "isDangerousChat": true });
        assert_eq!(get_concierge_state(Some(&chat)), ConciergeState::Flagged);
        assert!(is_chat_active_dangerous(Some(&chat)));
    }

    #[test]
    fn safe_default() {
        let chat = json!({ "isDangerousChat": false });
        assert_eq!(get_concierge_state(Some(&chat)), ConciergeState::Safe);
        assert!(!is_concierge_off_duty(Some(&chat)));
    }

    #[test]
    fn none_chat_is_safe_not_off_duty() {
        assert!(!is_concierge_off_duty(None));
        assert_eq!(get_concierge_state(None), ConciergeState::Safe);
        assert!(!is_chat_active_dangerous(None));
    }
}
