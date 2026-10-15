//! The index's two maintenance operations behind the Memories API — P4.D292
//! (L-3), a port of v4 `lib/memory/index-store/backfill.ts`'s
//! `generateMissingEmbeddings` / `rebuildVectorIndex` at the `83334b9a5` pin
//! ("Moved from the retired `memory-service.ts` (memory refactor Phase E)",
//! `584d28164`; rewritten for the rows-as-index by `af48f6cc2`). The third
//! function of v4's module, `enqueueMissingEmbeddings`, is P4.D288's
//! ([`crate::services::memory_index::enqueue_missing_embeddings`], C1-3).
//!
//! "backfill the rows that have no embedding, and reload a character's cache
//! from its rows." v5 keeps no cache (the rows ARE the index), so the rebuild
//! is a read; there is no `batchSize` and no periodic save (v4 dropped both).

use serde_json::Value;

use crate::db::memories_read;
use crate::db::runtime::Db;
use crate::db::vector_store::Tier;
use crate::db::DbError;
use crate::memory_embedding_text::{embedding_text_for, lexical_embedding_text_for};
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
use crate::services::embedding_generate_job::embedding_text_source;
use crate::services::memory_index::{self, MemoryIndexUpsert};
use crate::services::memory_time_zone::refresh_memory_time_zone;

/// v4 `generateMissingEmbeddings`'s `{ processed, failed, skipped }`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GenerateMissingEmbeddingsResult {
    pub processed: usize,
    pub failed: usize,
    /// Always 0 in v4 (the counter exists but nothing increments it) —
    /// carried for response-shape fidelity.
    pub skipped: usize,
}

/// v4 `rebuildVectorIndex`'s `{ indexed, failed }` — `failed` now means "rows
/// without an embedding" (it meant "add failures" before `af48f6cc2`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RebuildVectorIndexResult {
    pub indexed: usize,
    pub failed: usize,
}

/// v4 `!m.embedding || m.embedding.length === 0` over a hydrated row (its
/// stored vector reads as the `Float32Array`-object `{"0": x, …}`).
fn embedding_missing(memory: &Value) -> bool {
    match memory.get("embedding") {
        Some(Value::Object(o)) => o.is_empty(),
        _ => true,
    }
}

/// JS `String(error)` for what the embedding seam threw: v4's `EmbeddingError`
/// (`name = 'EmbeddingError'`, carries a provider) renders `EmbeddingError:
/// <message>`; a plain error escaping the service unwrapped (v4's dead outer
/// catch — `embedding_provider_tier3`'s note) renders `Error: <message>`.
fn js_error_string(message: &str, embedding_error: bool) -> String {
    if embedding_error {
        format!("EmbeddingError: {message}")
    } else {
        format!("Error: {message}")
    }
}

/// v4 `generateMissingEmbeddings(characterId, { userId, embeddingProfileId })`
/// (`backfill.ts:29-74`): every row of the character with no embedding (or an
/// empty one), in `findByCharacterId` order; the memory-calendar zone once
/// ("Anchor dates render on the instance's memory calendar"); per row the
/// ANCHORED text with `lexical_text` (background priority), then "Row and
/// index in one call; the row's tier rides with the vector" through
/// [`memory_index::upsert`]. A per-row failure (the embed OR the write) logs
/// WARN `` `[Memory] Failed to generate embedding for memory ${id}` `` (the id
/// IS in the message) `{characterId, memoryId, userId, error: String(error)}`
/// and counts; the sweep goes on. Ends with DEBUG `[Memory] Backfilled missing
/// embeddings {characterId, processed, failed, skipped}`. The host supplies
/// its zone NAME and wall clock (core reads neither ambiently).
pub async fn generate_missing_embeddings<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    character_id: &str,
    user_id: &str,
    embedding_profile_id: Option<&str>,
    host_zone_fallback: &str,
    now_ms: i64,
) -> Result<GenerateMissingEmbeddingsResult, DbError> {
    let cid = character_id.to_string();
    let memories = db.read_main(move |c| memories_read::find_by_character_id(c, &cid))?;
    let missing: Vec<&Value> = memories.iter().filter(|m| embedding_missing(m)).collect();

    let mut result = GenerateMissingEmbeddingsResult::default();
    let memory_time_zone = refresh_memory_time_zone(db, host_zone_fallback, now_ms as f64);

    for memory in missing {
        let id = memory
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let source = embedding_text_source(memory);
        let tier = if memory.get("tier").and_then(Value::as_str) == Some("cold") {
            Tier::Cold
        } else {
            Tier::Hot
        };
        let step: Result<(), String> = async {
            let embedded = provider
                .generate_embedding_for_user(
                    &embedding_text_for(&source, &memory_time_zone),
                    user_id,
                    embedding_profile_id,
                    EmbeddingCallOptions {
                        lexical_text: Some(lexical_embedding_text_for(&source)),
                        ..EmbeddingCallOptions::background()
                    },
                )
                .await
                .map_err(|e| js_error_string(&e.message, e.provider.is_some()))?;
            let (cid, mid, vec) = (character_id.to_string(), id.clone(), embedded.embedding);
            db.write(move |ws| {
                memory_index::upsert(
                    ws.main().connection(),
                    &cid,
                    vec![MemoryIndexUpsert {
                        id: mid,
                        embedding: vec,
                        tier: Some(tier),
                        ..Default::default()
                    }],
                )
            })
            .await
            .map_err(|e| js_error_string(&crate::db::fallback::error_text(&e), false))?;
            Ok(())
        }
        .await;

        match step {
            Ok(()) => result.processed += 1,
            Err(error) => {
                tracing::warn!(
                    target: "quilltap::memories",
                    characterId = %character_id,
                    memoryId = %id,
                    userId = %user_id,
                    error = %error,
                    "[Memory] Failed to generate embedding for memory {id}",
                );
                result.failed += 1;
            }
        }
    }

    tracing::debug!(
        target: "quilltap::memories",
        characterId = %character_id,
        processed = result.processed,
        failed = result.failed,
        skipped = result.skipped,
        "[Memory] Backfilled missing embeddings",
    );
    Ok(result)
}

/// v4 `rebuildVectorIndex(characterId)` (`backfill.ts:85-101`): "The rows are
/// the index (`memories.embedding` + `memories.tier`); the in-process store is
/// only a cache of them. Rebuilding drops the cache and reads the rows again,
/// so the tiers come with the vectors (bug 205 cannot recur)." `indexed` = the
/// entries the reloaded index holds (decodable rows), `failed` = the rows with
/// `embedding IS NULL` (a zero-length blob counts in neither), DEBUG `[Memory]
/// Rebuilt vector index from memory rows {characterId, indexed, coldIndexed,
/// withoutEmbedding}`. Nothing is written; v4's `reload` (a cache drop) has no
/// v5 twin.
pub async fn rebuild_vector_index(
    db: &Db,
    character_id: &str,
) -> Result<RebuildVectorIndexResult, DbError> {
    let cid = character_id.to_string();
    let (indexed, cold_indexed, failed) = db.read_main(move |c| {
        let entries = memory_index::entries(c, &cid);
        let cold = entries.iter().filter(|e| e.tier == Tier::Cold).count();
        let failed = memories_read::count_without_embedding(c, Some(&cid))? as usize;
        Ok((entries.len(), cold, failed))
    })?;
    tracing::debug!(
        target: "quilltap::memories",
        characterId = %character_id,
        indexed = indexed,
        coldIndexed = cold_indexed,
        withoutEmbedding = failed,
        "[Memory] Rebuilt vector index from memory rows",
    );
    Ok(RebuildVectorIndexResult { indexed, failed })
}
