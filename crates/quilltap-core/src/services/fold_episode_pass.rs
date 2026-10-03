//! Fold-time episode pass (episodic spine — creation-side keystone) — v4
//! `lib/memory/fold-episode-pass.ts`.
//!
//! Per-turn extraction sees one turn at a time and produces fragments; a real
//! outing spans many turns and deserves one coherent record. On the existing
//! fold cadence (piggybacking [`crate::services::context_summary`], no new
//! trigger), this pass asks the cheap LLM for 0–[`FOLD_EPISODE_CAP`]
//! consolidated episode records over the just-folded message window, writes
//! them as `kind: 'episodic'` memories for each present character through the
//! normal gate (the gate's date guard keeps them from being swallowed by
//! near-dup skips), and links the per-turn fragment memories from the same
//! window via `relatedMemoryIds` so one-hop expansion can pull the fragments
//! when the episode surfaces.
//!
//! Best-effort throughout: never fails into the fold; a failed episode pass
//! costs nothing but the episode.
//!
//! **What it says** (P4.144) — v4's three REACHABLE `[FoldEpisodePass]` lines,
//! at v4's levels and bytes, fields camelCase in v4's order, under this
//! module's default target, proven against v4's own logger output by
//! `fold_episode_tier3_equivalence`:
//!
//! - `Episode extraction failed` (WARN, `fold-episode-pass.ts:103-106`) — on
//!   the `!extraction.success` arm ONLY. A `[]` reply and an unparseable one
//!   are both `success: true` (`parseFoldEpisodes` swallows bad JSON into `[]`
//!   from its own catch) and silent.
//! - `Failed to write episode for character` (WARN, `:201-205`) — the
//!   per-character catch. It sits OUTSIDE both link loops, so a failed gate
//!   write OR a failed link update ends THAT character's remaining linking;
//!   the next character proceeds. See [`write_episode_for_character`].
//! - `Episode pass complete` (INFO, `:210-215`) — once, after the episode
//!   loop, whenever the extraction was non-empty (even if every write failed).
//!
//! **Two v4 lines are deliberately NOT ported — they cannot fire** (survey
//! P4.144 §A2), so nobody should add them later:
//!
//! - `[FoldEpisodePass] Episode pass failed (non-fatal)` (WARN,
//!   `fold-episode-pass.ts:218-221`), the pass's outer catch. Every expression
//!   in the outer `try` that sits outside the per-character `try` is a
//!   FALLBACK read (`chats.findById` is `_findById`, a `null`-fallback
//!   `safeQuery`), a pure helper (`resolveEpisodicAnchors`, the renders, the
//!   entity lower-casing over `strArray`-guaranteed strings), a function that
//!   catches its own awaits (`resolveSpeakerNames`), or an executor that
//!   returns failures as VALUES (`extractEpisodesFromFold` →
//!   `executeCheapLLMTask`). Only a defect in a pure helper or in
//!   `getRepositories()` could reach it.
//! - `[Context Summary] Fold episode pass failed:` (ERROR, a 3-arg
//!   `logger.error(message, { chatId }, err)`, the trailing colon part of its
//!   bytes; `lib/chat/context-summary.ts:566`), the fold's catch around this
//!   pass — `runFoldEpisodePass` never rejects, so v5's seams take the
//!   infallible [`FoldEpisodePassResult`] and have no site for it.

use serde_json::Value;

use crate::chat_predicates::{is_participant_present, participant_status_from_str};
use crate::cheap_llm::CheapLlmSelection;
use crate::db::fallback::error_text;
use crate::db::memories::MemUpdate;
use crate::db::runtime::Db;
use crate::db::{chats_read, memories_read, DbError};
use crate::episodic::resolve_when_phrase;
use crate::memory_tasks::{
    build_fold_episode_messages, parse_fold_episodes, ExtractionClock, FoldEpisode,
    FoldEpisodeMessage,
};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::cheap_llm_exec::CheapLlmTaskOptions;
use crate::services::memory_gate::{
    create_memory_with_gate, CreateMemoryOptions, GateAction, MemoryServiceOptions,
};
use crate::services::speaker_names::{resolve_speaker_names, speaker_label};

/// Cap on fragment links attached to one episode (per character) — v4
/// `MAX_FRAGMENT_LINKS`.
const MAX_FRAGMENT_LINKS: usize = 8;

