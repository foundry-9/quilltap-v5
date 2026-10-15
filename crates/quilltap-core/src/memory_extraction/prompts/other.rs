//! The OTHER prompt — port of v4 `lib/memory/extraction/prompts/other.ts`
//! (pin `83334b9a5`): what an observer retains about each subject, one call
//! per observer, at turn or fold grain. Body moved unchanged from the
//! retired `cheap-llm-tasks/memory-tasks.ts` (memory refactor Phase D).
//!
//! v4's why, carried: the body refers to "the observer" and "the subject"
//! throughout so the prompt prefix is byte-stable across every OTHER call —
//! providers' prefix caches can hit regardless of which (observer, subject)
//! pair is in play. The actual names and canon block sit in a CONTEXT footer
//! at the end. Each subject's `isUser` flag is rendered into that footer as a
//! parenthetical label so the model can distinguish the user-controlled
//! character from AI characters without branching the stable prompt body.

use super::prompt_text;
use super::shared::{
    render_clock_block, render_orienting_context, ExtractionClock, OrientingContext,
    AUTONOMOUS_ROOM_USER_ABSENCE_CLAUSE,
};
use crate::memory_extraction::window::{render_stretch, render_turn_context, StretchMessage};
use crate::memory_format::{format_name_with_pronouns, Pronouns};
use crate::model::completion::CompletionMessage;
use crate::services::turn_transcript::TurnTranscript;

/// Which grain an OTHER extraction reads (v4 `'turn' | 'fold'`): one turn, or
/// a folded STRETCH of many turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtractionGrain {
    Turn,
    Fold,
}

/// v4 `otherBodyForCap(perSubjectCap, grain = 'turn')`. The fold grain
/// prepends `FOLD_GRAIN_OTHER_BLOCK` + a blank line — a turn shows one beat of
/// a thread and yields fragments; a stretch shows the whole thread, so the
/// model is told to state each thread once, as its settled outcome.
fn other_body_for_cap(per_subject_cap: usize, grain: ExtractionGrain) -> String {
    let grain_block = match grain {
        ExtractionGrain::Fold => format!("{}\n\n", prompt_text::FOLD_GRAIN_OTHER_BLOCK),
        ExtractionGrain::Turn => String::new(),
    };
    format!(
        "{grain_block}{}{per_subject_cap}{}",
        prompt_text::OTHER_BODY_BEFORE_CAP,
        prompt_text::OTHER_BODY_AFTER_CAP
    )
}

/// One subject of a multi-subject OTHER extraction call (v4
/// `OtherSubjectInput`).
#[derive(Clone, Debug)]
pub struct OtherSubjectInput {
    /// Stable identifier the caller will use to route returned candidates.
    pub id: String,
    pub name: String,
    pub pronouns: Option<Pronouns>,
    /// True for the user-controlled character; false for AI characters.
    pub is_user: bool,
    /// Pre-rendered "ALREADY ESTABLISHED about <name>" block for this subject.
    pub canon_block: String,
}

/// v4 `getOtherMemoryExtractionPrompt`. The autonomous preamble comes BEFORE
/// the fold grain's GRAIN block.
fn get_other_memory_extraction_prompt(
    per_subject_cap: usize,
    observer_name: &str,
    subjects: &[OtherSubjectInput],
    in_autonomous_room: bool,
    orienting: Option<&OrientingContext>,
    clock: Option<&ExtractionClock>,
    grain: ExtractionGrain,
) -> String {
    let subjects_block = subjects
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let label = format_name_with_pronouns(&s.name, s.pronouns.as_ref());
            let user_tag = if s.is_user {
                " (the user-controlled character)"
            } else {
                ""
            };
            format!("SUBJECT {}: {label}{user_tag}\n{}", i + 1, s.canon_block)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let preamble = if in_autonomous_room {
        AUTONOMOUS_ROOM_USER_ABSENCE_CLAUSE
    } else {
        ""
    };
    let orienting_block = render_orienting_context(orienting);
    let clock_block = render_clock_block(clock);
    format!(
        "{preamble}{}\n\n{clock_block}{orienting_block}CONTEXT\nOBSERVER: {observer_name}\n\n{subjects_block}",
        other_body_for_cap(per_subject_cap, grain)
    )
}

/// v4 `OtherPromptInput`.
#[derive(Clone, Copy, Debug)]
pub struct OtherPromptInput<'a> {
    pub observer_name: &'a str,
    pub observer_pronouns: Option<&'a Pronouns>,
    pub subjects: &'a [OtherSubjectInput],
    /// Per-subject cap the prompt states (the parser enforces it).
    pub cap: usize,
    pub in_autonomous_room: bool,
    pub orienting: Option<&'a OrientingContext>,
    pub clock: Option<&'a ExtractionClock>,
}

/// The per-turn OTHER call's messages over one closed turn (v4
/// `buildOtherTurnMessages`).
pub fn build_other_turn_messages(
    input: &OtherPromptInput<'_>,
    transcript: &TurnTranscript,
) -> Vec<CompletionMessage> {
    let observer_label = format_name_with_pronouns(input.observer_name, input.observer_pronouns);
    vec![
        CompletionMessage::system(get_other_memory_extraction_prompt(
            input.cap,
            &observer_label,
            input.subjects,
            input.in_autonomous_room,
            input.orienting,
            input.clock,
            ExtractionGrain::Turn,
        )),
        CompletionMessage::user(render_turn_context(transcript)),
    ]
}

/// The fold-grain OTHER call's messages over a stretch (v4
/// `buildOtherFoldMessages`).
pub fn build_other_fold_messages(
    input: &OtherPromptInput<'_>,
    stretch: &[StretchMessage],
) -> Vec<CompletionMessage> {
    let observer_label = format_name_with_pronouns(input.observer_name, input.observer_pronouns);
    vec![
        CompletionMessage::system(get_other_memory_extraction_prompt(
            input.cap,
            &observer_label,
            input.subjects,
            input.in_autonomous_room,
            input.orienting,
            input.clock,
            ExtractionGrain::Fold,
        )),
        CompletionMessage::user(format!(
            "CONVERSATION STRETCH ({} messages, in the order spoken):\n\n{}",
            stretch.len(),
            render_stretch(stretch)
        )),
    ]
}
