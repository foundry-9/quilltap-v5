//! First-message context (Phase-3 Unit-3 wave 3) — v4
//! `loadParticipantMemories` / `loadProjectContext` / `buildFirstMessageContext`
//! (`lib/chat/first-message-context.ts`).
//!
//! Builds the enhanced context for an auto-generated first message: the active
//! project's context (name / description / instructions) and the speaking
//! character's memories about the *other* participants. Read-only.
//!
//! The participant-memory strategy per participant is **Recent + Semantic +
//! text-fallback**, deduped by memory id, then importance-sorted and capped:
//!
//!   1. up to 3 recent inter-character memories
//!      (`findByCharacterAboutCharacter`, importance-then-recency order);
//!   2. semantic search on the participant's name (+ description) — limit 8,
//!      minScore 0.4 — folding in memories about the participant OR general
//!      (`aboutCharacterId` null/absent) memories;
//!   3. **only when semantic returned nothing AND we're still short**, a text
//!      `searchByContent` on just the name;
//!   4. sort the collected map by importance desc, take `memoriesPerParticipant`
//!      (default 5).
//!
//! Every per-participant load is wrapped so one participant's failure never aborts
//! the others (v4 logs + continues); the semantic / text sub-steps swallow their own
//! errors the same way.
//!
//! Composes the ported [`crate::services::memory_service::search_memories_semantic`]
//! + the memories reads + the [`EmbeddingProvider`] seam.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{characters_read, memories_read, DbError};
use crate::model::embedding::EmbeddingProvider;

use super::memory_service::{search_memories_semantic, SemanticSearchOptions};

/// Info about one participant (v4 `ParticipantInfo`).
#[derive(Debug, Clone)]
pub struct ParticipantInfo {
    pub character_id: String,
    pub name: String,
    pub description: Option<String>,
    /// `'llm' | 'user'`.
    pub controlled_by: String,
}

/// The chat-participant input subset `buildFirstMessageContext` consumes (v4
/// `ChatParticipantBaseInput`).
#[derive(Debug, Clone)]
pub struct ChatParticipantInput {
    /// `'CHARACTER' | 'USER'` (only CHARACTER participants with a distinct id are
    /// loaded).
    pub participant_type: String,
    pub character_id: Option<String>,
    /// `'llm' | 'user'` — defaults to `'llm'` when absent.
    pub controlled_by: Option<String>,
}

/// One collected participant memory (v4 `ParticipantMemory`).
#[derive(Debug, Clone, PartialEq)]
pub struct ParticipantMemory {
    pub about_character_id: String,
    pub about_character_name: String,
    pub summary: String,
    pub importance: f64,
}

/// Project context for system-prompt injection (v4 `ProjectContext`).
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectContext {
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
}

/// The full result (v4 `FirstMessageContextResult`).
#[derive(Debug, Clone, PartialEq)]
pub struct FirstMessageContextResult {
    pub project_context: Option<ProjectContext>,
    pub participant_memories: Vec<ParticipantMemory>,
}

/// Options carried into `build_first_message_context`. `now_ms` is the injected
/// `Date.now()` seam (see [`SemanticSearchOptions::now_ms`]).
#[derive(Debug, Clone, Default)]
pub struct FirstMessageContextOptions {
    pub user_id: String,
    pub project_id: Option<String>,
    pub embedding_profile_id: Option<String>,
    pub now_ms: f64,
}

const DEFAULT_MEMORIES_PER_PARTICIPANT: usize = 5;
const SEMANTIC_LIMIT: usize = 8;
const SEMANTIC_MIN_SCORE: f64 = 0.4;

