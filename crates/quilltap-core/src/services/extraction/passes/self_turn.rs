//! v4 `lib/memory/extraction/passes/self-turn.ts` — one SELF call per
//! `characterSlices` entry whose holder is in the cast. With a bug-245 bound
//! that is the speakers with NEW lines only.

use crate::memory_extraction::parse::{pass_cap, HARD_CANDIDATE_CAP};
use crate::memory_extraction::prompts::self_turn::{build_self_turn_messages, SelfTurnPromptInput};
use crate::memory_extraction::window::{first_valid_iso, TranscriptWindow};
use crate::services::extraction::context::{clock_at, ExtractionContext};
use crate::services::extraction::run::{CallParse, DraftBatch, PassCall};

pub(super) fn calls(ctx: &ExtractionContext, window: &TranscriptWindow) -> Vec<PassCall> {
    let TranscriptWindow::Turn {
        transcript,
        now_iso,
    } = window
    else {
        return Vec::new();
    };
    let cap = pass_cap(HARD_CANDIDATE_CAP as f64, Some(ctx.route.max_tokens));
    let clock = clock_at(ctx, now_iso);
    let mut calls = Vec::new();
    for slice in &transcript.character_slices {
        let Some(holder) = ctx.cast_get(&slice.character_id) else {
            continue;
        };
        let canon = ctx.canon_for_self(&slice.character_id);
        let Some(messages) = build_self_turn_messages(&SelfTurnPromptInput {
            transcript,
            holder_id: &slice.character_id,
            canon_block: &canon,
            cap,
            in_autonomous_room: ctx.in_autonomous_room,
            orienting: Some(&ctx.orienting),
            clock: Some(&clock),
        }) else {
            continue;
        };
        let source_message_id = slice
            .contributing_message_ids
            .last()
            .cloned()
            .or_else(|| transcript.latest_assistant_message_id.clone());
        let source_time = first_valid_iso(&[
            slice.last_message_created_at.as_deref(),
            transcript.turn_timestamp.as_deref(),
        ]);
        let anchor = source_time.clone().unwrap_or_else(|| now_iso.clone());
        calls.push(PassCall {
            label: format!(
                "SELF memory for {}{}",
                holder.name,
                if slice.is_user_controlled {
                    " (user-controlled)"
                } else {
                    ""
                }
            ),
            holders: vec![holder.character_id.clone()],
            messages,
            log_character_id: Some(holder.character_id.clone()),
            parse: CallParse::SelfDrafts {
                cap,
                template: DraftBatch {
                    holders: vec![holder.character_id.clone()],
                    about: None,
                    drafts: Vec::new(),
                    floor: None,
                    source_message_id,
                    source_created_at: source_time,
                    reference_iso: Some(anchor.clone()),
                    fallback_iso: Some(anchor),
                },
            },
        });
    }
    calls
}
