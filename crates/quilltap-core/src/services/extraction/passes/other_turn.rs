//! v4 `lib/memory/extraction/passes/other-turn.ts` — one per-turn OTHER call
//! per observer, with bug 245's observer rules: everyone who spoke in the
//! turn observes and is observed, lines an earlier extraction covered
//! included. A speaker from that earlier part has observed it already: it
//! reads only the new lines, and is not asked when nobody else said anything
//! new. A speaker new to the turn observes all of it.

use std::collections::HashSet;

use crate::memory_extraction::parse::{pass_cap, HARD_CANDIDATE_CAP};
use crate::memory_extraction::prompts::other::{build_other_turn_messages, OtherPromptInput};
use crate::memory_extraction::window::{first_valid_iso, TranscriptWindow};
use crate::services::extraction::context::{clock_at, CastMember, ExtractionContext};
use crate::services::extraction::passes::subjects::{subject_inputs_for, subject_pool};
use crate::services::extraction::run::{BatchFloor, CallParse, DraftBatch, PassCall};
use crate::services::turn_transcript::{with_prior_as_material, TurnCharacterSlice};

/// The hybrid floor's reason text (a debug-line fragment).
const HYBRID_FLOOR_REASON: &str = "Hybrid floor (the rest is left to the fold-grain pass)";

pub(super) fn calls(ctx: &ExtractionContext, window: &TranscriptWindow) -> Vec<PassCall> {
    let TranscriptWindow::Turn {
        transcript,
        now_iso,
    } = window
    else {
        return Vec::new();
    };
    let mode = &ctx.settings.mode;
    if mode.other_pass == "fold" {
        return Vec::new();
    }
    let floor = (mode.other_pass == "hybrid").then_some(BatchFloor {
        value: mode.per_turn_other_floor,
        reason: HYBRID_FLOOR_REASON,
    });
    let cap = pass_cap(HARD_CANDIDATE_CAP as f64, Some(ctx.route.max_tokens));
    let clock = clock_at(ctx, now_iso);
    let prior: &[TurnCharacterSlice] = transcript.prior_slices.as_deref().unwrap_or(&[]);
    let prior_speaker_ids: HashSet<&str> = prior.iter().map(|s| s.character_id.as_str()).collect();
    let new_speaker_ids: HashSet<&str> = transcript
        .character_slices
        .iter()
        .map(|s| s.character_id.as_str())
        .collect();
    // v4's `Map.set` over `[...prior, ...material]`: a key keeps its FIRST
    // position and its LAST slice.
    let mut observer_slices: Vec<(String, &TurnCharacterSlice)> = Vec::new();
    for slice in prior.iter().chain(transcript.character_slices.iter()) {
        let key = crate::memory_extraction::window::speaker_key(slice);
        match observer_slices.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = slice,
            None => observer_slices.push((key, slice)),
        }
    }
    let speakers: Vec<&CastMember> = observer_slices
        .iter()
        .filter_map(|(_, s)| ctx.cast_get(&s.character_id))
        .collect();
    let pool = subject_pool(ctx, &speakers);
    let whole_turn = with_prior_as_material(transcript);

    let mut calls = Vec::new();
    for (_, slice) in &observer_slices {
        let Some(observer) = ctx.cast_get(&slice.character_id) else {
            continue;
        };
        let saw_earlier_part = prior_speaker_ids.contains(observer.character_id.as_str());
        if saw_earlier_part
            && !new_speaker_ids
                .iter()
                .any(|id| *id != observer.character_id.as_str())
        {
            continue;
        }
        let view = if saw_earlier_part {
            transcript
        } else {
            &whole_turn
        };
        let subjects = subject_inputs_for(ctx, observer, &pool);
        if subjects.is_empty() {
            continue;
        }
        // An observer with no new line of its own observes the new lines:
        // date its memories by the turn's latest message, not its own earlier
        // one.
        let own_new = new_speaker_ids
            .contains(observer.character_id.as_str())
            .then_some(*slice);
        let source_message_id = own_new
            .and_then(|s| s.contributing_message_ids.last().cloned())
            .or_else(|| transcript.latest_assistant_message_id.clone());
        let source_time = first_valid_iso(&[
            own_new.and_then(|s| s.last_message_created_at.as_deref()),
            transcript.turn_timestamp.as_deref(),
        ]);
        let anchor = source_time.clone().unwrap_or_else(|| now_iso.clone());
        let inputs: Vec<_> = subjects.iter().map(|s| s.input.clone()).collect();
        let messages = build_other_turn_messages(
            &OtherPromptInput {
                observer_name: &observer.name,
                observer_pronouns: observer.pronouns.as_ref(),
                subjects: &inputs,
                cap,
                in_autonomous_room: ctx.in_autonomous_room,
                orienting: Some(&ctx.orienting),
                clock: Some(&clock),
            },
            view,
        );
        calls.push(PassCall {
            label: format!(
                "OTHER memory {}{} → {} subject(s)",
                observer.name,
                if slice.is_user_controlled {
                    " (user-controlled)"
                } else {
                    ""
                },
                subjects.len()
            ),
            holders: vec![observer.character_id.clone()],
            messages,
            log_character_id: Some(observer.character_id.clone()),
            parse: CallParse::OtherBySubject {
                cap,
                subject_ids: subjects
                    .iter()
                    .map(|s| s.member.character_id.clone())
                    .collect(),
                template: DraftBatch {
                    holders: vec![observer.character_id.clone()],
                    about: None,
                    drafts: Vec::new(),
                    floor: floor.clone(),
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
