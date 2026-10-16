//! The writes behind an admission decision and a fold — v4
//! `lib/memory/admission/apply.ts` (`8af764514`; bugs 204 / 216 / 226 / 231 /
//! 234).
//!
//! v4's why, carried: "Every write here lands on the parent's read-write
//! connection and returns the row as it now stands — no buffered local views.
//! Counts, footnotes and links are written as deltas against the row at write
//! time (`incrementReinforcement`, `appendReinforcementFootnotes`,
//! `addRelatedMemoryIds`), so two admissions of the same row never overwrite
//! each other's (bugs 182, 231)."
//!
//! Each step is its OWN writer job (v4 awaits each write in turn), and every
//! line of this module is logged on the caller thread after its write
//! returns; the repository lines a step triggers (`Memory reinforcement
//! counted`, `Reinforcement footnotes appended`, `Memory links updated`) are
//! P4.D288's, logged on the writer thread.

use serde_json::{Map, Value};

use crate::admission::{AdmissionRow, FoldPlan};
use crate::db::memories::{CreateOptions, MemCreate, MemUpdate};
use crate::db::memories_read;
use crate::db::runtime::Db;
use crate::db::vector_store::Tier;
use crate::db::{js_number_to_json, DbError};
use crate::memory_embedding_text::{embedding_text_for, lexical_embedding_text_for};
use crate::memory_scoring::reinforced_importance;
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
use crate::services::memory_index::{self, MemoryIndexEntry, MemoryIndexUpsert};
use crate::services::memory_time_zone::refresh_memory_time_zone;

use super::{now_ms, text_source_of};

/// v4's child-logger binding (`logger.child({ module:
/// 'memory:admission:apply' })`).
const MODULE: &str = "memory:admission:apply";

fn id_of(memory: &Value) -> &str {
    memory.get("id").and_then(Value::as_str).unwrap_or_default()
}

/// A row without its vector, for outcomes (v4 `withoutEmbedding`).
pub fn without_embedding(memory: Value) -> Value {
    match memory {
        Value::Object(mut m) => {
            // `shift_remove`: a preserve-order `Map`'s `remove` is a swap-remove,
            // which would drop the LAST key into `embedding`'s slot; v4's
            // `withoutEmbedding` keeps `MemorySchema` order (the `83334b9a5`
            // unification, P4.D298's `CREATE_KEY_ORDER_RESIDUE` finding).
            m.shift_remove("embedding");
            Value::Object(m)
        }
        other => other,
    }
}

/// A vector as the row JSON carries it — the `Float32Array` object shape
/// `{"0":v0,…}` [`memories_read`] marshals.
pub(crate) fn embedding_value(embedding: &[f32]) -> Value {
    let mut m = Map::new();
    for (i, f) in embedding.iter().enumerate() {
        m.insert(i.to_string(), js_number_to_json(*f as f64));
    }
    Value::Object(m)
}

/// v4's `x || null` over an optional string.
fn truthy(s: &Option<String>) -> Option<String> {
    s.clone().filter(|s| !s.is_empty())
}

/// The entity v4's `memories.create` returns for `insertRow`'s data — the
/// `MemorySchema`-parsed `{ ...data, tier, consolidatedFrom, id, createdAt,
/// updatedAt }`: the schema's key order, every key `insertRow` names present
/// (its explicit `null`s included), the two it does not name
/// (`lastAccessedAt`, `lastReinforcedAt`) and the digest-only tier fields
/// absent. Not a re-read: v4 answers the create, and the manual create's
/// wire (`{ memory, action }`) carries these bytes.
fn created_entity(c: &MemCreate, opts: &CreateOptions) -> Value {
    let opt = |s: &Option<String>| s.clone().map_or(Value::Null, Value::String);
    serde_json::json!({
        "id": opts.id,
        "characterId": c.character_id,
        "aboutCharacterId": opt(&c.about_character_id),
        "chatId": opt(&c.chat_id),
        "projectId": opt(&c.project_id),
        "content": c.content,
        "summary": c.summary,
        "keywords": c.keywords,
        "tags": c.tags,
        "importance": js_number_to_json(c.importance),
        "embedding": embedding_value(c.embedding.as_deref().unwrap_or_default()),
        "source": c.source,
        "witnessedContext": opt(&c.witnessed_context),
        "occurredAt": opt(&c.occurred_at),
        "narrativeTime": opt(&c.narrative_time),
        "entities": c.entities,
        "kind": c.kind,
        "sourceMessageId": opt(&c.source_message_id),
        "reinforcementCount": js_number_to_json(c.reinforcement_count),
        "relatedMemoryIds": c.related_memory_ids,
        "reinforcedImportance": js_number_to_json(c.reinforced_importance),
        "tier": c.tier,
        "consolidatedFrom": c.consolidated_from,
        "createdAt": opts.created_at,
        "updatedAt": opts.updated_at,
    })
}

