//! The trailing scene note on chained multi-character turns (v4
//! `lib/chat/context/user-narration-anchor.ts`, 105 lines at `ca363178d`).
//!
//! On a chained turn (the second responder onward, or a continue / nudge) no
//! new user message rides at the tail: the human's narration sits mid-history
//! and the newest thing in the prompt is another character's reply. When that
//! reply contradicts the narration — a stopped turn that finished server-side
//! and landed after the human's newer message, or a second tab posting
//! mid-chain — nothing tells the next character which account wins. This note
//! does: the human's latest message is the state of the scene.
//!
//! ## Where it lands
//!
//! Never in the cached prefix. The block-1 multi-character turn anchor
//! (`applyMultiCharacterTurnAnchor` — a different feature that only shares the
//! word "anchor") edits system block 1 and must never carry per-turn wording;
//! this is a trailing per-turn section on the uncached tail, pushed by
//! [`crate::services::build_context`] ahead of the progressions report and the
//! turn-skip note on the chained-turn path only. Conditional, not structural,
//! so neither `IDENTITY_STACK_BUILDER_VERSION` nor
//! `PROMPT_CACHE_STRUCTURE_VERSION` moves. Not persisted. (`historyTailHash`
//! does move on a turn that carries it — the former tail enters the hashed
//! history — exactly as it does in v4.)
//!
//! ## Empty is byte-for-byte nothing
//!
//! When the note does not apply this returns `""` and the caller pushes
//! nothing, the same contract [`crate::progressions::prompt_section`] keeps.
//!
//! ## Truthiness, not shape
//!
//! Every v4 test here is a JS truthiness test, and an empty string is falsy:
//! an `""` row id never matches a human id, an `""` participant id is not a
//! character, an `""` seat name falls back to `user_name`. The Rust below
//! spells each of those out rather than leaning on `Option::is_some`.
//!
//! Design of record: v4 `docs/developer/features/prompt-trust-and-anti-committee.md` §9.

use std::collections::HashSet;

/// v4's `CONTEXT` — the tracing target the one line below carries.
const CONTEXT: &str = "chat.context.user-narration-anchor";

/// One row of the history window as this module reads it — v4's
/// `{ role: string; id?: string; participantId?: string | null }`.
#[derive(Clone, Copy, Debug)]
pub struct NarrationWindowRow<'a> {
    pub role: &'a str,
    pub id: Option<&'a str>,
    pub participant_id: Option<&'a str>,
}

/// v4's `nameForParticipant` callback: a participant id → that seat's
/// character name, or `None` when it cannot be named.
pub type NameForParticipant<'a> = &'a dyn Fn(&str) -> Option<String>;

/// v4 `BuildUserNarrationAnchorInput`.
pub struct BuildUserNarrationAnchorInput<'a> {
    /// Multi-character chat? Single-character chats have no race to settle.
    /// (In `build_context` this is its own predicate, which COUNTS the user's
    /// persona seat — so a 1:1 chat with a persona seat qualifies.)
    pub is_multi_character: bool,
    /// True when this turn carries a new user message (first responder) — v4's
    /// `!!newUserMessage`, so an empty string is NOT a new message.
    pub has_new_user_message: bool,
    /// The history window this turn will send, oldest first — the POST-trim
    /// selection, never the pre-trim list. In a multi-character chat other
    /// characters' replies are attributed to role `user` with their
    /// participant id, so a character line is recognised by either signal.
    pub history_window: &'a [NarrationWindowRow<'a>],
    /// Row ids of the human's own turns (USER, no `systemSender`), captured
    /// before whisper normalization re-roles Staff whispers to USER. `None`
    /// and an empty set are the same thing: no note.
    pub human_turn_message_ids: Option<&'a HashSet<String>>,
    /// Fallback display name, resolved the way `{{user}}` is. Used only when
    /// the matched human message's author cannot be named (an unseated user).
    pub user_name: &'a str,
    /// Names the author of the matched human message from its participant id.
    /// The human may drive several seats, and the "Speaking As" selection is
    /// not necessarily who wrote the latest line, so the seat that wrote it
    /// wins.
    pub name_for_participant: Option<NameForParticipant<'a>>,
}

