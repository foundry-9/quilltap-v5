//! Per-chat Concierge override helpers (v4
//! `lib/services/dangerous-content/chat-override.ts`) — the single source of
//! truth for a chat's danger status.
//!
//! Danger lives in two stored fields, `isDangerousChat` (the classification
//! label) and `conciergeOverride` (`'OFF'` = the operator vouched the chat
//! safe; `'UNCENSORED'` = the operator asserted it spicy and opened the
//! uncensored door themselves). Neither field is meaningful on its own: both
//! operator states *preserve* the label (so the user can return to Monitored
//! or Flagged later) while taking the classifier off the case.
//!
//! The four states are a 2×2 — rows are the route, columns are the provenance:
//!
//! ```text
//!   |                    | Concierge decides | operator decides |
//!   | ordinary route     | 'monitored'       | 'vouched'        |
//!   | uncensored route   | 'flagged'         | 'uncensored'     |
//! ```
//!
//! Because the two fields must always be read together, NOTHING outside this
//! module (and the handful of sanctioned writers/serializers) should read the
//! raw fields. Derive everything from [`get_concierge_state`], or ask one of
//! the purpose-named questions:
//!
//!   - "Take the uncensored routes right now?" → [`should_use_uncensored_route`]
//!     (or [`concierge_state_uses_uncensored_route`], given a derived state)
//!   - "Paint danger styling in the UI?"        → [`should_show_danger_styling`]
//!   - "May the classifier run at all?"          → [`is_classifier_on_duty`]
//!
//! Reading a raw field on its own — or answering one question with another
//! question's predicate — is how an override gets silently dropped. v4
//! `60e3c4a0a` DELETED the two overloaded predicates (`isConciergeOffDuty`,
//! `isChatActiveDangerous`) rather than re-pointing them, so every call site is
//! forced to state which question it is asking; this port does the same.
//!
//! The port operates on a `serde_json::Value` chat row (the shape every ported
//! read yields), reading only `conciergeOverride` / `isDangerousChat`.

use serde_json::Value;

/// The stored `chats.conciergeOverride` domain (NULL = the classifier decides)
/// — v4 `ConciergeOverrideValue`.
pub const CONCIERGE_OVERRIDE_OFF: &str = "OFF";
/// See [`CONCIERGE_OVERRIDE_OFF`].
pub const CONCIERGE_OVERRIDE_UNCENSORED: &str = "UNCENSORED";

/// The canonical four-state for a chat's Concierge status (v4
/// `ConciergeState`). The string values are also the wire contract for the
/// manual-flip control (`PUT /api/v1/chats/[id]` `conciergeState`), so they must
/// stay `'monitored' | 'flagged' | 'vouched' | 'uncensored'`.
///
/// - `Monitored` — not classified dangerous; the classifier keeps watch and may
///   auto-flip to `Flagged`.
/// - `Flagged` — classified dangerous (auto or manual): uncensored routes,
///   danger styling, the works.
/// - `Vouched` — operator vouched the chat safe (`conciergeOverride === 'OFF'`).
///   No classification, no uncensored routes; the label is preserved underneath.
/// - `Uncensored` — operator asserted the chat spicy (`conciergeOverride ===
///   'UNCENSORED'`). Every uncensored route, zero classification, zero danger
///   styling; the label is preserved underneath.
///
/// Only the classifier moves a chat between `Monitored` and `Flagged`; only the
/// operator can enter or leave `Vouched` / `Uncensored`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConciergeState {
    Monitored,
    Flagged,
    Vouched,
    Uncensored,
}

impl ConciergeState {
    /// The v4 wire string (`'monitored' | 'flagged' | 'vouched' | 'uncensored'`).
    pub fn as_str(self) -> &'static str {
        match self {
            ConciergeState::Monitored => "monitored",
            ConciergeState::Flagged => "flagged",
            ConciergeState::Vouched => "vouched",
            ConciergeState::Uncensored => "uncensored",
        }
    }

    /// The inverse of [`ConciergeState::as_str`] — the four wire strings the
    /// `conciergeState` PUT arm accepts. Anything else is `None` (v4's
    /// `z.enum([...])` refuses it with a 400).
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "monitored" => Some(ConciergeState::Monitored),
            "flagged" => Some(ConciergeState::Flagged),
            "vouched" => Some(ConciergeState::Vouched),
            "uncensored" => Some(ConciergeState::Uncensored),
            _ => None,
        }
    }
}

