//! The SELF prompt — port of v4 `lib/memory/extraction/prompts/self.ts`
//! (pin `83334b9a5`): what the subject retains about themselves after a
//! turn. Body moved unchanged from the retired `cheap-llm-tasks/memory-tasks.ts`
//! (memory refactor Phase D). The `self-turn` pass (and Carina's answers,
//! which run on it) assembles its messages here.

use super::prompt_text;
use super::shared::{
    render_clock_block, render_orienting_context, ExtractionClock, OrientingContext,
    AUTONOMOUS_ROOM_USER_ABSENCE_CLAUSE,
};
use crate::memory_extraction::window::render_turn_context;
use crate::memory_format::format_name_with_pronouns;
use crate::model::completion::CompletionMessage;
use crate::services::turn_transcript::TurnTranscript;

/// v4 `selfBodyForCap`. The body refers to "the subject" throughout so the
/// prompt prefix is byte-stable across every SELF call — providers' prefix
/// caches can hit regardless of which character is the subject. The actual
/// subject name and canon block sit in a CONTEXT footer at the end, where
/// divergence doesn't break upstream caching.
fn self_body_for_cap(max_memories: usize) -> String {
    format!(
        "{}{max_memories}{}",
        prompt_text::SELF_BODY_BEFORE_CAP,
        prompt_text::SELF_BODY_AFTER_CAP
    )
}

/// Prepended to the SELF prompt when the subject is a character a human is
/// driving directly (its turn slice was authored by the user). Human
/// role-play is frequently first person ("I told her I wouldn't go"),
/// whereas the SELF extractor is otherwise tuned on third-person-ish
/// assistant prose. This clause binds first-person pronouns in the subject's
/// own lines to the subject so the extractor attributes those decisions and
/// realizations correctly.
///
/// It is a *prepended* preamble (like the autonomous clause) so the cached
/// body prefix stays byte-stable for the common AI path; user-controlled
/// SELF calls simply form their own cache lineage.
const FIRST_PERSON_USER_CLAUSE: &str =
    "IMPORTANT: The SUBJECT below is a character a human is playing directly, so \
the SUBJECT's lines in the transcript may be written in the first person. \
Read every \"I\", \"me\", \"my\", and \"myself\" in the SUBJECT's own lines as \
referring to the SUBJECT — attribute those decisions, realizations, and \
actions to the SUBJECT, not to anyone else in the exchange. \
The SUBJECT's lines came first in this turn and the SUBJECT has not yet \
responded to anything the characters said after them. Never record the \
SUBJECT as having accepted, agreed to, or consented to anything proposed \
in those later lines.\n\n";

/// v4 `getSelfMemoryExtractionPrompt`.
fn get_self_memory_extraction_prompt(
    max_memories: usize,
    observer_name: &str,
    canon_block: &str,
    in_autonomous_room: bool,
    orienting: Option<&OrientingContext>,
    is_user_controlled: bool,
    clock: Option<&ExtractionClock>,
) -> String {
    let mut preamble = String::new();
    if is_user_controlled {
        preamble.push_str(FIRST_PERSON_USER_CLAUSE);
    }
    if in_autonomous_room {
        preamble.push_str(AUTONOMOUS_ROOM_USER_ABSENCE_CLAUSE);
    }
    let orienting_block = render_orienting_context(orienting);
    let clock_block = render_clock_block(clock);
    format!(
        "{preamble}{}\n\n{clock_block}{orienting_block}CONTEXT\nSUBJECT: {observer_name}\n\n{canon_block}",
        self_body_for_cap(max_memories)
    )
}

/// v4 `SelfTurnPromptInput`.
#[derive(Clone, Copy, Debug)]
pub struct SelfTurnPromptInput<'a> {
    pub transcript: &'a TurnTranscript,
    /// The slice whose holder the memories are for.
    pub holder_id: &'a str,
    pub canon_block: &'a str,
    /// Candidates asked for (the prompt states it; the parser enforces it).
    pub cap: usize,
    pub in_autonomous_room: bool,
    pub orienting: Option<&'a OrientingContext>,
    pub clock: Option<&'a ExtractionClock>,
}

/// The SELF call's messages, or `None` when the holder has no slice in the
/// turn (v4 `buildSelfTurnMessages`). Only `character_slices` count — with a
/// bug-245 bound that is the speakers with NEW lines.
pub fn build_self_turn_messages(input: &SelfTurnPromptInput<'_>) -> Option<Vec<CompletionMessage>> {
    let target = input
        .transcript
        .character_slices
        .iter()
        .find(|s| s.character_id == input.holder_id)?;
    let target_label =
        format_name_with_pronouns(&target.character_name, target.character_pronouns.as_ref());
    Some(vec![
        CompletionMessage::system(get_self_memory_extraction_prompt(
            input.cap,
            &target_label,
            input.canon_block,
            input.in_autonomous_room,
            input.orienting,
            target.is_user_controlled,
            input.clock,
        )),
        CompletionMessage::user(render_turn_context(input.transcript)),
    ])
}