/// v4 `loadParticipantMemories`. Loads the speaking character's memories about each
/// other participant; a per-participant failure is logged-and-continued (here:
/// swallowed).
#[allow(clippy::too_many_arguments)]
pub async fn load_participant_memories<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    speaking_character_id: &str,
    other_participants: &[ParticipantInfo],
    user_id: &str,
    embedding_profile_id: Option<&str>,
    memories_per_participant: usize,
    now_ms: f64,
) -> Result<Vec<ParticipantMemory>, DbError> {
    if other_participants.is_empty() {
        return Ok(Vec::new());
    }

    let mut all: Vec<ParticipantMemory> = Vec::new();
    for participant in other_participants {
        // v4 try/catch: continue with the other participants on failure.
        match load_for_participant(
            db,
            provider,
            speaking_character_id,
            participant,
            user_id,
            embedding_profile_id,
            memories_per_participant,
            now_ms,
        )
        .await
        {
            Ok(mut mems) => all.append(&mut mems),
            Err(_) => { /* logged + continue in v4 */ }
        }
    }
    Ok(all)
}

/// v4 `loadMemoriesForParticipant` — the Recent + Semantic + text-fallback strategy
/// for one participant.
#[allow(clippy::too_many_arguments)]
async fn load_for_participant<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    speaking_character_id: &str,
    participant: &ParticipantInfo,
    user_id: &str,
    embedding_profile_id: Option<&str>,
    limit: usize,
    now_ms: f64,
) -> Result<Vec<ParticipantMemory>, DbError> {
    // The dedup map preserves insertion order (v4's `Map`); we mirror that with a
    // Vec of (id, ParticipantMemory) + a membership check.
    let mut memory_map: Vec<(String, ParticipantMemory)> = Vec::new();

    // 1. Recent inter-character memories (up to 3).
    let recent = {
        let cid = speaking_character_id.to_string();
        let about = participant.character_id.clone();
        db.read_main(move |conn| {
            memories_read::find_by_character_about_character(conn, &cid, &about)
        })?
    };
    let recent_count = 3.min(recent.len());
    for memory in recent.iter().take(recent_count) {
        let Some(id) = mem_id(memory) else { continue };
        if !map_has(&memory_map, &id) {
            memory_map.push((id, participant_memory(participant, memory)));
        }
    }

    // 2. Semantic search by participant name (+ description).
    let search_query = match &participant.description {
        Some(d) if !d.is_empty() => format!("{} - {}", participant.name, d),
        _ => participant.name.clone(),
    };
    let mut semantic_worked = false;
    // v4 wraps the semantic call in try/catch; `search_memories_semantic` already
    // fails soft internally (falling back to text), but a hard DbError here is
    // swallowed to match v4's `catch`.
    let semantic = search_memories_semantic(
        db,
        provider,
        speaking_character_id,
        &search_query,
        &SemanticSearchOptions {
            user_id: user_id.to_string(),
            embedding_profile_id: embedding_profile_id.map(str::to_string),
            limit: Some(SEMANTIC_LIMIT),
            min_score: Some(SEMANTIC_MIN_SCORE),
            now_ms,
            ..Default::default()
        },
        None,
    )
    .await;
    if let Ok(results) = semantic {
        for r in &results {
            let about = r.memory.get("aboutCharacterId");
            let is_about_participant =
                about.and_then(Value::as_str) == Some(participant.character_id.as_str());
            // aboutCharacterId null OR absent → general memory.
            let is_general = about.map(Value::is_null).unwrap_or(true);
            let Some(id) = mem_id(&r.memory) else {
                continue;
            };
            if (is_about_participant || is_general) && !map_has(&memory_map, &id) {
                memory_map.push((id, participant_memory(participant, &r.memory)));
            }
        }
        semantic_worked = !results.is_empty();
    }

    // 3. Text fallback — only when still short AND semantic found nothing.
    if memory_map.len() < limit && !semantic_worked {
        let text = {
            let cid = speaking_character_id.to_string();
            let name = participant.name.clone();
            db.read_main(move |conn| memories_read::search_by_content(conn, &cid, &name))
        };
        if let Ok(text_results) = text {
            for memory in &text_results {
                let Some(id) = mem_id(memory) else { continue };
                if !map_has(&memory_map, &id) {
                    memory_map.push((id, participant_memory(participant, memory)));
                }
                if memory_map.len() >= limit {
                    break;
                }
            }
        }
    }

    // 4. Sort by importance desc, take top N. v4's `.sort((a,b) => b.importance -
    // a.importance)` is stable — keep insertion order among ties.
    let mut memories: Vec<ParticipantMemory> = memory_map.into_iter().map(|(_, m)| m).collect();
    memories.sort_by(|a, b| {
        b.importance
            .partial_cmp(&a.importance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    memories.truncate(limit);
    Ok(memories)
}

/// v4 `loadProjectContext`. Returns `None` for a missing project or on failure
/// (both branches logged in v4).
pub fn load_project_context(db: &Db, project_id: &str) -> Result<Option<ProjectContext>, DbError> {
    let pid = project_id.to_string();
    let project = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let repo = crate::db::projects::ProjectsRepository::new(main, mount);
            // find_by_id throws Unavailable when the store is missing — v4's catch
            // logs + returns null, so map any error to None.
            Ok(repo.find_by_id(&pid).ok().flatten())
        })
    });
    match project {
        Ok(Some(p)) => Ok(Some(ProjectContext {
            name: p
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            description: p
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            instructions: p
                .get("instructions")
                .and_then(Value::as_str)
                .map(str::to_string),
        })),
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}

