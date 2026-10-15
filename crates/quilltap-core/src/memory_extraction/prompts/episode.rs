//! The episode prompt — port of v4 `lib/memory/extraction/prompts/episode.ts`
//! (pin `83334b9a5`): 0–[`FOLD_EPISODE_CAP`] consolidated, dated episode
//! records over a folded stretch. Body moved unchanged from the retired
//! `cheap-llm-tasks/memory-tasks.ts` (memory refactor Phase D), minus the
//! `participants` field line `e0ac7493a` removed. The CLOCK line rides
//! inline, now with its memory-calendar date stamp (bug 212).

use super::prompt_text;
use super::shared::{render_clock_stamp, ExtractionClock};
use crate::memory_extraction::window::{render_stretch, StretchMessage};
use crate::model::completion::CompletionMessage;

/// Hard cap on consolidated episodes per fold. The episode pass uses it RAW
/// — never through `pass_cap`, whatever v4 `parse.ts`'s comment says (code
/// wins; work order P4.D296 "What the recorded description got wrong").
pub const FOLD_EPISODE_CAP: usize = 2;

/// v4 `FOLD_EPISODE_PROMPT`, split at its one interpolation in the generated
/// [`prompt_text`] module.
pub fn fold_episode_prompt() -> String {
    format!(
        "{}{FOLD_EPISODE_CAP}{}",
        prompt_text::FOLD_EPISODE_PROMPT_BEFORE_CAP,
        prompt_text::FOLD_EPISODE_PROMPT_AFTER_CAP
    )
}

/// The episode call's messages over a stretch (v4 `buildEpisodeFoldMessages`).
/// Every call's bytes moved in the span, even in UTC: the CLOCK line is now
/// `current date/time <stamp>` with the calendar date and weekday.
pub fn build_episode_fold_messages(
    stretch: &[StretchMessage],
    clock: &ExtractionClock,
) -> Vec<CompletionMessage> {
    let clock_line = format!(
        "CLOCK: current date/time {}; timeline mode {}",
        render_clock_stamp(clock),
        clock.timeline_mode
    );
    vec![
        CompletionMessage::system(fold_episode_prompt()),
        CompletionMessage::user(format!(
            "{clock_line}\n\nDATED TURNS:\n\n{}",
            render_stretch(stretch)
        )),
    ]
}