/// Insert a new row together with its embedding (v4 `insertRow`). "The
/// embedding rides in the `create` itself, so no row exists without its
/// vector (invariant 4) and none is written twice." The id is minted here
/// (v4's repository mints it); `createdAt` / `updatedAt` are the source
/// message's time when the row carries one, else now. Then the index is told
/// (`index_inserted` — P4.D288's line) and DEBUG `New memory persisted
/// {module, characterId, memoryId, source}` (`source` OMITTED when the row
/// has none — v4's `undefined` field). Returns the row as written, with its
/// vector.
pub async fn insert_row(db: &Db, row: &AdmissionRow, embedding: &[f32]) -> Result<Value, DbError> {
    let importance = row.importance.unwrap_or(0.5);
    let id = uuid::Uuid::new_v4().to_string();
    let (created_at, updated_at) = match truthy(&row.source_message_timestamp) {
        Some(ts) => (ts.clone(), ts),
        None => {
            let now = crate::clock::now_iso();
            (now.clone(), now)
        }
    };
    let create = MemCreate {
        character_id: row.character_id.clone(),
        about_character_id: truthy(&row.about_character_id),
        chat_id: truthy(&row.chat_id),
        project_id: row.project_id.clone(),
        content: row.content.clone(),
        summary: row.summary.clone(),
        keywords: row.keywords.clone().unwrap_or_default(),
        tags: row.tags.clone().unwrap_or_default(),
        importance,
        embedding: Some(embedding.to_vec()),
        source: truthy(&row.source).unwrap_or_else(|| "MANUAL".to_string()),
        witnessed_context: row.witnessed_context.clone(),
        occurred_at: row.occurred_at.clone(),
        narrative_time: row.narrative_time.clone(),
        entities: row.entities.clone().unwrap_or_default(),
        kind: row.kind.clone().unwrap_or_else(|| "semantic".to_string()),
        source_message_id: truthy(&row.source_message_id),
        last_accessed_at: None,
        reinforcement_count: 1.0,
        last_reinforced_at: None,
        related_memory_ids: Vec::new(),
        // Bug 216: one formula for every writer — at count 1 it equals
        // importance.
        reinforced_importance: reinforced_importance(importance, 1.0),
        tier: "hot".to_string(),
        superseded_by_id: None,
        superseded_at: None,
        consolidated_from: Vec::new(),
        consolidated_at: None,
    };
    let opts = CreateOptions {
        id: id.clone(),
        created_at,
        updated_at,
    };
    let memory = created_entity(&create, &opts);
    db.write(move |w| w.main().memories().create(&create, &opts))
        .await?;
    // A new row is always hot.
    memory_index::index_inserted(
        &row.character_id,
        &[MemoryIndexEntry {
            id: id.clone(),
            embedding: embedding.to_vec(),
            tier: Some(Tier::Hot),
        }],
    );
    match &row.source {
        Some(source) => tracing::debug!(
            module = MODULE,
            characterId = %row.character_id,
            memoryId = %id,
            source = %source,
            "New memory persisted"
        ),
        None => tracing::debug!(
            module = MODULE,
            characterId = %row.character_id,
            memoryId = %id,
            "New memory persisted"
        ),
    }
    Ok(memory)
}