/// THE canonical derivation of a chat's Concierge status from its two stored
/// fields (v4 `getConciergeState`). Every other helper — and every
/// display/management read — should go through this so an operator override can
/// never be silently dropped. Either override wins over the classification
/// label.
///
/// A `None` chat is `Monitored` (v4's `chat?.` optional chaining falls through
/// to the label test, and an absent label is not `=== true`).
pub fn get_concierge_state(chat: Option<&Value>) -> ConciergeState {
    let override_value = chat
        .and_then(|c| c.get("conciergeOverride"))
        .and_then(Value::as_str);
    if override_value == Some(CONCIERGE_OVERRIDE_UNCENSORED) {
        return ConciergeState::Uncensored;
    }
    if override_value == Some(CONCIERGE_OVERRIDE_OFF) {
        return ConciergeState::Vouched;
    }
    let flagged = chat
        .and_then(|c| c.get("isDangerousChat"))
        .and_then(Value::as_bool)
        == Some(true);
    if flagged {
        ConciergeState::Flagged
    } else {
        ConciergeState::Monitored
    }
}

/// Is this state on the uncensored row of the 2×2 — `Flagged` (the classifier's
/// verdict) or `Uncensored` (the operator's assertion)? (v4
/// `conciergeStateUsesUncensoredRoute`, `c43d3b1b4`.)
///
/// The state-only twin of [`should_use_uncensored_route`], for callers that
/// already hold a derived state (list payloads carry `conciergeState` rather
/// than the raw pair) and would otherwise have to fabricate a chat-like to ask
/// the question. This is THE one place that says which states take the
/// uncensored route; [`should_use_uncensored_route`] delegates to it.
pub fn concierge_state_uses_uncensored_route(state: ConciergeState) -> bool {
    state == ConciergeState::Flagged || state == ConciergeState::Uncensored
}

/// Should this chat take the Concierge's uncensored routes right now? (v4
/// `shouldUseUncensoredRoute`.)
///
/// True for `Flagged` (the classifier's verdict) and `Uncensored` (the
/// operator's assertion) — the two states on the uncensored row of the 2×2. Use
/// this everywhere the Concierge would reroute providers, pick candid over
/// concealed prompt guidance, or select an uncensored cheap-LLM.
pub fn should_use_uncensored_route(chat: Option<&Value>) -> bool {
    concierge_state_uses_uncensored_route(get_concierge_state(chat))
}

/// Should the UI paint this chat with danger styling (red rings, warning
/// accents)? (v4 `shouldShowDangerStyling`.)
///
/// True only for `Flagged`: the styling announces the *Concierge's* verdict. An
/// `Uncensored` chat takes the same routes by the operator's own hand and is
/// deliberately not painted as a hazard.
pub fn should_show_danger_styling(chat: Option<&Value>) -> bool {
    get_concierge_state(chat) == ConciergeState::Flagged
}

