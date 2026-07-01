//! The **memory gate** service — v4's `createMemoryWithGate` / `runMemoryGate`
//! (`lib/memory/memory-service.ts` + `lib/memory/memory-gate.ts`). The pre-write
//! similarity check that turned the memory system from append-only into
//! append-or-reinforce.
//!
//! Given a candidate memory it (1) generates an embedding for the candidate text
//! (the model call — the tier-3 seam, with v4's one-retry-on-failure), (2) queries
//! the character's vector store for the top-`GATE_TOP_K` nearest existing memories,
//! and (3) makes a five-outcome decision by similarity band:
//!
//! | Outcome | Band | Effect |
//! |---|---|---|
//! | `SkipNearDuplicate`   | `>= NEAR_DUPLICATE_THRESHOLD` | absorbed; no new row |
//! | `Reinforce`           | `>= MERGE_THRESHOLD`          | boost the existing memory |
//! | `InsertRelated`       | `>= RELATED_THRESHOLD`        | insert + link the related memories |
//! | `Insert`              | below `RELATED_THRESHOLD`     | fresh memory |
//! | `SkipEmbeddingFailed` | — (embedding unavailable)     | skip, no row |
//!
//! ⚠️ The v4 file's header comment ("REINFORCE >= 0.80 / INSERT_RELATED 0.70–0.80")
//! is **stale** — the authoritative exported constants are the ones below
//! (`0.90` / `0.85` / `0.70`). Same trap as the SQLCipher-vs-ChaCha20 comment:
//! comments lie, the constants are truth. The differential proves the bands.
//!
//! ## Shape under the Phase-3 runtime
//!
//! The service is `async` and generic over an [`EmbeddingProvider`]: it awaits the
//! model call, reads the vector store + matched memories off the read pool
//! ([`Db::read_main`]), and funnels every mutation through the writer thread
//! ([`Db::write`]) — so the "channel is the only mutator" invariant holds and this
//! is the first service to exercise the whole Unit-0 write path end to end. The
//! vector store is the in-memory [`CharacterVectorStore`] shim (load off the read
//! pool, flush on the writer). This validates Units 0 + 0.5 + tier-2 together.
//!
//! ## Deferred (tracked, out of scope for this unit)
//!
//!   - `maybeEnqueueHousekeeping` — the fire-and-forget watermark check v4 `void`s
//!     after an INSERT / INSERT_RELATED. Never awaited, no bearing on the gate's own
//!     DB effect; lands with the housekeeping job unit.
//!   - the `skipGate` / `skipEmbedding` → `createMemoryDirect` path — the
//!     force-insert-without-gate flow (no similarity check). This port always runs
//!     the gate; the direct path lands with the extraction driver.
//!   - `applyNamePresenceCheck`'s cross-character resolution — the AUTO
//!     mis-attribution safety net looks up the about/holder characters through the
//!     `characters` **vault overlay** (aliases live in `properties.json`). The
//!     no-lookup branches (null proposal / self-reference / non-AUTO source) are
//!     ported; the lookup-and-resolve branch is deferred and the corpus keeps
//!     `aboutCharacterId == null`, so it is a verified no-op here.
//!   - the 500 ms inter-retry delay (`EMBEDDING_RETRY_DELAY_MS`) — a host-timing
//!     concern with no DB-state effect; reproducing it would pull a timer into the
//!     scheduler-free core, so the retry is issued without the sleep.

use std::collections::HashMap;

use serde_json::Value;

use crate::db::memories::{CreateOptions, MemCreate, MemUpdate};
use crate::db::vector_store::CharacterVectorStore;
use crate::db::{memories_read, DbError};
use crate::memory_gate::{calculate_reinforced_importance, extract_novel_details};
use crate::model::embedding::{EmbeddingPriority, EmbeddingProvider};

use crate::db::runtime::Db;

/// At or above this cosine similarity the candidate is an exact-or-near-exact
/// restatement and is skipped entirely (not even reinforced).
pub const NEAR_DUPLICATE_THRESHOLD: f64 = 0.90;
/// At or above this the best match is reinforced instead of duplicated.
pub const MERGE_THRESHOLD: f64 = 0.85;
/// Related-but-distinct band floor — matches here get bidirectionally linked.
pub const RELATED_THRESHOLD: f64 = 0.70;
/// Top-K nearest existing memories fetched from the vector store during the gate.
pub const GATE_TOP_K: usize = 5;