/// Re-embed a row from its current text and write the vector through the
/// index, tier kept (v4 `reembedRow`). Best-effort: any failure — the
/// embedding or the write — logs WARN `Failed to re-embed memory; it keeps
/// its old vector {module, memoryId, error}` and answers `None`; otherwise
/// DEBUG `Re-embedded memory {module, memoryId}` and the new vector. The text
/// carries the anchor line in the memory-calendar zone (bug 204), the call
/// `lexical_text` (C1-10).
pub async fn reembed_row<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    memory: &Value,
    user_id: &str,
    host_zone_fallback: &str,
) -> Option<Vec<f32>> {
    let memory_id = id_of(memory).to_string();
    let character_id = memory
        .get("characterId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let tier = if memory.get("tier").and_then(Value::as_str) == Some("cold") {
        Tier::Cold
    } else {
        Tier::Hot
    };
    let source = text_source_of(memory);
    let time_zone = refresh_memory_time_zone(db, host_zone_fallback, now_ms());
    let text = embedding_text_for(&source, &time_zone);
    let call =
        EmbeddingCallOptions::background().with_lexical_text(lexical_embedding_text_for(&source));
    let result: Result<Vec<f32>, String> = async {
        let embedding = provider
            .generate_embedding_for_user(&text, user_id, None, call)
            .await
            .map_err(|e| e.message)?
            .embedding;
        let (cid, mid, written) = (character_id.clone(), memory_id.clone(), embedding.clone());
        db.write(move |w| {
            memory_index::upsert(
                w.main().connection(),
                &cid,
                vec![MemoryIndexUpsert {
                    id: mid,
                    embedding: written,
                    tier: Some(tier),
                    alongside: MemUpdate::default(),
                }],
            )
        })
        .await
        .map_err(|e| e.to_string())?;
        Ok(embedding)
    }
    .await;
    match result {
        Ok(embedding) => {
            tracing::debug!(module = MODULE, memoryId = %memory_id, "Re-embedded memory");
            Some(embedding)
        }
        Err(error) => {
            tracing::warn!(
                module = MODULE,
                memoryId = %memory_id,
                error = %error,
                "Failed to re-embed memory; it keeps its old vector"
            );
            None
        }
    }
}

/// One fold step's write missed its row (v4's `missed(step)`): before the
/// commit point the fold is off (WARN `Fold target is gone; nothing folded`,
/// `true` — return `None`); after it the fold stands (WARN `Fold step missed
/// after the fold landed; the fold stands`, `false` — carry on).
fn missed(landed: bool, survivor_id: &str, character_id: &str, step: &str) -> bool {
    if !landed {
        tracing::warn!(
            module = MODULE,
            survivorId = %survivor_id,
            characterId = %character_id,
            step = step,
            "Fold target is gone; nothing folded"
        );
        return true;
    }
    tracing::warn!(
        module = MODULE,
        survivorId = %survivor_id,
        characterId = %character_id,
        step = step,
        "Fold step missed after the fold landed; the fold stands"
    );
    false
}

/// `{ ...a, key: value }` on a row's JSON.
fn with_key(mut memory: Value, key: &str, value: Value) -> Value {
    if let Some(m) = memory.as_object_mut() {
        m.insert(key.to_string(), value);
    }
    memory
}

/// v4's `Number.isInteger(by) && by >= 1` for the repository's `by`, else
/// v4's throw text.
fn reinforcement_by(delta: f64) -> Result<i64, DbError> {
    if delta.is_finite() && delta.fract() == 0.0 && delta >= 1.0 {
        Ok(delta as i64)
    } else {
        Err(DbError::Internal(format!(
            "Memory reinforcement: delta must be a positive integer (got {})",
            crate::pascal::js_value::number_to_string(delta)
        )))
    }
}