/// Is the classifier allowed to run on this chat at all? (v4
/// `isClassifierOnDuty`.)
///
/// True for the two Concierge-decides states (`Monitored`, `Flagged`); false for
/// both operator states — once the operator has spoken, nothing may reclassify
/// the chat out from under them. **True for a `None` chat** (nothing has taken
/// the classifier off the case), which v4 pins with its own test.
pub fn is_classifier_on_duty(chat: Option<&Value>) -> bool {
    let s = get_concierge_state(chat);
    s == ConciergeState::Monitored || s == ConciergeState::Flagged
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// v4's `chat-override.test.ts` TABLE, row for row: the full stored-field
    /// truth table with the preserved `isDangerousChat` label in each operator
    /// position (the label must not leak into any predicate).
    /// `(conciergeOverride, isDangerousChat, state, uncensoredRoute,
    /// dangerStyling, classifierOnDuty)`.
    type Row = (
        Option<&'static str>,
        Option<bool>,
        ConciergeState,
        bool,
        bool,
        bool,
    );

    const TABLE: &[Row] = &[
        (
            None,
            Some(false),
            ConciergeState::Monitored,
            false,
            false,
            true,
        ),
        (None, None, ConciergeState::Monitored, false, false, true),
        (None, Some(true), ConciergeState::Flagged, true, true, true),
        (
            Some("OFF"),
            Some(false),
            ConciergeState::Vouched,
            false,
            false,
            false,
        ),
        (
            Some("OFF"),
            Some(true),
            ConciergeState::Vouched,
            false,
            false,
            false,
        ),
        (
            Some("UNCENSORED"),
            Some(false),
            ConciergeState::Uncensored,
            true,
            false,
            false,
        ),
        (
            Some("UNCENSORED"),
            Some(true),
            ConciergeState::Uncensored,
            true,
            false,
            false,
        ),
    ];

    fn chat_of(over: Option<&str>, danger: Option<bool>) -> Value {
        json!({
            "conciergeOverride": over.map(Value::from).unwrap_or(Value::Null),
            "isDangerousChat": danger.map(Value::from).unwrap_or(Value::Null),
        })
    }

    #[test]
    fn truth_table_matches_v4() {
        for (over, danger, state, route, styling, on_duty) in TABLE {
            let chat = chat_of(*over, *danger);
            let c = Some(&chat);
            assert_eq!(
                get_concierge_state(c),
                *state,
                "state for {over:?}/{danger:?}"
            );
            assert_eq!(
                should_use_uncensored_route(c),
                *route,
                "route for {over:?}/{danger:?}"
            );
            assert_eq!(
                should_show_danger_styling(c),
                *styling,
                "styling for {over:?}/{danger:?}"
            );
            assert_eq!(
                is_classifier_on_duty(c),
                *on_duty,
                "onDuty for {over:?}/{danger:?}"
            );
        }
    }

    /// v4 `chat-override.test.ts` `describe('conciergeStateUsesUncensoredRoute')`
    /// (`c43d3b1b4`): the bottom row of the 2×2 and nothing else, plus the
    /// `it.each(TABLE)` agreement claim — the state-only twin answers exactly
    /// what the chat-shaped predicate answers, row for row.
    #[test]
    fn state_only_twin_is_the_bottom_row_and_agrees_with_the_chat_predicate() {
        assert!(!concierge_state_uses_uncensored_route(
            ConciergeState::Monitored
        ));
        assert!(!concierge_state_uses_uncensored_route(
            ConciergeState::Vouched
        ));
        assert!(concierge_state_uses_uncensored_route(
            ConciergeState::Flagged
        ));
        assert!(concierge_state_uses_uncensored_route(
            ConciergeState::Uncensored
        ));

        for (over, danger, state, route, _, _) in TABLE {
            assert_eq!(
                concierge_state_uses_uncensored_route(*state),
                *route,
                "state twin for {over:?}/{danger:?}"
            );
            let chat = chat_of(*over, *danger);
            assert_eq!(
                concierge_state_uses_uncensored_route(get_concierge_state(Some(&chat))),
                should_use_uncensored_route(Some(&chat)),
                "twin agrees with chat predicate for {over:?}/{danger:?}"
            );
        }
    }

    #[test]
    fn uncensored_routes_but_is_never_painted_as_a_hazard() {
        // The two predicates diverge exactly on 'uncensored'.
        for (over, danger, state, _, _, _) in TABLE {
            if *state != ConciergeState::Uncensored {
                continue;
            }
            let chat = chat_of(*over, *danger);
            assert!(should_use_uncensored_route(Some(&chat)));
            assert!(!should_show_danger_styling(Some(&chat)));
        }
    }

    #[test]
    fn none_chat_is_monitored_and_the_classifier_stays_on_the_case() {
        assert_eq!(get_concierge_state(None), ConciergeState::Monitored);
        assert!(!should_use_uncensored_route(None));
        assert!(!should_show_danger_styling(None));
        // v4: "nothing has taken the classifier off the case".
        assert!(is_classifier_on_duty(None));
    }

    #[test]
    fn empty_chat_is_monitored() {
        let chat = json!({});
        assert_eq!(get_concierge_state(Some(&chat)), ConciergeState::Monitored);
        assert!(is_classifier_on_duty(Some(&chat)));
    }

    #[test]
    fn wire_strings_round_trip() {
        for s in [
            ConciergeState::Monitored,
            ConciergeState::Flagged,
            ConciergeState::Vouched,
            ConciergeState::Uncensored,
        ] {
            assert_eq!(ConciergeState::from_wire(s.as_str()), Some(s));
        }
        // v4's z.enum refuses the retired tri-state spellings.
        for s in ["safe", "off", "on", "", "MONITORED"] {
            assert_eq!(ConciergeState::from_wire(s), None, "{s} must not decode");
        }
    }
}
