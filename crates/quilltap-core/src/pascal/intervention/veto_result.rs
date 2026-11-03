//! Operator intervention — what a model is told when the operator vetoes its
//! `run_custom` call (v4 `lib/pascal/intervention/veto-result.ts`, spec I13).
//!
//! A plain character does not know the chat is a game, so it hears only that
//! the tool did not run — as generic as a flaky tool call, with nothing about a
//! table, a game or anyone deciding. Only a `systemTransparency` character, or
//! a user-controlled seat, hears that the table is closed. The one place these
//! strings are spelled.

/// For a character that does not know it is in a game.
pub const VETO_RESULT_PLAIN: &str = "The tool did not run.";

/// For a character that knows about Pascal and the staff.
pub const VETO_RESULT_TRANSPARENT: &str =
    "The table is closed for the moment; this tool did not run. Carry on without it.";

/// The veto's tool-result wording for a caller who does (or does not) know the
/// chat is a game.
pub fn veto_result_for(transparent: bool) -> &'static str {
    if transparent {
        VETO_RESULT_TRANSPARENT
    } else {
        VETO_RESULT_PLAIN
    }
}
