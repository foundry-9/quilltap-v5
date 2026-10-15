//! Stage 3 — hydrate: read the rows the pool names, in one batch, and apply
//! the row filters every path shares (v4 `lib/memory/recall/hydrate.ts`) —
//! tier (`includeCold`), exclusions, importance (`reinforcedImportance`),
//! source and subject (`aboutCharacterId`). The tool path's literal-phrase
//! boost lands here, on the relevance it gathered with.
//!
//! Also the one other read the pipeline makes: the one-hop neighbours of the
//! ranked hits, for related expansion ([`hydrate_neighbours`]).

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::db::runtime::Db;
use crate::db::{memories_read, DbError};
use crate::literal_boost::{apply_literal_boost, contains_literal_phrase};
use crate::memory_scoring::filter_importance;
use crate::recall_tags::{RELATED_EXPANSION_MAX_PER_HIT, RELATED_EXPANSION_MAX_TOTAL};

use super::gather::{Gathered, RecallStrategy};
use super::types::{
    memory_id, num, trace_stage, HydratedHit, RecallCandidate, RecallStage, RecallTrace,
};

/// The row filters (v4 `HydrateFilters`). Every field is optional; absent
/// means "no filter".
#[derive(Clone, Debug, Default)]
pub struct HydrateFilters {
    pub include_cold: bool,
    pub exclude_ids: Option<HashSet<String>>,
    pub min_importance: Option<f64>,
    /// `'AUTO' | 'MANUAL' | 'CONSOLIDATED'`.
    pub source: Option<String>,
    pub about_character_id: Option<String>,
}

/// v4 `filterImportance(memory)` over a row: `reinforcedImportance ??
/// importance` (an absent importance reads NaN, which fails every `<`).
pub fn row_filter_importance(memory: &Value) -> f64 {
    filter_importance(
        memory
            .get("importance")
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN),
        memory.get("reinforcedImportance").and_then(Value::as_f64),
    )
}

/// v4 `passesFilters` — true when a row passes the filters, tested in v4's
/// order: excluded → cold → importance → source → subject.
pub fn passes_filters(memory: &Value, filters: &HydrateFilters) -> bool {
    if filters
        .exclude_ids
        .as_ref()
        .is_some_and(|e| e.contains(memory_id(memory)))
    {
        return false;
    }
    if !filters.include_cold && memory.get("tier").and_then(Value::as_str) == Some("cold") {
        return false;
    }
    if let Some(min) = filters.min_importance {
        if row_filter_importance(memory) < min {
            return false;
        }
    }
    if let Some(source) = &filters.source {
        if memory.get("source").and_then(Value::as_str) != Some(source.as_str()) {
            return false;
        }
    }
    if let Some(about) = &filters.about_character_id {
        if memory.get("aboutCharacterId").and_then(Value::as_str) != Some(about.as_str()) {
            return false;
        }
    }
    true
}

async fn find_by_ids(db: &Db, ids: Vec<String>) -> Result<Vec<Value>, DbError> {
    db.read_main(move |conn| memories_read::find_by_ids(conn, &ids))
}

/// v4 `hydrate` — read the pool's rows and keep the ones that pass, in pool
/// order.
pub async fn hydrate(
    db: &Db,
    gathered: &Gathered,
    filters: &HydrateFilters,
    trace: &mut RecallTrace,
    label: Option<&str>,
) -> Result<Vec<HydratedHit>, DbError> {
    let missing: Vec<String> = gathered
        .hits
        .iter()
        .filter(|h| h.memory.is_none())
        .map(|h| h.id.clone())
        .collect();
    let rows = if missing.is_empty() {
        Vec::new()
    } else {
        find_by_ids(db, missing).await?
    };
    let read = rows.len();
    let by_id: HashMap<String, Value> = rows
        .into_iter()
        .map(|m| (memory_id(&m).to_string(), m))
        .collect();

    let mut out: Vec<HydratedHit> = Vec::new();
    let mut dropped = 0usize;
    for hit in &gathered.hits {
        let memory = match &hit.memory {
            Some(m) => Some(m),
            None => by_id.get(&hit.id),
        };
        let Some(memory) = memory.filter(|m| passes_filters(m, filters)) else {
            dropped += 1;
            continue;
        };
        let literal = match &gathered.literal_phrase {
            Some(phrase) => {
                gathered.literal_hit_ids.contains(memory_id(memory))
                    || contains_literal_phrase(
                        memory.get("content").and_then(Value::as_str),
                        phrase,
                    )
                    || contains_literal_phrase(
                        memory.get("summary").and_then(Value::as_str),
                        phrase,
                    )
            }
            None => false,
        };
        out.push(HydratedHit {
            memory: memory.clone(),
            raw_score: hit.score,
            // v4 `applyLiteralBoost(hit.score)` — its default fraction, 0.5.
            score: if literal {
                apply_literal_boost(hit.score, 0.5)
            } else {
                hit.score
            },
            used_embedding: gathered.strategy == RecallStrategy::Vector,
        });
    }
    let mut detail = Map::new();
    detail.insert("read".into(), num(read as f64));
    detail.insert("kept".into(), num(out.len() as f64));
    detail.insert("dropped".into(), num(dropped as f64));
    trace_stage(trace, RecallStage::Hydrate, detail, label);
    Ok(out)
}

/// v4 `hydrateNeighbours` — the one-hop neighbours of the top `limit` ranked
/// hits that are not already in the pool: at most `maxPerHit` per seed and
/// `maxTotal` in all, read in one batch and filtered like the pool (plus the
/// character scope; the subject filter does NOT apply). `select` scores them.
/// An excluded neighbour is skipped without consuming its seed's quota.
pub async fn hydrate_neighbours(
    db: &Db,
    ranked: &[RecallCandidate],
    limit: usize,
    character_id: &str,
    filters: &HydrateFilters,
) -> Result<Vec<Value>, DbError> {
    let in_pool: HashSet<&str> = ranked.iter().map(|r| r.id()).collect();
    let mut neighbour_ids: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    'seeds: for seed in ranked.iter().take(limit) {
        let mut from_seed = 0usize;
        let related: Vec<String> = seed
            .memory
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        for neighbour_id in related {
            if neighbour_ids.len() >= RELATED_EXPANSION_MAX_TOTAL {
                break;
            }
            if from_seed >= RELATED_EXPANSION_MAX_PER_HIT {
                break;
            }
            if in_pool.contains(neighbour_id.as_str()) || seen.contains(&neighbour_id) {
                continue;
            }
            if filters
                .exclude_ids
                .as_ref()
                .is_some_and(|e| e.contains(&neighbour_id))
            {
                continue;
            }
            seen.insert(neighbour_id.clone());
            neighbour_ids.push(neighbour_id);
            from_seed += 1;
        }
        if neighbour_ids.len() >= RELATED_EXPANSION_MAX_TOTAL {
            break 'seeds;
        }
    }
    if neighbour_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = find_by_ids(db, neighbour_ids).await?;
    let neighbour_filters = HydrateFilters {
        about_character_id: None,
        ..filters.clone()
    };
    Ok(rows
        .into_iter()
        .filter(|m| {
            m.get("characterId").and_then(Value::as_str) == Some(character_id)
                && passes_filters(m, &neighbour_filters)
        })
        .collect())
}
