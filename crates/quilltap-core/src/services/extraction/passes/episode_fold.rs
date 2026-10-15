//! v4 `lib/memory/extraction/passes/episode-fold.ts` — the episode pass: ONE
//! call per stretch (`episode memory`, no `logCharacterId`), holders = the
//! cast members with a seat AND present, one batch per episode, the cap
//! `FOLD_EPISODE_CAP` raw (never `pass_cap`). Its `afterBatch` links each
//! INSERTED episode to the holder's per-turn fragments from the stretch —
//! never a reinforce / absorb (that names an existing row, perhaps one of the
//! very fragments — review of PR 94) and never a sibling episode of the same
//! run (finding 10).

use std::collections::HashSet;

use serde_json::Value;

use crate::admission::outcome::admission_inserted;
use crate::db::fallback::error_text;
use crate::db::memories_read::find_by_character_and_source_message_ids_or_empty;
use crate::db::DbError;
use crate::memory_extraction::prompts::episode::{build_episode_fold_messages, FOLD_EPISODE_CAP};
use crate::memory_extraction::window::{message_id, TranscriptWindow};
use crate::services::admission::link_both_ways;
use crate::services::extraction::context::{clock_at, ExtractionContext};
use crate::services::extraction::run::{AdmittedCandidate, CallParse, DraftBatch, PassCall};

/// Cap on fragment links attached to one episode (per character).
pub const MAX_FRAGMENT_LINKS: usize = 8;

pub(super) fn calls(ctx: &ExtractionContext, window: &TranscriptWindow) -> Vec<PassCall> {
    let TranscriptWindow::Stretch(window) = window else {
        return Vec::new();
    };
    if window.messages.is_empty() {
        return Vec::new();
    }
    let holders: Vec<String> = ctx
        .cast
        .iter()
        .filter(|m| m.participant_id.is_some() && m.present)
        .map(|m| m.character_id.clone())
        .collect();
    if holders.is_empty() {
        return Vec::new();
    }
    let last_stamped = window.messages.iter().rev().find(|m| {
        m.get("createdAt")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    });
    let source_message_id = last_stamped
        .or_else(|| window.messages.last())
        .map(|m| message_id(m).to_string());
    vec![PassCall {
        label: "episode memory".to_string(),
        holders: holders.clone(),
        messages: build_episode_fold_messages(&window.rendered, &clock_at(ctx, &window.now_iso)),
        log_character_id: None,
        parse: CallParse::Episodes {
            cap: FOLD_EPISODE_CAP,
            template: DraftBatch {
                holders,
                about: None,
                drafts: Vec::new(),
                floor: None,
                source_message_id,
                source_created_at: Some(window.now_iso.clone()),
                reference_iso: Some(window.now_iso.clone()),
                fallback_iso: Some(window.start_iso.clone()),
            },
        },
    }]
}

pub(super) async fn after_batch(
    written_episode_ids: &mut HashSet<String>,
    ctx: &ExtractionContext,
    window: &TranscriptWindow,
    admitted: &[AdmittedCandidate],
) {
    let TranscriptWindow::Stretch(window) = window else {
        return;
    };
    let episodes: Vec<&AdmittedCandidate> = admitted
        .iter()
        .filter(|a| a.outcome.memory.is_some() && admission_inserted(&a.outcome.action))
        .collect();
    for a in &episodes {
        if let Some(id) = memory_id(a) {
            written_episode_ids.insert(id);
        }
    }
    if ctx.dry_run {
        return;
    }
    let window_message_ids: Vec<String> = window
        .messages
        .iter()
        .map(|m| message_id(m).to_string())
        .collect();
    for a in episodes {
        let Some(memory) = a.outcome.memory.as_ref() else {
            continue;
        };
        let episode_id = memory_id(a).unwrap_or_default();
        let character_id = a.candidate.subject.holder_id.clone();
        let links: Vec<String> = memory
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if let Err(error) = link_fragments(
            ctx,
            &character_id,
            &episode_id,
            &links,
            &window_message_ids,
            written_episode_ids,
        )
        .await
        {
            tracing::warn!(
                chatId = %ctx.chat_id,
                characterId = %character_id,
                episodeId = %episode_id,
                error = %error_text(&error),
                "[Memory] Failed to link episode fragments"
            );
        }
    }
}

fn memory_id(a: &AdmittedCandidate) -> Option<String> {
    a.outcome
        .memory
        .as_ref()
        .and_then(|m| m.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// v4 `linkFragments` — link a character's per-turn fragments from the
/// stretch to its episode. Links are deltas against the rows at commit (bug
/// 231); only pairs not already linked on both sides are written. The
/// admitted row's `relatedMemoryIds` is the post-link row, so links admission
/// made are kept (bug 26). Episodes this run wrote are siblings, not
/// fragments (finding 10).
async fn link_fragments(
    ctx: &ExtractionContext,
    character_id: &str,
    episode_id: &str,
    episode_links: &[String],
    window_message_ids: &[String],
    written_episode_ids: &HashSet<String>,
) -> Result<usize, DbError> {
    let (cid, ids) = (character_id.to_string(), window_message_ids.to_vec());
    let found = ctx.db.read_main(move |c| {
        Ok(find_by_character_and_source_message_ids_or_empty(
            c, &cid, &ids,
        ))
    })?;
    let id_of = |m: &Value| {
        m.get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let fragments: Vec<&Value> = found
        .iter()
        .filter(|f| {
            let id = id_of(f);
            !written_episode_ids.contains(&id) && id != episode_id
        })
        .take(MAX_FRAGMENT_LINKS)
        .collect();
    if fragments.is_empty() {
        return Ok(0);
    }
    let names_episode = |f: &Value| {
        f.get("relatedMemoryIds")
            .and_then(Value::as_array)
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(episode_id)))
    };
    let newly_linked: Vec<String> = fragments
        .iter()
        .filter(|f| !names_episode(f))
        .map(|f| id_of(f))
        .collect();
    let link_to: Vec<String> = fragments
        .iter()
        .map(|f| id_of(f))
        .filter(|id| !episode_links.contains(id) || newly_linked.contains(id))
        .collect();
    if !link_to.is_empty() {
        link_both_ways(&ctx.db, character_id, episode_id, &link_to).await?;
    }
    tracing::debug!(
        characterId = %character_id,
        episodeId = %episode_id,
        fragments = fragments.len(),
        newlyLinked = newly_linked.len(),
        "[Memory] Episode fragments linked"
    );
    Ok(newly_linked.len())
}
