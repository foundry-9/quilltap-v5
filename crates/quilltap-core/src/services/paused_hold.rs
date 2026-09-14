//! Paused-chat hold rule (v4 `lib/services/chat-message/paused-hold.ts`,
//! `31436bae4` bug 137).
//!
//! A paused conversation never moves on its own. The rule has two halves, and
//! they live in different places:
//!
//!  - **Nothing follows a turn.** `execute_turn_chain` / `should_chain_next` stop
//!    the rotation for as long as `isPaused` stands. That half predates this
//!    module (P4.D160, v4 bug 123).
//!  - **Nothing starts one either.** This predicate. A message typed into a
//!    paused room is recorded in full and answered by nobody; the floor waits
//!    where the user left it.
//!
//! Together they mean a paused room only ever speaks when the human asks it to,
//! one turn at a time, and only Resume lets it carry on by itself.

/// What the hold rule needs to know about a send (v4 `PausedHoldInput`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PausedHoldInput {
    /// True for the explicit summons — Nudge, Skip, the all-LLM modal's Continue,
    /// an autonomous-room turn. These ARE the human asking, so they run.
    pub is_continue_mode: bool,
    /// The chat's persisted `isPaused` as read at the top of the turn.
    pub chat_is_paused: bool,
    /// Autonomous rooms keep their own lifecycle (`runState`) and opt out of every
    /// user-facing pause; `isPaused` is not their flag to obey.
    ///
    /// `Option` because v4's field is optional (`neverPauseForUser?: boolean`) and
    /// the guard is `=== true`: absent and `false` are one answer. The spine
    /// passes `Some(options.never_pause_for_user)`; the absent arm exists so the
    /// differential can drive v4's whole input grid.
    pub never_pause_for_user: Option<bool>,
}

/// Whether this send should be recorded without drawing a reply
/// (v4 `shouldHoldUserTurnForPause`).
///
/// Note what is NOT consulted: whisper targets, an explicit responding
/// participant, attachments, staged tool results. A paused room holds every
/// typed message the same way — the only thing that takes the floor is a turn
/// the human summons.
#[must_use]
pub fn should_hold_user_turn_for_pause(input: PausedHoldInput) -> bool {
    if input.is_continue_mode {
        return false;
    }
    if input.never_pause_for_user == Some(true) {
        return false;
    }
    input.chat_is_paused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        is_continue_mode: bool,
        chat_is_paused: bool,
        never_pause_for_user: Option<bool>,
    ) -> PausedHoldInput {
        PausedHoldInput {
            is_continue_mode,
            chat_is_paused,
            never_pause_for_user,
        }
    }

    /// v4 `__tests__/unit/lib/services/chat-message/paused-hold.test.ts`, the four
    /// shapes by name (the differential drives the whole grid).
    #[test]
    fn holds_a_typed_message_while_the_chat_is_paused() {
        assert!(should_hold_user_turn_for_pause(input(false, true, None)));
    }

    #[test]
    fn lets_a_typed_message_through_when_the_chat_is_not_paused() {
        assert!(!should_hold_user_turn_for_pause(input(false, false, None)));
    }

    #[test]
    fn never_holds_a_continue_mode_summons_paused_or_not() {
        assert!(!should_hold_user_turn_for_pause(input(true, true, None)));
        assert!(!should_hold_user_turn_for_pause(input(true, false, None)));
    }

    #[test]
    fn never_holds_an_autonomous_room_turn() {
        assert!(!should_hold_user_turn_for_pause(input(
            false,
            true,
            Some(true)
        )));
    }

    /// A production-zone census: the spine consults this predicate EXACTLY ONCE.
    ///
    /// v4's `processMessage` computes `holdForPausedChat` off the FRESH chat read
    /// at the top of the turn and then reads the LOCAL four more times. A port
    /// that re-evaluated the predicate at the seam instead would be invisible to
    /// every differential — the corpus cannot change a chat row mid-turn, so both
    /// spellings agree on every case that can be written. The single consultation
    /// is therefore pinned structurally, in the `db_error_key_guard` idiom.
    #[test]
    fn the_spine_consults_the_predicate_once() {
        let src = include_str!("orchestrator.rs");
        let zone = src.split("\n#[cfg(test)]\n").next().unwrap_or(src);
        assert_eq!(
            zone.matches("should_hold_user_turn_for_pause(").count(),
            1,
            "the orchestrator must call the predicate exactly once (v4 computes \
             `holdForPausedChat` once and reads the local thereafter)"
        );
        // …and the local is what the four guards + the seam read: the binding, the
        // log branch, the three `!hold` conjuncts, and the seam's own `if`.
        let reads = zone
            .lines()
            .filter(|l| l.contains("hold_for_paused_chat") && !l.trim_start().starts_with("//"))
            .count();
        assert_eq!(
            reads, 6,
            "expected the binding plus five reads of the local (log branch, three \
             `!hold` conjuncts, the seam); found {reads}"
        );
    }

    /// The `=== true` guard: an explicit `false` is the same answer as absent.
    #[test]
    fn an_explicit_false_never_pause_flag_reads_as_absent() {
        assert!(should_hold_user_turn_for_pause(input(
            false,
            true,
            Some(false)
        )));
    }
}