/// v4 `buildFirstMessageContext`. Builds the other-participant info list (excluding
/// the speaker), loads the optional project context, then the participant memories.
pub async fn build_first_message_context<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    speaking_character_id: &str,
    participants: &[ChatParticipantInput],
    options: &FirstMessageContextOptions,
) -> Result<FirstMessageContextResult, DbError> {
    // Build the other-participant info list.
    let mut other_participants: Vec<ParticipantInfo> = Vec::new();
    for participant in participants {
        if participant.participant_type != "CHARACTER" {
            continue;
        }
        let Some(cid) = &participant.character_id else {
            continue;
        };
        if cid == speaking_character_id {
            continue;
        }
        // Load the character (vault-overlaid) for name + description.
        let character = {
            let id = cid.clone();
            db.read_main(|main| {
                db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &id))
            })?
        };
        if let Some(ch) = character {
            other_participants.push(ParticipantInfo {
                character_id: cid.clone(),
                name: ch
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                description: ch
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                controlled_by: participant
                    .controlled_by
                    .clone()
                    .unwrap_or_else(|| "llm".to_string()),
            });
        }
    }

    // Load project context if applicable.
    let project_context = match &options.project_id {
        Some(pid) => load_project_context(db, pid)?,
        None => None,
    };

    // Load participant memories.
    let participant_memories = load_participant_memories(
        db,
        provider,
        speaking_character_id,
        &other_participants,
        &options.user_id,
        options.embedding_profile_id.as_deref(),
        DEFAULT_MEMORIES_PER_PARTICIPANT,
        options.now_ms,
    )
    .await?;

    Ok(FirstMessageContextResult {
        project_context,
        participant_memories,
    })
}

fn map_has(map: &[(String, ParticipantMemory)], id: &str) -> bool {
    map.iter().any(|(k, _)| k == id)
}

fn participant_memory(participant: &ParticipantInfo, memory: &Value) -> ParticipantMemory {
    ParticipantMemory {
        about_character_id: participant.character_id.clone(),
        about_character_name: participant.name.clone(),
        summary: mem_summary(memory),
        importance: mem_importance(memory),
    }
}

fn mem_id(m: &Value) -> Option<String> {
    m.get("id").and_then(Value::as_str).map(str::to_string)
}
fn mem_summary(m: &Value) -> String {
    m.get("summary")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
fn mem_importance(m: &Value) -> f64 {
    m.get("importance").and_then(Value::as_f64).unwrap_or(0.0)
}
