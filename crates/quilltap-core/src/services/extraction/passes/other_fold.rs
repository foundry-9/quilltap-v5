//! v4 `lib/memory/extraction/passes/other-fold.ts` — the fold-grain OTHER
//! pass: one call per speaker of the stretch (cast members matched by
//! `participantId`, in order of first line), the cap `pass_cap(
//! foldCandidatesPerSubject, maxTokens)` — model-visible: a profile at or
//! under 8,000 tokens states `up to 1` (work order P4.D296 L-3).

use serde_json::Value;

use crate::memory_extraction::parse::pass_cap;
use crate::memory_extraction::prompts::other::{build_other_fold_messages, OtherPromptInput};
use crate::memory_extraction::window::{first_valid_iso, message_id, TranscriptWindow};
use crate::services::extraction::context::{clock_at, CastMember, ExtractionContext};
use crate::services::extraction::passes::subjects::{subject_inputs_for, subject_pool};
use crate::services::extraction::run::{CallParse, DraftBatch, PassCall};

/// The characters who spoke in a stretch, in order of first line, and each
/// one's last message (v4 `stretchSpeakers`).
pub fn stretch_speakers<'a>(
    ctx: &'a ExtractionContext,
    messages: &'a [Value],
) -> (Vec<&'a CastMember>, Vec<(String, &'a Value)>) {
    let mut speakers: Vec<&CastMember> = Vec::new();
    let mut last_message: Vec<(String, &Value)> = Vec::new();
    for m in messages {
        let Some(pid) = m
            .get("participantId")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
        else {
            continue;
        };
        // v4 `new Map(cast.filter(m => m.participantId).map(m => [pid, m]))` —
        // a later member on the same seat wins.
        let Some(member) = ctx
            .cast
            .iter()
            .rev()
            .find(|c| c.participant_id.as_deref() == Some(pid))
        else {
            continue;
        };
        match last_message
            .iter_mut()
            .find(|(id, _)| *id == member.character_id)
        {
            Some(entry) => entry.1 = m,
            None => {
                speakers.push(member);
                last_message.push((member.character_id.clone(), m));
            }
        }
    }
    (speakers, last_message)
}

pub(super) fn calls(ctx: &ExtractionContext, window: &TranscriptWindow) -> Vec<PassCall> {
    let TranscriptWindow::Stretch(window) = window else {
        return Vec::new();
    };
    if window.messages.is_empty() {
        return Vec::new();
    }
    let cap = pass_cap(
        ctx.settings.mode.fold_candidates_per_subject as f64,
        Some(ctx.route.max_tokens),
    );
    let clock = clock_at(ctx, &window.now_iso);
    let (speakers, last_message) = stretch_speakers(ctx, &window.messages);
    let pool = subject_pool(ctx, &speakers);
    if speakers.is_empty() || pool.len() < 2 {
        return Vec::new();
    }
    let mut calls = Vec::new();
    for observer in &speakers {
        let subjects = subject_inputs_for(ctx, observer, &pool);
        if subjects.is_empty() {
            continue;
        }
        let last = last_message
            .iter()
            .find(|(id, _)| *id == observer.character_id)
            .map(|(_, m)| *m)
            .or_else(|| window.messages.last());
        let source_time = first_valid_iso(&[last
            .and_then(|m| m.get("createdAt"))
            .and_then(Value::as_str)]);
        // An undated draft happened by the observer's last line here, the
        // time its row is stamped with — not the stretch's end (bug 238).
        let anchor = source_time
            .clone()
            .unwrap_or_else(|| window.now_iso.clone());
        let inputs: Vec<_> = subjects.iter().map(|s| s.input.clone()).collect();
        let messages = build_other_fold_messages(
            &OtherPromptInput {
                observer_name: &observer.name,
                observer_pronouns: observer.pronouns.as_ref(),
                subjects: &inputs,
                cap,
                in_autonomous_room: ctx.in_autonomous_room,
                orienting: Some(&ctx.orienting),
                clock: Some(&clock),
            },
            &window.rendered,
        );
        calls.push(PassCall {
            label: format!(
                "fold OTHER memory {} → {} subject(s)",
                observer.name,
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
                    floor: None,
                    source_message_id: last.map(|m| message_id(m).to_string()),
                    source_created_at: source_time,
                    reference_iso: Some(anchor.clone()),
                    fallback_iso: Some(anchor),
                },
            },
        });
    }
    calls
}
