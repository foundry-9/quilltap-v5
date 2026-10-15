//! The memory service — v4 `lib/memory/memory-service.ts`.
//!
//! P4.D288 (C1-7): v4's cascade-delete family (`deleteMemoryWithVector`, the
//! two `deleteMemoriesBySourceMessage…WithVectors`, and
//! `deleteMemoriesByChatIdWithVectors`) was DELETED with `af48f6cc2` — every
//! deletion runs through the ONE retire path,
//! [`crate::services::memory_retire`].
//!
//! P4.D291: v4 Phase E (`584d28164`) DELETED `searchMemoriesSemantic` — recall
//! is the pipeline in [`crate::services::recall`] (query → gather → hydrate →
//! score → select, with [`crate::services::recall::search::search_memories`] as
//! the one-shot form). ONE forwarding shim, [`search_memories_semantic`],
//! survives here for the memories search route (`api/memories.rs`, P4.D298's
//! file); it keeps the old failure contract and goes at unification. What
//! remains beside it is the two repair services behind the memories routes.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::vector_store::CharacterVectorStore;
use crate::db::{memories_read, DbError};
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
use crate::recall_tags::TimeWindow;
use crate::services::recall::search::{
    search_memories, RecallSearchOptions, SearchMemoriesOptions,
};

pub use crate::services::recall::context::RecallContextInput;
pub use crate::services::recall::query::EmbeddingMemo;

/// v4 `SemanticSearchResult` — the recall pipeline's candidate (the shim's
/// result type; `memory`, `score` and `used_embedding` are what its one caller
/// reads).
pub type SemanticSearchResult = crate::services::recall::RecallCandidate;

/// Options for [`search_memories_semantic`] — the field set as it stood before
/// P4.D291, mapped field for field onto [`SearchMemoriesOptions`].
#[derive(Debug, Clone, Default)]
pub struct SemanticSearchOptions {
    pub user_id: String,
    pub embedding_profile_id: Option<String>,
    /// v4 default 20.
    pub limit: Option<usize>,
    /// Explicit cosine floor; `None` → the provider default.
    pub min_score: Option<f64>,
    pub min_importance: Option<f64>,
    /// `'AUTO' | 'MANUAL'` filter.
    pub source: Option<String>,
    /// Restrict to memories held *about* this other character.
    pub about_character_id: Option<String>,
    /// Union verbatim query hits into the candidate pool + boost their cosine.
    pub apply_literal_phrase_boost: bool,
    pub recall_context: Option<RecallContextInput>,
    /// Event-time window.
    pub occurred_within: Option<TimeWindow>,
    /// Entity strings unioned into the candidate pool.
    pub entity_anchors: Vec<String>,
    /// Additional embedding probes (≤ 2).
    pub extra_probes: Vec<String>,
    /// The ranking clock (epoch millis).
    pub now_ms: f64,
    /// Search both tiers (default false — hot only).
    pub include_cold: bool,
    /// Ids never returned.
    pub exclude_memory_ids: Vec<String>,
    /// The dynamic head's entry cap for this search.
    pub head_size: Option<usize>,
    pub embedding_memo: Option<EmbeddingMemo>,
}

/// A query and the vector embedded for it — kept for the shim's signature; the
/// pipeline hands its query back through
/// [`crate::services::recall::query::RecallQuery`] instead.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchQueryEmbedding {
    pub query: String,
    pub embedding: Vec<f32>,
}