/// The action the gate took (v4 `MemoryGateOutcome.action`, minus `SKIP_GATE`
/// which is the deferred direct path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateAction {
    Insert,
    InsertRelated,
    Reinforce,
    SkipNearDuplicate,
    SkipEmbeddingFailed,
}

/// The gate's result (v4 `MemoryGateOutcome`): the affected memory + the action,
/// plus the per-outcome extras callers (e.g. the extraction path) consume.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryGateOutcome {
    /// The memory affected by this outcome. For `SkipNearDuplicate` it is the
    /// existing memory that absorbed the observation; `None` only for
    /// `SkipEmbeddingFailed`.
    pub memory_id: Option<String>,
    pub action: GateAction,
    /// Novel details appended on `Reinforce` (empty otherwise).
    pub novel_details: Vec<String>,
    /// The ids linked on `InsertRelated` (empty otherwise).
    pub related_memory_ids: Vec<String>,
    /// Cosine similarity to the existing memory (`SkipNearDuplicate` / `Reinforce`).
    pub similarity: Option<f64>,
    /// Human-readable reason (`SkipEmbeddingFailed`).
    pub reason: Option<String>,
}

/// Options for memory creation (v4 `CreateMemoryOptions`), minus the deferred
/// `skipGate` / `skipEmbedding` flags (this port always runs the gate).
#[derive(Debug, Clone, Default)]
pub struct CreateMemoryOptions {
    pub character_id: String,
    pub content: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub tags: Vec<String>,
    pub importance: Option<f64>,
    pub about_character_id: Option<String>,
    pub chat_id: Option<String>,
    pub project_id: Option<String>,
    /// `"AUTO"` / `"MANUAL"` (v4 default `"MANUAL"`).
    pub source: Option<String>,
    pub source_message_id: Option<String>,
    /// Override createdAt/updatedAt with the source-message timestamp (batch
    /// extraction). When present, `create` pins both timestamps to it.
    pub source_message_timestamp: Option<String>,
    pub witnessed_context: Option<String>,
}

/// Options for the operation (v4 `MemoryServiceOptions`), minus the deferred skip
/// flags.
#[derive(Debug, Clone, Default)]
pub struct MemoryServiceOptions {
    pub user_id: String,
    pub embedding_profile_id: Option<String>,
}

/// The internal gate decision (v4 `GateDecision`), carrying the matched memories as
/// their net JSON (the shape [`memories_read::find_by_ids`] returns).
enum GateDecision {
    Insert,
    Reinforce { existing: Value },
    InsertRelated { related: Vec<RelatedMatch> },
    SkipNearDuplicate { existing: Value, similarity: f64 },
    SkipEmbeddingFailed { reason: String },
}

struct RelatedMatch {
    memory: Value,
    #[allow(dead_code)]
    similarity: f64,
}

struct GateResult {
    decision: GateDecision,
    embedding: Option<Vec<f32>>,
}