/// The note itself (v4 `renderUserNarrationAnchor`).
pub fn render_user_narration_anchor(user_name: &str) -> String {
    format!(
        "Scene note: {user_name}'s most recent message is the current state of the scene. \
Where any other speaker's line \u{2014} before or after it \u{2014} conflicts with what {user_name} narrated, \
{user_name}'s account is what happened. Adjust without arguing; what you do about it is yours."
    )
}

/// Returns the scene note for a chained multi-character turn where the human
/// has spoken and a character has answered since, or `""` otherwise (v4
/// `buildUserNarrationAnchor`).
pub fn build_user_narration_anchor(input: &BuildUserNarrationAnchorInput<'_>) -> String {
    if !input.is_multi_character || input.has_new_user_message {
        return String::new();
    }
    let Some(human_ids) = input.human_turn_message_ids.filter(|s| !s.is_empty()) else {
        return String::new();
    };
    // v4 `id && humanIds.has(id)` — `""` is no match.
    let is_human = |id: Option<&str>| id.is_some_and(|i| !i.is_empty() && human_ids.contains(i));

    let window = input.history_window;
    let Some(last_human_index) = window.iter().rposition(|m| is_human(m.id)) else {
        return String::new();
    };

    let character_spoke_since = window[last_human_index + 1..].iter().any(|m| {
        m.role.to_lowercase() == "assistant"
            || (m.participant_id.is_some_and(|p| !p.is_empty()) && !is_human(m.id))
    });
    if !character_spoke_since {
        return String::new();
    }

    // v4 `(authorParticipantId && nameForParticipant?.(authorParticipantId)) || userName`.
    let author_name = window[last_human_index]
        .participant_id
        .filter(|p| !p.is_empty())
        .and_then(|p| input.name_for_participant.and_then(|f| f(p)))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| input.user_name.to_string());

    // `resolvedFromSeat` is a NAME comparison, not "came from a seat": a seat
    // whose character shares the `{{user}}` name logs `false`.
    tracing::debug!(
        target: "chat.context.user-narration-anchor",
        context = CONTEXT,
        historyWindowSize = window.len(),
        lastHumanIndex = last_human_index,
        resolvedFromSeat = author_name != input.user_name,
        "[UserNarrationAnchor] Scene note applies to this chained turn"
    );
    render_user_narration_anchor(&author_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn row<'a>(role: &'a str, id: Option<&'a str>, pid: Option<&'a str>) -> NarrationWindowRow<'a> {
        NarrationWindowRow {
            role,
            id,
            participant_id: pid,
        }
    }

    fn ids(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    const OWEN: &str = "Scene note: Owen's most recent message is the current state of the scene. Where any other speaker's line \u{2014} before or after it \u{2014} conflicts with what Owen narrated, Owen's account is what happened. Adjust without arguing; what you do about it is yours.";

    fn base_window() -> Vec<NarrationWindowRow<'static>> {
        vec![
            row("USER", Some("u1"), Some("p-user")),
            row("ASSISTANT", Some("a1"), Some("p-a")),
            row("USER", Some("u2"), Some("p-user")),
            row("USER", Some("a2"), Some("p-b")),
        ]
    }

    fn build(
        multi: bool,
        has_new: bool,
        window: &[NarrationWindowRow<'_>],
        human: Option<&HashSet<String>>,
        user_name: &str,
        names: Option<&HashMap<&str, &str>>,
    ) -> String {
        let f = |p: &str| names.and_then(|m| m.get(p).map(|s| s.to_string()));
        let dynf: &dyn Fn(&str) -> Option<String> = &f;
        build_user_narration_anchor(&BuildUserNarrationAnchorInput {
            is_multi_character: multi,
            has_new_user_message: has_new,
            history_window: window,
            human_turn_message_ids: human,
            user_name,
            name_for_participant: names.map(|_| dynf),
        })
    }

    #[test]
    fn render_bytes_match_v4_literal() {
        assert_eq!(render_user_narration_anchor("Owen"), OWEN);
    }

    #[test]
    fn v4_eight_cases() {
        let h = ids(&["u1", "u2"]);
        let w = base_window();
        // applies
        assert_eq!(build(true, false, &w, Some(&h), "Owen", None), OWEN);
        // single-character
        assert_eq!(build(false, false, &w, Some(&h), "Owen", None), "");
        // first responder
        assert_eq!(build(true, true, &w, Some(&h), "Owen", None), "");
        // no human in window ×3
        assert_eq!(
            build(true, false, &w, Some(&ids(&["elsewhere"])), "Owen", None),
            ""
        );
        assert_eq!(build(true, false, &w, None, "Owen", None), "");
        assert_eq!(
            build(true, false, &w, Some(&HashSet::new()), "Owen", None),
            ""
        );
        // staff whisper only after the human
        let whisper = [
            row("USER", Some("u2"), Some("p-user")),
            row("USER", Some("host-1"), None),
        ];
        assert_eq!(build(true, false, &whisper, Some(&h), "Owen", None), "");
        // role assistant without a participant id
        let bare = [
            row("user", Some("u2"), None),
            row("assistant", Some("a9"), None),
        ];
        assert_ne!(build(true, false, &bare, Some(&h), "Owen", None), "");
        // seat wins
        let seats = [
            row("USER", Some("u1"), Some("p-user-2")),
            row("USER", Some("u2"), Some("p-user")),
            row("ASSISTANT", Some("a1"), Some("p-a")),
        ];
        let names: HashMap<&str, &str> = [("p-user", "Owen"), ("p-user-2", "Alex")].into();
        assert_eq!(
            build(true, false, &seats, Some(&h), "Alex", Some(&names)),
            OWEN
        );
        // unseated fallback (a callback that names nobody)
        let none: HashMap<&str, &str> = HashMap::new();
        assert_eq!(
            build(true, false, &w, Some(&h), "Alex", Some(&none)),
            render_user_narration_anchor("Alex")
        );
    }

    #[test]
    fn truthiness_rows() {
        let h = ids(&["u1", "u2"]);
        // An empty-string pid is not a character.
        let w = [
            row("user", Some("u2"), None),
            row("user", Some("x"), Some("")),
        ];
        assert_eq!(build(true, false, &w, Some(&h), "Owen", None), "");
        // An empty-string id never matches, but a pid with an "" id IS a character.
        let w = [
            row("user", Some("u2"), None),
            row("user", Some(""), Some("p-b")),
        ];
        assert_ne!(build(true, false, &w, Some(&h), "Owen", None), "");
        // A second human seat after the human is not a character (M5's row).
        let w = [
            row("user", Some("u1"), Some("p-user")),
            row("user", Some("u2"), Some("p-user-2")),
        ];
        assert_eq!(build(true, false, &w, Some(&h), "Owen", None), "");
        // Uppercase ASSISTANT counts.
        let w = [row("user", Some("u2"), None), row("ASSISTANT", None, None)];
        assert_ne!(build(true, false, &w, Some(&h), "Owen", None), "");
        // An empty seat name falls back.
        let names: HashMap<&str, &str> = [("p-user", "")].into();
        assert_eq!(
            build(true, false, &base_window(), Some(&h), "Alex", Some(&names)),
            render_user_narration_anchor("Alex")
        );
    }

    #[test]
    fn debug_line_values_per_arm() {
        let h = ids(&["u1", "u2"]);
        let w = base_window();
        let names: HashMap<&str, &str> = [("p-user", "Owen")].into();
        // Seat-named, different from the `{{user}}` name → true.
        let (_, lines) = crate::test_support::captured_with(|| {
            build(true, false, &w, Some(&h), "Alex", Some(&names))
        });
        assert_eq!(
            lines,
            vec!["DEBUG chat.context.user-narration-anchor [UserNarrationAnchor] Scene note applies to this chained turn context=chat.context.user-narration-anchor historyWindowSize=4 lastHumanIndex=2 resolvedFromSeat=true".to_string()]
        );
        // Seat-named but the SAME name → false (a name comparison, M7).
        let (_, lines) = crate::test_support::captured_with(|| {
            build(true, false, &w, Some(&h), "Owen", Some(&names))
        });
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("resolvedFromSeat=false"), "{lines:?}");
        // Silence on every non-applying arm.
        let (_, lines) = crate::test_support::captured_with(|| {
            build(false, false, &w, Some(&h), "Owen", None);
            build(true, true, &w, Some(&h), "Owen", None);
            build(true, false, &w, None, "Owen", None);
            build(true, false, &w[..3], Some(&h), "Owen", None);
        });
        assert!(lines.is_empty(), "{lines:?}");
    }
}