/// One message of the folded window (v4's `MessageEvent` subset this pass
/// reads).
#[derive(Clone, Debug)]
pub struct FoldWindowMessage {
    pub id: String,
    /// `"USER"` / `"ASSISTANT"` — only `"USER"` is distinguished (the speaker
    /// fallback label).
    pub role: String,
    /// v4 `m.content ?? ''`.
    pub content: Option<String>,
    pub participant_id: Option<String>,
    pub created_at: Option<String>,
}

/// v4 `RunFoldEpisodePassInput`.
#[derive(Clone, Debug)]
pub struct RunFoldEpisodePassInput {
    pub chat_id: String,
    pub user_id: String,
    /// The just-folded window: USER + character messages, chronological.
    pub window_messages: Vec<FoldWindowMessage>,
    pub timeline_mode: String,
    pub project_id: Option<String>,
    pub in_autonomous_room: bool,
}

/// v4 `FoldEpisodePassResult`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FoldEpisodePassResult {
    pub episodes_extracted: usize,
    pub memories_written: usize,
    pub fragments_linked: usize,
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// The per-episode values every character's write of that episode shares.
struct EpisodeWrite<'a> {
    episode: &'a FoldEpisode,
    occurred_at: &'a str,
    narrative_time: &'a Option<String>,
    source_message_id: &'a Option<String>,
    window_message_ids: &'a [String],
}