/// Create a memory through the gate (v4 `createMemoryWithGate`). Runs the
/// name-presence check, the gate, and the per-outcome writes, returning the full
/// outcome.
pub async fn create_memory_with_gate<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    data: &CreateMemoryOptions,
    opts: &MemoryServiceOptions,
) -> Result<MemoryGateOutcome, DbError> {
    let data = apply_name_presence_check(data);

    let gate = run_memory_gate(
        db,
        provider,
        &data.character_id,
        &data.content,
        &data.summary,
        &opts.user_id,
        opts.embedding_profile_id.as_deref(),
    )
    .await?;

    let GateResult {
        decision,
        embedding,
    } = gate;

    match decision {
        GateDecision::SkipNearDuplicate {
            existing,
            similarity,
        } => Ok(MemoryGateOutcome {
            memory_id: id_of(&existing),
            action: GateAction::SkipNearDuplicate,
            novel_details: Vec::new(),
            related_memory_ids: Vec::new(),
            similarity: Some(similarity),
            reason: None,
        }),

        GateDecision::SkipEmbeddingFailed { reason } => Ok(MemoryGateOutcome {
            memory_id: None,
            action: GateAction::SkipEmbeddingFailed,
            novel_details: Vec::new(),
            related_memory_ids: Vec::new(),
            similarity: None,
            reason: Some(reason),
        }),

        GateDecision::Reinforce { existing } => {
            let (memory_id, novel_details) = reinforce_memory(
                db,
                provider,
                &existing,
                &data.content,
                &opts.user_id,
                opts.embedding_profile_id.as_deref(),
            )
            .await?;
            Ok(MemoryGateOutcome {
                memory_id: Some(memory_id),
                action: GateAction::Reinforce,
                novel_details,
                related_memory_ids: Vec::new(),
                similarity: None,
                reason: None,
            })
        }

        GateDecision::InsertRelated { related } => {
            let new_id = create_memory_direct_with_embedding(db, &data, embedding).await?;
            let linked = link_related_memories(db, &new_id, &data.character_id, &related).await?;
            // Deferred: `void maybeEnqueueHousekeeping(...)` — fire-and-forget.
            Ok(MemoryGateOutcome {
                memory_id: Some(new_id),
                action: GateAction::InsertRelated,
                novel_details: Vec::new(),
                related_memory_ids: linked,
                similarity: None,
                reason: None,
            })
        }

        GateDecision::Insert => {
            let new_id = create_memory_direct_with_embedding(db, &data, embedding).await?;
            // Deferred: `void maybeEnqueueHousekeeping(...)`.
            Ok(MemoryGateOutcome {
                memory_id: Some(new_id),
                action: GateAction::Insert,
                novel_details: Vec::new(),
                related_memory_ids: Vec::new(),
                similarity: None,
                reason: None,
            })
        }
    }
}

/// Run the gate (v4 `runMemoryGate`): embed the candidate (one retry), search the
/// vector store, and decide the band.
async fn run_memory_gate<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    character_id: &str,
    content: &str,
    summary: &str,
    user_id: &str,
    embedding_profile_id: Option<&str>,
) -> Result<GateResult, DbError> {
    let embedding_text = format!("{summary}\n\n{content}");

    let embedding =
        match generate_with_retry(provider, &embedding_text, user_id, embedding_profile_id).await {
            Ok(e) => e,
            Err(reason) => {
                return Ok(GateResult {
                    decision: GateDecision::SkipEmbeddingFailed { reason },
                    embedding: None,
                });
            }
        };

    // Load the store + search off the read pool.
    let results = {
        let embedding = &embedding;
        db.read_main(move |conn| {
            let store = CharacterVectorStore::load(conn, character_id)?;
            Ok(store.search(embedding, GATE_TOP_K))
        })?
    };

    if results.is_empty() {
        return Ok(GateResult {
            decision: GateDecision::Insert,
            embedding: Some(embedding),
        });
    }

    // Fetch full memory data for the matched ids (only the top-K).
    let matched_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    let matched = db.read_main(move |conn| memories_read::find_by_ids(conn, &matched_ids))?;
    let memory_map: HashMap<String, Value> = matched
        .into_iter()
        .filter_map(|m| id_of(&m).map(|id| (id, m)))
        .collect();

    let best = &results[0];
    let best_mem = memory_map.get(&best.id).cloned();

    if best.score >= NEAR_DUPLICATE_THRESHOLD {
        if let Some(existing) = best_mem {
            return Ok(GateResult {
                decision: GateDecision::SkipNearDuplicate {
                    existing,
                    similarity: best.score,
                },
                embedding: Some(embedding),
            });
        }
    }

    if best.score >= MERGE_THRESHOLD {
        if let Some(existing) = best_mem {
            return Ok(GateResult {
                decision: GateDecision::Reinforce { existing },
                embedding: Some(embedding),
            });
        }
    }

    // Related band: [RELATED_THRESHOLD, MERGE_THRESHOLD), in score-desc order.
    let related: Vec<RelatedMatch> = results
        .iter()
        .filter(|r| r.score >= RELATED_THRESHOLD && r.score < MERGE_THRESHOLD)
        .filter_map(|r| {
            memory_map.get(&r.id).cloned().map(|memory| RelatedMatch {
                memory,
                similarity: r.score,
            })
        })
        .collect();

    if !related.is_empty() {
        return Ok(GateResult {
            decision: GateDecision::InsertRelated { related },
            embedding: Some(embedding),
        });
    }

    Ok(GateResult {
        decision: GateDecision::Insert,
        embedding: Some(embedding),
    })
}