/// Write a fold plan to its survivor and re-embed it when the plan says so —
/// v4 `applyFold(survivor, plan, { userId })`. Returns the survivor as it now
/// stands (with its vector — the new one after a re-embed), or `None` when
/// nothing landed (the row is gone).
///
/// v4's why, carried: "The first write that lands is the commit point: once
/// the survivor holds what the absorbed sources knew, a later miss is logged
/// and the fold stands, so a caller deleting the absorbed rows does not keep
/// them beside a survivor that already took them in."
///
/// The four steps in v4's order, each a delta against the row at write time:
/// `footnotes` (capped append), `anchors` (the patch in one row update),
/// `links` (added), `reinforcement` (counted, bug 226). The re-embed runs only
/// when something landed and the plan says the embedded text changed (bug
/// 204 — also at the footnote cap); without a `user_id` (or a provider) a
/// changed survivor keeps its old vector, WARN `Survivor changed but no
/// userId to re-embed with {module, survivorId}`. DEBUG `Folded into survivor
/// {module, survivorId, characterId, footnotes, anchorsJson, links,
/// reinforcementDelta, reembedded}`.
///
/// NO-PORT (LR-4): v4's job-child refusal — v5 has no child.
pub async fn apply_fold<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: Option<&P>,
    survivor: &Value,
    plan: &FoldPlan,
    user_id: Option<&str>,
    host_zone_fallback: &str,
) -> Result<Option<Value>, DbError> {
    let character_id = survivor
        .get("characterId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let id = id_of(survivor).to_string();
    let mut current = survivor.clone();
    let mut landed = false;

    if !plan.footnotes.is_empty() {
        let (cid, mid, details) = (character_id.clone(), id.clone(), plan.footnotes.clone());
        let written = db
            .write(move |w| {
                w.main()
                    .memories()
                    .append_reinforcement_footnotes(&cid, &mid, &details)
            })
            .await?;
        match written {
            None => {
                if missed(landed, &id, &character_id, "footnotes") {
                    return Ok(None);
                }
            }
            Some(w) => {
                current = with_key(current, "content", Value::String(w.content));
                landed = true;
            }
        }
    }

    if !plan.patch.is_empty() {
        let patch = MemUpdate {
            occurred_at: plan.patch.occurred_at.clone().map(Some),
            narrative_time: plan.patch.narrative_time.clone().map(Some),
            entities: plan.patch.entities.clone(),
            ..Default::default()
        };
        let (cid, mid) = (character_id.clone(), id.clone());
        let row = db
            .write(move |w| {
                let main = w.main();
                if !main.memories().update_for_character(&cid, &mid, &patch)? {
                    return Ok(None);
                }
                memories_read::find_by_id(main.connection(), &mid)
            })
            .await?;
        match row {
            None => {
                if missed(landed, &id, &character_id, "anchors") {
                    return Ok(None);
                }
            }
            Some(row) => {
                current = row;
                landed = true;
            }
        }
    }

    if !plan.add_links.is_empty() {
        let (cid, mid, links) = (character_id.clone(), id.clone(), plan.add_links.clone());
        let written = db
            .write(move |w| {
                w.main()
                    .memories()
                    .add_related_memory_ids(&cid, &mid, &links)
            })
            .await?;
        match written {
            None => {
                if missed(landed, &id, &character_id, "links") {
                    return Ok(None);
                }
            }
            Some(links) => {
                current = with_key(current, "relatedMemoryIds", serde_json::json!(links));
                landed = true;
            }
        }
    }

    if plan.reinforcement_delta > 0.0 {
        let by = reinforcement_by(plan.reinforcement_delta)?;
        let at = plan
            .reinforced_at
            .clone()
            .unwrap_or_else(crate::clock::now_iso);
        let (cid, mid) = (character_id.clone(), id.clone());
        let counted = db
            .write(move |w| {
                w.main()
                    .memories()
                    .increment_reinforcement(&cid, &mid, &at, by)
            })
            .await?;
        match counted {
            None => {
                if missed(landed, &id, &character_id, "reinforcement") {
                    return Ok(None);
                }
            }
            Some(c) => {
                current = with_key(
                    current,
                    "reinforcementCount",
                    js_number_to_json(c.reinforcement_count),
                );
                current = with_key(
                    current,
                    "reinforcedImportance",
                    js_number_to_json(c.reinforced_importance),
                );
                current = with_key(
                    current,
                    "lastReinforcedAt",
                    Value::String(c.last_reinforced_at),
                );
                landed = true;
            }
        }
    }

    let mut reembedded = false;
    if plan.reembed && landed {
        match (user_id, provider) {
            (Some(user_id), Some(provider)) => {
                if let Some(embedding) =
                    reembed_row(db, provider, &current, user_id, host_zone_fallback).await
                {
                    current = with_key(current, "embedding", embedding_value(&embedding));
                    reembedded = true;
                }
            }
            _ => {
                tracing::warn!(
                    module = MODULE,
                    survivorId = %id,
                    "Survivor changed but no userId to re-embed with"
                );
            }
        }
    }

    tracing::debug!(
        module = MODULE,
        survivorId = %id,
        characterId = %character_id,
        footnotes = plan.footnotes.len(),
        anchorsJson = %serde_json::to_string(&plan.patch.keys()).unwrap_or_default(),
        links = plan.add_links.len(),
        reinforcementDelta = plan.reinforcement_delta,
        reembedded = reembedded,
        "Folded into survivor"
    );
    Ok(Some(current))
}

/// A hand-written candidate folded into a row raises that row's importance
/// to the one asked for, when higher, and recomputes `reinforcedImportance`
/// from the row's count (v4 `raiseImportance`). "It never folds into an
/// extracted row: the decider links it instead." A missed update logs WARN
/// `Failed to raise the importance of a row a manual memory folded into
/// {module, memoryId}` and keeps the row; a raise logs DEBUG `Raised
/// importance for a manual write {module, memoryId, importance}`. Never
/// lowers.
pub async fn raise_importance(
    db: &Db,
    memory: Value,
    requested: Option<f64>,
) -> Result<Value, DbError> {
    let current = memory
        .get("importance")
        .and_then(Value::as_f64)
        .unwrap_or(f64::NAN);
    let Some(requested) = requested else {
        return Ok(memory);
    };
    // v4 `requested <= memory.importance` — a NaN comparison is false, so a
    // row with no numeric importance is raised.
    if requested <= current {
        return Ok(memory);
    }
    let count = memory
        .get("reinforcementCount")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    let patch = MemUpdate {
        importance: Some(requested),
        reinforced_importance: Some(reinforced_importance(requested, count)),
        ..Default::default()
    };
    let character_id = memory
        .get("characterId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let memory_id = id_of(&memory).to_string();
    let (cid, mid) = (character_id.clone(), memory_id.clone());
    let updated = db
        .write(move |w| {
            let main = w.main();
            if !main.memories().update_for_character(&cid, &mid, &patch)? {
                return Ok(None);
            }
            memories_read::find_by_id(main.connection(), &mid)
        })
        .await?;
    match updated {
        None => {
            tracing::warn!(
                module = MODULE,
                memoryId = %memory_id,
                "Failed to raise the importance of a row a manual memory folded into"
            );
            Ok(memory)
        }
        Some(updated) => {
            tracing::debug!(
                module = MODULE,
                memoryId = %memory_id,
                importance = requested,
                "Raised importance for a manual write"
            );
            Ok(updated)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_embedding_drops_only_the_vector() {
        let m = serde_json::json!({ "id": "a", "embedding": { "0": 1 }, "content": "x", "updatedAt": "t" });
        // By BYTES: `Value ==` is key-order-blind, and the order is the point.
        assert_eq!(
            serde_json::to_string(&without_embedding(m)).unwrap(),
            r#"{"id":"a","content":"x","updatedAt":"t"}"#
        );
    }

    #[test]
    fn a_reinforcement_delta_must_be_a_positive_integer() {
        assert_eq!(reinforcement_by(1.0).unwrap(), 1);
        assert_eq!(reinforcement_by(3.0).unwrap(), 3);
        let err = reinforcement_by(1.5).unwrap_err().to_string();
        assert!(
            err.contains("delta must be a positive integer (got 1.5)"),
            "{err}"
        );
    }
}