/// The pre-P4.D291 entry point, forwarding to the recall pipeline's one-shot
/// [`search_memories`]. Keeps the old failure contract (`Result<Vec<_>,
/// DbError>`); the capture out-param is never filled. The three companion
/// types above ([`SemanticSearchResult`], [`SemanticSearchOptions`],
/// [`SearchQueryEmbedding`]) are part of this one shim and go with it.
// LANE-REMOVE(P4.D291): v4 deleted searchMemoriesSemantic in 584d28164
pub async fn search_memories_semantic<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    character_id: &str,
    query: &str,
    options: &SemanticSearchOptions,
    _capture_query_embedding: Option<&mut Option<SearchQueryEmbedding>>,
) -> Result<Vec<SemanticSearchResult>, DbError> {
    let opts = SearchMemoriesOptions {
        search: RecallSearchOptions {
            limit: options.limit,
            min_score: options.min_score,
            min_importance: options.min_importance,
            source: options.source.clone(),
            about_character_id: options.about_character_id.clone(),
            include_cold: options.include_cold,
            exclude_ids: (!options.exclude_memory_ids.is_empty())
                .then(|| options.exclude_memory_ids.iter().cloned().collect()),
            literal_phrase: options.apply_literal_phrase_boost,
            recall_context: options.recall_context.clone(),
            head_size: options.head_size,
            now_ms: options.now_ms,
            ..Default::default()
        },
        user_id: options.user_id.clone(),
        embedding_profile_id: options.embedding_profile_id.clone(),
        window: options.occurred_within.clone(),
        anchors: options.entity_anchors.clone(),
        probes: (!options.extra_probes.is_empty()).then(|| options.extra_probes.clone()),
        embedding_memo: options.embedding_memo.clone(),
    };
    search_memories(db, provider, character_id, query, &opts).await
}

// ============================================================================
// P4.6BL tier 2 — the two repair services behind the memories route arms
// (v4 `generateMissingEmbeddings` / `rebuildVectorIndex`,
// lib/memory/memory-service.ts:1325 / :1392).
// ============================================================================

/// v4 `generateMissingEmbeddings`'s `{ processed, failed, skipped }`.
#[derive(Debug, Default, PartialEq)]
pub struct GenerateMissingEmbeddingsResult {
    pub processed: usize,
    pub failed: usize,
    /// Always 0 in v4 (the counter exists but nothing increments it) —
    /// carried for response-shape fidelity.
    pub skipped: usize,
}

/// v4 `rebuildVectorIndex`'s `{ indexed, failed }`.
#[derive(Debug, Default, PartialEq)]
pub struct RebuildVectorIndexResult {
    pub indexed: usize,
    pub failed: usize,
}

/// The anchors view a hydrated memory row satisfies in v4 (the `memory` object
/// IS the `EpisodicAnchorView` there — `buildMemoryEmbeddingText(summary,
/// content, memory)`).
pub(crate) fn anchor_view_of(memory: &Value) -> crate::episodic::EpisodicAnchorView {
    let entities = match memory.get("entities") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(raw)) => serde_json::from_str::<Vec<String>>(raw).unwrap_or_default(),
        _ => Vec::new(),
    };
    crate::episodic::EpisodicAnchorView {
        occurred_at: memory
            .get("occurredAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        narrative_time: memory
            .get("narrativeTime")
            .and_then(Value::as_str)
            .map(str::to_string),
        entities,
    }
}

/// Decode the `Float32Array`-object shape `memories_read` emits for a stored
/// embedding (`{"0": x, "1": y, …}`) back to a vector. `None` for anything
/// else (null / absent / malformed).
pub(crate) fn embedding_vec_of(memory: &Value) -> Option<Vec<f32>> {
    let obj = memory.get("embedding")?.as_object()?;
    let mut out = vec![0f32; obj.len()];
    for (k, v) in obj {
        let i: usize = k.parse().ok()?;
        if i >= out.len() {
            return None;
        }
        out[i] = v.as_f64()? as f32;
    }
    Some(out)
}

/// True when the row's stored embedding is absent or empty (v4's
/// `!m.embedding || m.embedding.length === 0` filter).
fn embedding_missing(memory: &Value) -> bool {
    match embedding_vec_of(memory) {
        Some(v) => v.is_empty(),
        None => true,
    }
}