/// Generate an embedding with v4's one-retry-on-failure (`SKIP_EMBEDDING_FAILED`
/// reason on the second failure). The 500 ms inter-retry delay is omitted (see the
/// module deferrals).
async fn generate_with_retry<P: EmbeddingProvider>(
    provider: &P,
    text: &str,
    user_id: &str,
    embedding_profile_id: Option<&str>,
) -> Result<Vec<f32>, String> {
    match provider
        .generate_embedding_for_user(
            text,
            user_id,
            embedding_profile_id,
            EmbeddingPriority::Background,
        )
        .await
    {
        Ok(r) => Ok(r.embedding),
        Err(_first) => match provider
            .generate_embedding_for_user(
                text,
                user_id,
                embedding_profile_id,
                EmbeddingPriority::Background,
            )
            .await
        {
            Ok(r) => Ok(r.embedding),
            Err(second) => Err(format!("Embedding failed after retry: {}", second.message)),
        },
    }
}

/// Create a memory and store its pre-computed embedding (v4
/// `createMemoryDirectWithEmbedding`). Mints id + timestamps (or pins them to the
/// source-message timestamp), writes the row, then — when an embedding is present —
/// sets the BLOB and adds the vector to the store, all on the writer thread.
/// Returns the new memory id.
async fn create_memory_direct_with_embedding(
    db: &Db,
    data: &CreateMemoryOptions,
    embedding: Option<Vec<f32>>,
) -> Result<String, DbError> {
    let importance = data.importance.unwrap_or(0.5);
    let id = uuid::Uuid::new_v4().to_string();
    let (created_at, updated_at) = match &data.source_message_timestamp {
        Some(ts) => (ts.clone(), ts.clone()),
        None => {
            let now = crate::clock::now_iso();
            (now.clone(), now)
        }
    };

    let create = MemCreate {
        character_id: data.character_id.clone(),
        about_character_id: data.about_character_id.clone(),
        chat_id: data.chat_id.clone(),
        project_id: data.project_id.clone(),
        content: data.content.clone(),
        summary: data.summary.clone(),
        keywords: data.keywords.clone(),
        tags: data.tags.clone(),
        importance,
        // v4 creates the row WITHOUT an embedding, then updates it below.
        embedding: None,
        source: data.source.clone().unwrap_or_else(|| "MANUAL".to_string()),
        witnessed_context: data.witnessed_context.clone(),
        source_message_id: data.source_message_id.clone(),
        last_accessed_at: None,
        reinforcement_count: 1.0,
        last_reinforced_at: None,
        related_memory_ids: Vec::new(),
        reinforced_importance: importance,
    };
    let create_opts = CreateOptions {
        id: id.clone(),
        created_at,
        updated_at,
    };

    let character_id = data.character_id.clone();
    let id_for_write = id.clone();
    db.write(move |writers| {
        let main = writers.main();
        main.memories().create(&create, &create_opts)?;

        if let Some(emb) = embedding {
            // Store the pre-computed embedding on the row (v4's
            // `updateForCharacter(id, { embedding })`).
            let patch = MemUpdate {
                embedding: Some(Some(emb.clone())),
                ..Default::default()
            };
            main.memories()
                .update_for_character(&character_id, &id_for_write, &patch)?;

            // Add to the vector store + persist.
            let mut store = CharacterVectorStore::load(main.connection(), &character_id)?;
            store.add_vector(&id_for_write, emb)?;
            store.flush(&main.vector_indices())?;
        }
        Ok(())
    })
    .await?;

    Ok(id)
}

