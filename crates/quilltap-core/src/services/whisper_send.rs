//! Operator whispers — how a whisper send differs from a public post (v4
//! `lib/services/chat-message/whisper-send.ts`, `036b32a78` bug 258).
//!
//! Pure; two primitives rather than v4's `Pick<SendMessageOptions, …>` so both
//! the core's `SendMessageOptions` and the host's send request can ask it.

/// Is this send the operator whispering to one or more characters (the Whisper
/// dialog), rather than a public post or a chained turn? (v4
/// `isOperatorWhisperSend`.)
///
/// A whisper is an aside, not a round: its target answers once and the floor
/// goes back to the operator. It also waits for any stream already running in
/// the chat instead of opening a second speaker beside it. Before bug 258 it
/// ran a whole turn chain of its own, in a stream the Salon never displayed,
/// alongside whatever chain was already going, both drawing from the same turn
/// queue and rotation.
pub fn is_operator_whisper_send(
    continue_mode: bool,
    target_participant_ids: Option<&[String]>,
) -> bool {
    !continue_mode && target_participant_ids.is_some_and(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    //! v4 `__tests__/unit/lib/services/chat-message/whisper-send.test.ts` 1:1.
    //! The differential over v4's REAL module is `turn_control_equivalence`'s
    //! `whisper` rows.
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn is_true_for_a_fresh_send_with_whisper_targets() {
        assert!(is_operator_whisper_send(false, Some(&ids(&["p-1"]))));
        assert!(is_operator_whisper_send(false, Some(&ids(&["p-1", "p-2"]))));
    }

    #[test]
    fn is_false_for_a_public_send() {
        assert!(!is_operator_whisper_send(false, None));
        assert!(!is_operator_whisper_send(false, Some(&[])));
    }

    #[test]
    fn is_false_for_a_chained_or_continued_turn_whatever_it_carries() {
        assert!(!is_operator_whisper_send(true, Some(&ids(&["p-1"]))));
    }
}
