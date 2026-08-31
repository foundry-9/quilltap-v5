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

use serde_json::Value;

use crate::chat_predicates::{is_participant_present, ParticipantStatus};
use crate::cheap_llm::CheapLlmSelection;
use crate::db::memories::MemUpdate;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, memories_read};
use crate::episodic::resolve_when_phrase;
use crate::memory_tasks::{
    build_fold_episode_messages, parse_fold_episodes, ExtractionClock, FoldEpisodeMessage,
};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::cheap_llm_exec::CheapLlmTaskOptions;
use crate::services::memory_gate::{
    create_memory_with_gate, CreateMemoryOptions, GateAction, MemoryServiceOptions,
};

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

fn parse_status(s: Option<&str>) -> ParticipantStatus {
    match s.unwrap_or("active") {
        "active" => ParticipantStatus::Active,
        "silent" => ParticipantStatus::Silent,
        "removed" => ParticipantStatus::Removed,
        _ => ParticipantStatus::Absent,
    }
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

    let chat_id = input.chat_id.clone();
    let Ok(Some(chat)) = db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id)) else {
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
                && is_participant_present(parse_status(str_field(p, "status")))
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

    // Resolve speaker names per participant (raw read — survives a broken vault).
    let mut speaker_names: Vec<(String, String)> = Vec::new();
    for p in &participants {
        let Some(character_id) = str_field(p, "characterId").filter(|c| !c.is_empty()) else {
            continue;
        };
        let participant_id = str_field(p, "id").unwrap_or_default().to_string();
        if speaker_names.iter().any(|(id, _)| id == &participant_id) {
            continue;
        }
        // v4 wraps the lookup in a try/catch — the name stays role-labelled.
        let cid = character_id.to_string();
        if let Ok(Some(character)) =
            db.read_main(move |conn| characters_read::find_by_id_raw(conn, &cid))
        {
            if let Some(name) = str_field(&character, "name") {
                speaker_names.push((participant_id, name.to_string()));
            }
        }
    }

    let rendered: Vec<FoldEpisodeMessage> = input
        .window_messages
        .iter()
        .map(|m| FoldEpisodeMessage {
            speaker: m
                .participant_id
                .as_deref()
                .and_then(|pid| {
                    speaker_names
                        .iter()
                        .find(|(id, _)| id == pid)
                        .map(|(_, n)| n.clone())
                })
                .unwrap_or_else(|| {
                    if m.role == "USER" {
                        "User"
                    } else {
                        "Character"
                    }
                    .to_string()
                }),
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
    let episodes = match (extraction.success, extraction.result) {
        (true, Some(episodes)) if !episodes.is_empty() => episodes,
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

        for participant in &present_participants {
            let character_id = str_field(participant, "characterId")
                .unwrap_or_default()
                .to_string();
            // v4 wraps each character's write in its own try/catch.
            let outcome = create_memory_with_gate(
                db,
                embedding,
                &CreateMemoryOptions {
                    character_id: character_id.clone(),
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
                    source_message_id: source_message_id.clone(),
                    source_message_timestamp: Some(occurred_at.clone()),
                    witnessed_context: Some(
                        if input.in_autonomous_room {
                            "autonomous_room"
                        } else {
                            "user_present"
                        }
                        .to_string(),
                    ),
                    occurred_at: Some(occurred_at.clone()),
                    narrative_time: narrative_time.clone(),
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
            .await;
            let Ok(outcome) = outcome else { continue };
            // v4 `if (!memory) continue` — only SKIP_EMBEDDING_FAILED is null.
            let Some(memory_id) = outcome.memory_id.clone() else {
                continue;
            };
            if matches!(
                outcome.action,
                GateAction::Insert | GateAction::InsertRelated
            ) {
                result.memories_written += 1;
            }

            // Link the character's per-turn fragment memories from the same
            // window so one-hop expansion can pull them when the episode
            // surfaces.
            let cid = character_id.clone();
            let ids = window_message_ids.clone();
            let Ok(fragments) = db.read_main(move |conn| {
                memories_read::find_by_character_and_source_message_ids(conn, &cid, &ids)
            }) else {
                continue;
            };
            let fragment_ids: Vec<String> = fragments
                .iter()
                .filter_map(|f| f.get("id").and_then(Value::as_str).map(str::to_string))
                .filter(|id| id != &memory_id)
                .take(MAX_FRAGMENT_LINKS)
                .collect();
            if fragment_ids.is_empty() {
                continue;
            }

            // v4 reads `outcome.memory.relatedMemoryIds` — the object as the gate
            // RETURNED it. v4 Bug 26 (`62ab1bc8`) fixed the INSERT_RELATED arm to
            // return the POST-LINK row, so that object now carries the gate's
            // links; folding them in preserves them instead of clobbering to `[]`.
            // A plain INSERT still carries `[]` (v4's object does), and everything
            // else re-reads the row. Split the arms explicitly — collapsing
            // INSERT_RELATED into a row re-read would be correct-by-accident and a
            // divergence from v4's post-fix code.
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
                let cid = character_id.clone();
                let mid = memory_id.clone();
                let patch = MemUpdate {
                    related_memory_ids: Some(episode_links),
                    ..Default::default()
                };
                let _ = db
                    .write(move |w| w.main().memories().update_for_character(&cid, &mid, &patch))
                    .await;
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
                let cid = character_id.clone();
                let fid = fragment_id.to_string();
                let patch = MemUpdate {
                    related_memory_ids: Some(links),
                    ..Default::default()
                };
                let _ = db
                    .write(move |w| w.main().memories().update_for_character(&cid, &fid, &patch))
                    .await;
                result.fragments_linked += 1;
            }
        }
    }

    result
}