/// Reinforce an existing memory (v4 `reinforceMemory`): extract novel details,
/// append them as footnotes when any, bump `reinforcementCount` /
/// `lastReinforcedAt` / `reinforcedImportance`, and re-embed if the content
/// changed. Returns `(memory_id, novel_details)`.
async fn reinforce_memory<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    existing: &Value,
    candidate_content: &str,
    user_id: &str,
    embedding_profile_id: Option<&str>,
) -> Result<(String, Vec<String>), DbError> {
    let existing_id = id_of(existing).unwrap_or_default();
    let existing_char = str_field(existing, "characterId");
    let existing_content = str_field(existing, "content");
    let existing_summary = str_field(existing, "summary");
    let existing_importance = num_field(existing, "importance").unwrap_or(0.5);
    // v4: `existingMemory.reinforcementCount ?? 1`.
    let existing_count = num_field(existing, "reinforcementCount").unwrap_or(1.0);

    let novel_details = extract_novel_details(candidate_content, &existing_content);
    let new_count = existing_count + 1.0;
    let now = crate::clock::now_iso();
    let new_reinforced_importance = calculate_reinforced_importance(existing_importance, new_count);

    let new_content = if novel_details.is_empty() {
        existing_content.clone()
    } else {
        let footnotes = novel_details
            .iter()
            .map(|d| format!("[+] {d}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("{existing_content}\n{footnotes}")
    };
    let content_changed = new_content != existing_content;

    // The reinforcement update (bumps updatedAt).
    {
        let patch = MemUpdate {
            reinforcement_count: Some(new_count),
            last_reinforced_at: Some(Some(now.clone())),
            reinforced_importance: Some(new_reinforced_importance),
            content: if content_changed {
                Some(new_content.clone())
            } else {
                None
            },
            ..Default::default()
        };
        let existing_char_w = existing_char.clone();
        let existing_id_w = existing_id.clone();
        let updated = db
            .write(move |writers| {
                writers.main().memories().update_for_character(
                    &existing_char_w,
                    &existing_id_w,
                    &patch,
                )
            })
            .await?;
        // v4: if the update failed, return the existing memory unchanged.
        if !updated {
            return Ok((existing_id, novel_details));
        }
    }

    // Re-embed if content changed (v4 wraps this in a non-fatal try/catch).
    if content_changed {
        let reembed_text = format!("{existing_summary}\n\n{new_content}");
        if let Ok(emb) =
            generate_with_retry(provider, &reembed_text, user_id, embedding_profile_id).await
        {
            let existing_char = existing_char.clone();
            let existing_id = existing_id.clone();
            db.write(move |writers| {
                let main = writers.main();
                let patch = MemUpdate {
                    embedding: Some(Some(emb.clone())),
                    ..Default::default()
                };
                main.memories()
                    .update_for_character(&existing_char, &existing_id, &patch)?;

                let mut store = CharacterVectorStore::load(main.connection(), &existing_char)?;
                if store.has_vector(&existing_id) {
                    store.update_vector(&existing_id, emb)?;
                } else {
                    store.add_vector(&existing_id, emb)?;
                }
                store.flush(&main.vector_indices())?;
                Ok(())
            })
            .await?;
        }
    }

    Ok((existing_id, novel_details))
}

/// Bidirectionally link a new memory with related existing memories (v4
/// `linkRelatedMemories`). Returns the linked ids in order.
async fn link_related_memories(
    db: &Db,
    new_memory_id: &str,
    new_memory_character_id: &str,
    related: &[RelatedMatch],
) -> Result<Vec<String>, DbError> {
    let mut linked: Vec<String> = Vec::new();

    for rm in related {
        let rel_char = str_field(&rm.memory, "characterId");
        let rel_id = id_of(&rm.memory).unwrap_or_default();
        let existing_related = str_array_field(&rm.memory, "relatedMemoryIds");

        if !existing_related.iter().any(|x| x == new_memory_id) {
            let mut updated = existing_related.clone();
            updated.push(new_memory_id.to_string());
            let rel_char = rel_char.clone();
            let rel_id = rel_id.clone();
            db.write(move |writers| {
                let patch = MemUpdate {
                    related_memory_ids: Some(updated),
                    ..Default::default()
                };
                writers
                    .main()
                    .memories()
                    .update_for_character(&rel_char, &rel_id, &patch)
            })
            .await?;
        }
        linked.push(rel_id);
    }

    if !linked.is_empty() {
        let linked_for_write = linked.clone();
        let new_char = new_memory_character_id.to_string();
        let new_id = new_memory_id.to_string();
        db.write(move |writers| {
            let patch = MemUpdate {
                related_memory_ids: Some(linked_for_write),
                ..Default::default()
            };
            writers
                .main()
                .memories()
                .update_for_character(&new_char, &new_id, &patch)
        })
        .await?;
    }

    Ok(linked)
}

/// The AUTO mis-attribution safety net (v4 `applyNamePresenceCheck`), ported for
/// the no-lookup branches only (see the module deferrals): a null / self /
/// non-AUTO proposal passes through unchanged. A cross-character AUTO proposal
/// would need the `characters` vault-overlay read; that branch is deferred, and
/// the corpus keeps `aboutCharacterId == null`, so this is a verified no-op.
fn apply_name_presence_check(data: &CreateMemoryOptions) -> CreateMemoryOptions {
    // Steps 1-3 (no DB read): null proposal / self-reference / non-AUTO source.
    match &data.about_character_id {
        None => return data.clone(),
        Some(proposed) if proposed == &data.character_id => return data.clone(),
        Some(_) => {}
    }
    if let Some(src) = &data.source {
        if src != "AUTO" {
            return data.clone();
        }
    }
    // The cross-character AUTO resolution branch is deferred; pass through
    // unchanged (the corpus never reaches here — aboutCharacterId is null).
    data.clone()
}

// ---------------------------------------------------------------------------
// Small accessors over the net Memory JSON (`memories_read::find_by_ids` shape).
// ---------------------------------------------------------------------------

fn id_of(memory: &Value) -> Option<String> {
    memory.get("id").and_then(Value::as_str).map(str::to_string)
}

fn str_field(memory: &Value, key: &str) -> String {
    memory
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn num_field(memory: &Value, key: &str) -> Option<f64> {
    memory.get(key).and_then(Value::as_f64)
}

fn str_array_field(memory: &Value, key: &str) -> Vec<String> {
    memory
        .get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::vector_indices::VectorEntryInput;
    use crate::db::Writer;
    use crate::model::embedding::CannedEmbeddingProvider;
    use tempfile::{tempdir, TempDir};

    /// A throwaway base64 pepper keys the fresh encrypted DB (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR: &str = "char-1";

    /// The minimal DDL the gate touches — the column sets the memories /
    /// vector_indices / vector_entries repos name (SQLite is dynamically typed, so
    /// this is enough for a *functional* self-test; byte-exactness is the
    /// differential's job).
    const DDL: &str = "
        CREATE TABLE memories (
            id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
            projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
            importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
            sourceMessageId TEXT, lastAccessedAt TEXT, reinforcementCount REAL,
            lastReinforcedAt TEXT, relatedMemoryIds TEXT, reinforcedImportance REAL,
            createdAt TEXT, updatedAt TEXT);
        CREATE TABLE vector_indices (
            id TEXT PRIMARY KEY, characterId TEXT, version REAL, dimensions REAL,
            createdAt TEXT, updatedAt TEXT);
        CREATE TABLE vector_entries (
            id TEXT PRIMARY KEY, characterId TEXT, embedding BLOB, createdAt TEXT);
    ";

    /// Build a fresh encrypted DB with the three tables, optionally seeding one
    /// existing memory + its vector, then open a `Db` over it.
    fn make_db(seed: Option<(&str, &str, Vec<f32>)>) -> (TempDir, Db) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
            if let Some((content, summary, vec)) = seed {
                w.memories()
                    .create(
                        &MemCreate {
                            character_id: CHAR.to_string(),
                            about_character_id: None,
                            chat_id: None,
                            project_id: None,
                            content: content.to_string(),
                            summary: summary.to_string(),
                            keywords: vec![],
                            tags: vec![],
                            importance: 0.5,
                            embedding: None,
                            source: "AUTO".to_string(),
                            witnessed_context: None,
                            source_message_id: None,
                            last_accessed_at: None,
                            reinforcement_count: 1.0,
                            last_reinforced_at: None,
                            related_memory_ids: vec![],
                            reinforced_importance: 0.5,
                        },
                        &CreateOptions {
                            id: "mem-seed".to_string(),
                            created_at: "2020-01-01T00:00:00.000Z".to_string(),
                            updated_at: "2020-01-01T00:00:00.000Z".to_string(),
                        },
                    )
                    .unwrap();
                let vi = w.vector_indices();
                vi.save_meta(CHAR, vec.len() as f64).unwrap();
                vi.add_entry(&VectorEntryInput {
                    id: "mem-seed".to_string(),
                    character_id: CHAR.to_string(),
                    embedding: Some(vec),
                })
                .unwrap();
            }
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn opts() -> MemoryServiceOptions {
        MemoryServiceOptions {
            user_id: "user-1".to_string(),
            embedding_profile_id: None,
        }
    }

    fn candidate(content: &str, summary: &str) -> CreateMemoryOptions {
        CreateMemoryOptions {
            character_id: CHAR.to_string(),
            content: content.to_string(),
            summary: summary.to_string(),
            source: Some("AUTO".to_string()),
            ..Default::default()
        }
    }

    fn count(db: &Db, table: &str) -> i64 {
        db.read_main(|c| {
            Ok(c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
        })
        .unwrap()
    }

    /// An empty store → INSERT: a new memory row, a new vector entry, and a fresh
    /// metadata row all land — the service driving a write through the Unit-0
    /// channel end to end.
    #[tokio::test]
    async fn empty_store_inserts_and_persists_vector() {
        let (_dir, db) = make_db(None);
        let provider = CannedEmbeddingProvider::new().with_vector("s\n\nnew fact", vec![1.0, 0.0]);
        let outcome = create_memory_with_gate(&db, &provider, &candidate("new fact", "s"), &opts())
            .await
            .unwrap();
        assert_eq!(outcome.action, GateAction::Insert);
        assert!(outcome.memory_id.is_some());
        assert_eq!(count(&db, "memories"), 1);
        assert_eq!(count(&db, "vector_entries"), 1);
        assert_eq!(count(&db, "vector_indices"), 1);
    }

    /// A best match in the merge band → REINFORCE: no new row, the existing memory's
    /// reinforcementCount bumps to 2, no content change.
    #[tokio::test]
    async fn merge_band_reinforces_existing() {
        let (_dir, db) = make_db(Some((
            "Mara trusts the crew completely.",
            "sum",
            vec![1.0, 0.0],
        )));
        // dot([0.87, 0.49299], [1,0]) = 0.87 -> [MERGE, NEAR_DUPLICATE) -> REINFORCE.
        let provider = CannedEmbeddingProvider::new()
            .with_vector("sum\n\nMara fully trusts the crew.", vec![0.87, 0.492_99]);
        let outcome = create_memory_with_gate(
            &db,
            &provider,
            &candidate("Mara fully trusts the crew.", "sum"),
            &opts(),
        )
        .await
        .unwrap();
        assert_eq!(outcome.action, GateAction::Reinforce);
        assert_eq!(outcome.memory_id.as_deref(), Some("mem-seed"));
        assert_eq!(count(&db, "memories"), 1, "no new row");
        let rc: f64 = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT reinforcementCount FROM memories WHERE id = 'mem-seed'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(rc, 2.0);
    }

    /// A best match at/above the near-duplicate threshold → SKIP_NEAR_DUPLICATE:
    /// nothing is written; the existing memory is returned.
    #[tokio::test]
    async fn near_duplicate_skips_without_writing() {
        let (_dir, db) = make_db(Some(("Mara trusts the crew.", "sum", vec![1.0, 0.0])));
        // dot([0.97, 0.24310], [1,0]) = 0.97 >= 0.90 -> SKIP_NEAR_DUPLICATE.
        let provider = CannedEmbeddingProvider::new()
            .with_vector("sum\n\nMara trusts the crew.", vec![0.97, 0.243_1]);
        let outcome = create_memory_with_gate(
            &db,
            &provider,
            &candidate("Mara trusts the crew.", "sum"),
            &opts(),
        )
        .await
        .unwrap();
        assert_eq!(outcome.action, GateAction::SkipNearDuplicate);
        assert_eq!(outcome.memory_id.as_deref(), Some("mem-seed"));
        assert_eq!(count(&db, "memories"), 1);
        assert_eq!(count(&db, "vector_entries"), 1);
    }

    /// The embedding provider failing on both attempts → SKIP_EMBEDDING_FAILED: no
    /// write, a reason surfaced.
    #[tokio::test]
    async fn embedding_failure_skips() {
        let (_dir, db) = make_db(None);
        let provider = CannedEmbeddingProvider::new().with_failure("s\n\nc");
        let outcome = create_memory_with_gate(&db, &provider, &candidate("c", "s"), &opts())
            .await
            .unwrap();
        assert_eq!(outcome.action, GateAction::SkipEmbeddingFailed);
        assert!(outcome.memory_id.is_none());
        assert!(outcome.reason.is_some());
        assert_eq!(count(&db, "memories"), 0);
    }
}