/// The PRE-SPAN `generateMissingEmbeddings(characterId, { userId, batchSize })`,
/// kept WHOLE for its one remaining caller (`api/memories.rs`'s `embeddings`
/// arm, P4.D298's file) until the union re-points it onto
/// [`crate::services::memory_index_backfill::generate_missing_embeddings`]
/// (P4.D292's port of v4 `index-store/backfill.ts` — anchored text in the
/// memory-calendar zone, `lexical_text`, the tier, v4's WARN / DEBUG, no
/// `batch_size`). Not a thin forward: this signature carries no host zone, and
/// core reads none ambiently (HANDOFF recorded in P4.D292's lane record).
// LANE-REMOVE(P4.D298): v4 deleted generateMissingEmbeddings from memory-service.ts in 584d28164
pub async fn generate_missing_embeddings<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    user_id: &str,
    character_id: &str,
    embedding_profile_id: Option<&str>,
    batch_size: usize,
) -> Result<GenerateMissingEmbeddingsResult, DbError> {
    let cid = character_id.to_string();
    let memories = db.read_main(move |c| memories_read::find_by_character_id(c, &cid))?;
    let missing: Vec<&Value> = memories.iter().filter(|m| embedding_missing(m)).collect();

    let mut result = GenerateMissingEmbeddingsResult::default();
    for memory in missing {
        let id = memory
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let summary = memory
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let content = memory
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let anchors = anchor_view_of(memory);
        let text = crate::episodic::build_memory_embedding_text(summary, content, Some(&anchors));

        // v4's try block: embed → updateForCharacter → store.addVector. Any
        // failure counts and the loop continues.
        let step: Result<(), String> = async {
            let embedded = provider
                .generate_embedding_for_user(
                    &text,
                    user_id,
                    embedding_profile_id,
                    // lexical_text: None until P4.D292 (the backfill — v4 `index-store/backfill.ts:58`) lands it (C1-10).
                    EmbeddingCallOptions::background(),
                )
                .await
                .map_err(|e| e.message)?;
            let (cid, mid, vec) = (
                character_id.to_string(),
                id.clone(),
                embedded.embedding.clone(),
            );
            // P4.D288 (C1-3, mechanical): row and index in one call — the ONE
            // writer of `memories.embedding` (v4 `memoryIndex.upsert`).
            db.write(move |ws| {
                crate::services::memory_index::upsert(
                    ws.main().connection(),
                    &cid,
                    vec![crate::services::memory_index::MemoryIndexUpsert {
                        id: mid,
                        embedding: vec,
                        ..Default::default()
                    }],
                )
            })
            .await
            .map_err(|e| format!("{e}"))?;
            Ok(())
        }
        .await;

        match step {
            Ok(()) => {
                // P4.D288: no store to save — each upsert wrote its row.
                result.processed += 1;
            }
            Err(e) => {
                tracing::warn!(
                    target: "quilltap::memories",
                    memory_id = %id,
                    character_id,
                    error = %e,
                    "[Memory] Failed to generate embedding for memory",
                );
                result.failed += 1;
            }
        }
    }

    let _ = batch_size;
    Ok(result)
}

/// A thin forward onto P4.D292's
/// [`crate::services::memory_index_backfill::rebuild_vector_index`] (v4
/// `index-store/backfill.ts:85-101`) for `api/memories.rs`'s `rebuild-index`
/// arm (P4.D298's file) until the union re-points the call.
// LANE-REMOVE(P4.D298): v4 deleted rebuildVectorIndex from memory-service.ts in 584d28164
pub async fn rebuild_vector_index(
    db: &Db,
    character_id: &str,
) -> Result<RebuildVectorIndexResult, DbError> {
    let r = crate::services::memory_index_backfill::rebuild_vector_index(db, character_id).await?;
    Ok(RebuildVectorIndexResult {
        indexed: r.indexed,
        failed: r.failed,
    })
}