fn related_ids(memory: &Value) -> Vec<String> {
    memory
        .get("relatedMemoryIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Run the episode pass over a folded window (v4 `runFoldEpisodePass`). See the
/// module docs. Every failure path is swallowed — the result simply reports
/// fewer episodes.
pub async fn run_fold_episode_pass<C: CompletionProvider, E: EmbeddingProvider>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    selection: &CheapLlmSelection,
    input: &RunFoldEpisodePassInput,
) -> FoldEpisodePassResult {
    let mut result = FoldEpisodePassResult::default();

    if input.window_messages.is_empty() {
        return result;
    }

    // v4 `repos.chats.findById` is `_findById`, a FALLBACK read: a failed read
    // logs `Error finding entity by ID` and answers `null`, which takes the
    // `if (!chat) return result` arm.
    let chat_id = input.chat_id.clone();
    let Ok(Some(chat)) =
        db.read_main(move |conn| Ok(chats_read::find_by_id_or_none(conn, &chat_id)))
    else {
        return result;
    };
    let participants: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // Present character participants form the episode memory — the same set
    // whose recall the whisper feeds. User-controlled characters form memories
    // too (matching the per-turn SELF pass).
    let present_participants: Vec<&Value> = participants
        .iter()
        .filter(|p| {
            str_field(p, "type") == Some("CHARACTER")
                && is_participant_present(participant_status_from_str(str_field(p, "status")))
                && str_field(p, "characterId").is_some_and(|c| !c.is_empty())
        })
        .collect();
    if present_participants.is_empty() {
        return result;
    }

    // Anchor the clock to the window's newest message so historical folds
    // (regeneration sweeps) date correctly.
    let last_stamped = input
        .window_messages
        .iter()
        .rev()
        .find(|m| m.created_at.as_deref().is_some_and(|s| !s.is_empty()));
    let clock = ExtractionClock {
        now_iso: last_stamped
            .and_then(|m| m.created_at.clone())
            .unwrap_or_else(crate::clock::now_iso),
        timeline_mode: input.timeline_mode.clone(),
        // v4 builds the fold clock with only `nowIso` + `timelineMode`.
        narrative_now: None,
    };

    // Resolve speaker names per participant. Shared with the context-summary
    // fold — see `services::speaker_names` for why there is only one of these
    // (v4 `e7821606f` deleted this pass's private loop; its `if (character)`
    // gate became the shared `if (character?.name)`).
    let speaker_names = db
        .read_main(|conn| Ok(resolve_speaker_names(conn, &participants)))
        .unwrap_or_default();

    let rendered: Vec<FoldEpisodeMessage> = input
        .window_messages
        .iter()
        .map(|m| FoldEpisodeMessage {
            speaker: speaker_label(m.participant_id.as_deref(), &m.role, &speaker_names),
            content: m.content.clone().unwrap_or_default(),
            created_at: m.created_at.clone(),
        })
        .collect();

    let Some(messages) = build_fold_episode_messages(&rendered, &clock) else {
        return result;
    };
    let extraction = executor
        .execute(
            completion,
            selection,
            messages,
            parse_fold_episodes,
            None,
            None,
            None,
            Some("fold-episode-extraction"),
            CheapLlmTaskOptions::default(),
        )
        .await;
    // v4 `:102-109`: a FAILED extraction warns; a successful-but-empty one is
    // silent. v5's executor sets `error` on every failure arm, so the field is
    // never empty here (v4's is `getErrorMessage(error)`, a string on every arm).
    if !extraction.success {
        tracing::warn!(
            chatId = %input.chat_id,
            error = %extraction.error.as_deref().unwrap_or_default(),
            "[FoldEpisodePass] Episode extraction failed"
        );
        return result;
    }
    let episodes = match extraction.result {
        Some(episodes) if !episodes.is_empty() => episodes,
        _ => return result,
    };
    result.episodes_extracted = episodes.len();

    // First message timestamp of the window — the default occurredAt when the
    // model's `when` phrase doesn't resolve.
    let window_start_iso = input
        .window_messages
        .iter()
        .find(|m| m.created_at.as_deref().is_some_and(|s| !s.is_empty()))
        .and_then(|m| m.created_at.clone())
        .unwrap_or_else(|| clock.now_iso.clone());
    let window_message_ids: Vec<String> =
        input.window_messages.iter().map(|m| m.id.clone()).collect();
    let source_message_id = last_stamped
        .map(|m| m.id.clone())
        .or_else(|| input.window_messages.last().map(|m| m.id.clone()));

    for episode in &episodes {
        let resolved = episode
            .when
            .as_deref()
            .and_then(|w| resolve_when_phrase(Some(w), &clock.now_iso));
        let occurred_at = resolved.unwrap_or_else(|| window_start_iso.clone());
        let narrative_time = if input.timeline_mode == "narrative" {
            episode
                .narrative_time
                .clone()
                .or_else(|| episode.when.clone())
        } else {
            episode.narrative_time.clone()
        };

        let write = EpisodeWrite {
            episode,
            occurred_at: &occurred_at,
            narrative_time: &narrative_time,
            source_message_id: &source_message_id,
            window_message_ids: &window_message_ids,
        };
        for participant in &present_participants {
            let character_id = str_field(participant, "characterId").unwrap_or_default();
            // v4 wraps each character's write in its own try/catch (`:132-206`).
            if let Err(error) =
                write_episode_for_character(db, embedding, input, &write, character_id, &mut result)
                    .await
            {
                tracing::warn!(
                    chatId = %input.chat_id,
                    characterId = %character_id,
                    error = %error_text(&error),
                    "[FoldEpisodePass] Failed to write episode for character"
                );
            }
        }
    }

    tracing::info!(
        chatId = %input.chat_id,
        episodesExtracted = result.episodes_extracted,
        memoriesWritten = result.memories_written,
        fragmentsLinked = result.fragments_linked,
        "[FoldEpisodePass] Episode pass complete"
    );
    result
}

/// One character's write of one episode — v4's per-character `try` body
/// (`fold-episode-pass.ts:132-200`) as ONE fallible block, so its catch
/// placement carries over: the caller's `Err` arm is v4's `catch`, which sits
/// OUTSIDE both link loops. A failed gate write, a failed episode link update
/// or a failed fragment back-link therefore ends THIS character's remaining
/// linking, and `fragments_linked` counts only back-links whose write returned
/// (v4 `:198` increments after the awaited update). The counts already made
/// stay made (`memoriesWritten` is incremented before linking, as v4 does).
///
/// `update_for_character` answering `Ok(false)` (not found / not owned) is NOT
/// an error: v4's `updateForCharacter` answers `null` there without throwing
/// and the flow continues — and still counts the back-link.
async fn write_episode_for_character<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    input: &RunFoldEpisodePassInput,
    write: &EpisodeWrite<'_>,
    character_id: &str,
    result: &mut FoldEpisodePassResult,
) -> Result<(), DbError> {
    let episode = write.episode;
    let outcome = create_memory_with_gate(
        db,
        embedding,
        &CreateMemoryOptions {
            character_id: character_id.to_string(),
            content: episode.narrative.clone(),
            summary: episode.summary.clone(),
            keywords: episode
                .entities
                .iter()
                .map(|e| e.to_lowercase())
                .chain([
                    "past".to_string(),
                    "scope: narrow".to_string(),
                    "history".to_string(),
                ])
                .collect(),
            importance: Some(episode.importance),
            chat_id: Some(input.chat_id.clone()),
            project_id: input.project_id.clone(),
            source: Some("AUTO".to_string()),
            source_message_id: write.source_message_id.clone(),
            source_message_timestamp: Some(write.occurred_at.to_string()),
            witnessed_context: Some(
                if input.in_autonomous_room {
                    "autonomous_room"
                } else {
                    "user_present"
                }
                .to_string(),
            ),
            occurred_at: Some(write.occurred_at.to_string()),
            narrative_time: write.narrative_time.clone(),
            entities: episode.entities.clone(),
            kind: Some("episodic".to_string()),
            tags: Vec::new(),
            about_character_id: None,
        },
        &MemoryServiceOptions {
            user_id: input.user_id.clone(),
            embedding_profile_id: None,
        },
    )
    .await?;
    // v4 `if (!memory) continue` — only SKIP_EMBEDDING_FAILED is null.
    let Some(memory_id) = outcome.memory_id.clone() else {
        return Ok(());
    };
    // v4 `:157` counts `INSERT | INSERT_RELATED | SKIP_GATE`. Its `SKIP_GATE` arm
    // is DEAD CODE: `createMemoryWithGate` answers `SKIP_GATE` only under
    // `options.skipGate || options.skipEmbedding` (`memory-service.ts:396-398`),
    // and this pass passes `{ userId }` — unchanged since the pass was written
    // (`8bf3cb5f3`). The two counts agree on every reachable input; do NOT add a
    // `GateAction::SkipGate` to "fix" it (P4.144 survey §A4).
    if matches!(
        outcome.action,
        GateAction::Insert | GateAction::InsertRelated
    ) {
        result.memories_written += 1;
    }

    // Link the character's per-turn fragment memories from the same window so
    // one-hop expansion can pull them when the episode surfaces. v4's
    // `findByCharacterAndSourceMessageIds` is a FALLBACK read (`[]` on error),
    // which then takes the `fragmentIds.length === 0` arm. A pool checkout
    // failure answers `[]` too (v4's `getCollection()` runs inside the same
    // fallback `safeQuery`), never this character's catch.
    let cid = character_id.to_string();
    let ids = write.window_message_ids.to_vec();
    let fragments = db
        .read_main(move |conn| {
            Ok(memories_read::find_by_character_and_source_message_ids_or_empty(conn, &cid, &ids))
        })
        .unwrap_or_default();
    let fragment_ids: Vec<String> = fragments
        .iter()
        .filter_map(|f| f.get("id").and_then(Value::as_str).map(str::to_string))
        .filter(|id| id != &memory_id)
        .take(MAX_FRAGMENT_LINKS)
        .collect();
    if fragment_ids.is_empty() {
        return Ok(());
    }

    // v4 reads `outcome.memory.relatedMemoryIds` — the object as the gate
    // RETURNED it. v4 Bug 26 (`62ab1bc8`) fixed the INSERT_RELATED arm to return
    // the POST-LINK row, so that object now carries the gate's links; folding
    // them in preserves them instead of clobbering to `[]`. A plain INSERT still
    // carries `[]` (v4's object does), and everything else re-reads the row.
    // Split the arms explicitly — collapsing INSERT_RELATED into a row re-read
    // would be correct-by-accident and a divergence from v4's post-fix code.
    let mut episode_links: Vec<String> = match outcome.action {
        GateAction::InsertRelated => outcome.related_memory_ids.clone(),
        GateAction::Insert => Vec::new(),
        _ => {
            let mid = memory_id.clone();
            db.read_main(move |conn| memories_read::find_by_id(conn, &mid))
                .ok()
                .flatten()
                .as_ref()
                .map(related_ids)
                .unwrap_or_default()
        }
    };
    let mut episode_links_changed = false;
    for fragment_id in &fragment_ids {
        if !episode_links.contains(fragment_id) {
            episode_links.push(fragment_id.clone());
            episode_links_changed = true;
        }
    }
    if episode_links_changed {
        let cid = character_id.to_string();
        let mid = memory_id.clone();
        let patch = MemUpdate {
            related_memory_ids: Some(episode_links),
            ..Default::default()
        };
        db.write(move |w| w.main().memories().update_for_character(&cid, &mid, &patch))
            .await?;
    }
    for fragment in &fragments {
        let Some(fragment_id) = fragment.get("id").and_then(Value::as_str) else {
            continue;
        };
        if fragment_id == memory_id {
            continue;
        }
        if !fragment_ids.iter().any(|id| id == fragment_id) {
            continue;
        }
        let mut links = related_ids(fragment);
        if links.iter().any(|id| id == &memory_id) {
            continue;
        }
        links.push(memory_id.clone());
        let cid = character_id.to_string();
        let fid = fragment_id.to_string();
        let patch = MemUpdate {
            related_memory_ids: Some(links),
            ..Default::default()
        };
        db.write(move |w| w.main().memories().update_for_character(&cid, &fid, &patch))
            .await?;
        result.fragments_linked += 1;
    }
    Ok(())
}
