//! Phase 2 — consolidate: v4 `lib/memory/maintenance/consolidate.ts`
//! (`104e986c7`, at the `83334b9a5` pin).
//!
//! v4's why, carried: "The loader half of consolidation: picks the candidates
//! out of the run's corpus, buckets them by subject, and hands them to the
//! planner and model route in `consolidation.ts`. Candidates are hot AUTO rows
//! **and cold rows demoted by cap pressure** (`supersededById` null, bug 211),
//! so demotion feeds consolidation instead of stranding rows. MANUAL rows are
//! never candidates."

use serde_json::{json, Map, Value};

use super::corpus::{is_hot, MaintenanceCorpus};
use crate::consolidation_plan::{ConsolidationWritePlan, LinkIndexRow, OrderedMap};
use crate::consolidation_tasks::ConsolidationBucketKind;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::episodic::js_date_parse_ms;
use crate::memory_scoring::js_sort::js_array_sort_by;
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::consolidation::{
    consolidate, has_embedding, ConsolidationBucket, ConsolidationBucketRef, ConsolidationInput,
    ConsolidationRun, ConsolidationStats, RunConsolidationOptions,
};

const MODULE: &str = "memory:maintenance:consolidate";

/// v4 `CONSOLIDATION_MAX_BUCKET_ROWS` (moved here from `consolidation.ts`):
/// "most candidate rows clustered per subject bucket in one run. Clustering is
/// O(n²) in memory and time; a bucket past this keeps its never-considered
/// rows first, then the weightiest, and the rest wait for the next run."
pub const CONSOLIDATION_MAX_BUCKET_ROWS: usize = 2000;

const DAY_MS: f64 = 86_400_000.0;

fn str_of<'a>(m: &'a Value, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str)
}

/// v4 `bucketKeyOf(memory, holderId)`: no `aboutCharacterId` ⇒ `none` (`??
/// null` then `!about` — an EMPTY id is none too), the holder ⇒ `self`, else
/// `other:<id>`.
fn bucket_key_of(memory: &Value, holder_id: &str) -> String {
    match str_of(memory, "aboutCharacterId").filter(|s| !s.is_empty()) {
        None => "none".to_string(),
        Some(about) if about == holder_id => "self".to_string(),
        Some(about) => format!("other:{about}"),
    }
}

/// JS truthiness of `consolidatedAt` (a non-empty string).
fn considered(m: &Value) -> bool {
    match m.get("consolidatedAt") {
        Some(Value::String(s)) => !s.is_empty(),
        None | Some(Value::Null) => false,
        Some(_) => true,
    }
}

/// v4 `capBucketRows`: over the cap, never-considered first, then
/// `reinforcedImportance` DESC (`?? 0`); stable.
fn cap_bucket_rows(rows: Vec<Value>) -> Vec<Value> {
    if rows.len() <= CONSOLIDATION_MAX_BUCKET_ROWS {
        return rows;
    }
    let mut sorted = rows;
    js_array_sort_by(&mut sorted, |a, b| {
        let (ac, bc) = (
            f64::from(u8::from(considered(a))),
            f64::from(u8::from(considered(b))),
        );
        if ac != bc {
            return ac - bc;
        }
        let ri = |m: &Value| match m.get("reinforcedImportance") {
            None | Some(Value::Null) => 0.0,
            Some(v) => v.as_f64().unwrap_or(f64::NAN),
        };
        ri(b) - ri(a)
    });
    sorted.truncate(CONSOLIDATION_MAX_BUCKET_ROWS);
    sorted
}

/// The loader's output (v4 `{rowIndex, buckets, stats}`).
pub struct BucketedCandidates {
    pub row_index: OrderedMap<LinkIndexRow>,
    pub buckets: Vec<ConsolidationBucket>,
    pub stats: ConsolidationStats,
}

/// v4 `bucketCandidates(rows, holderId, matureCutoffMs)`: "bucket the corpus
/// for consolidation: the link index over every row, and per subject the
/// candidate rows and the hot digests that seed clusters. Pure."
pub fn bucket_candidates(
    rows: &[&Value],
    holder_id: &str,
    mature_cutoff_ms: f64,
) -> BucketedCandidates {
    let mut stats = ConsolidationStats::default();
    let mut row_index: OrderedMap<LinkIndexRow> = OrderedMap::new();
    let mut buckets: Vec<ConsolidationBucket> = Vec::new();

    fn bucket_for<'b>(
        buckets: &'b mut Vec<ConsolidationBucket>,
        memory: &Value,
        holder_id: &str,
    ) -> &'b mut ConsolidationBucket {
        let key = bucket_key_of(memory, holder_id);
        if let Some(i) = buckets.iter().position(|b| b.key == key) {
            return &mut buckets[i];
        }
        let about = str_of(memory, "aboutCharacterId").map(str::to_string);
        let kind = match key.as_str() {
            "self" => ConsolidationBucketKind::SelfBucket,
            "none" => ConsolidationBucketKind::None,
            _ => ConsolidationBucketKind::Other,
        };
        let about = if key == "none" { None } else { about };
        buckets.push(ConsolidationBucket {
            key,
            reference: ConsolidationBucketRef {
                kind,
                subject_character_id: about.clone(),
                subject_name: String::new(),
            },
            about_character_id: about,
            rows: Vec::new(),
            digests: Vec::new(),
        });
        buckets.last_mut().expect("just pushed")
    }

    for memory in rows {
        stats.rows_loaded += 1;
        let id = str_of(memory, "id").unwrap_or_default().to_string();
        let related: Vec<String> = memory
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        row_index.set(
            &id,
            LinkIndexRow {
                id: id.clone(),
                related_memory_ids: related,
            },
        );
        let hot = is_hot(memory);
        let source = str_of(memory, "source");

        if source == Some("CONSOLIDATED") {
            if !hot {
                continue;
            }
            if !has_embedding(memory) {
                stats.no_embedding_skipped += 1;
                continue;
            }
            bucket_for(&mut buckets, memory, holder_id)
                .digests
                .push((*memory).clone());
            stats.digests_loaded += 1;
            continue;
        }
        // "Hand-written rows are never sent cold by policy."
        if source != Some("AUTO") {
            continue;
        }
        // "Cold rows a digest superseded belong to it; cold rows with no
        // successor were demoted by the cap and re-enter here (bug 211)."
        if !hot {
            if str_of(memory, "supersededById").is_some_and(|s| !s.is_empty()) {
                continue;
            }
            stats.demoted_candidates += 1;
        }
        let created = str_of(memory, "createdAt").and_then(js_date_parse_ms);
        if created.is_none_or(|c| c as f64 > mature_cutoff_ms) {
            stats.immature_skipped += 1;
            continue;
        }
        if !has_embedding(memory) {
            stats.no_embedding_skipped += 1;
            continue;
        }
        bucket_for(&mut buckets, memory, holder_id)
            .rows
            .push((*memory).clone());
        stats.candidates += 1;
    }

    for bucket in buckets.iter_mut() {
        let before = bucket.rows.len();
        let capped = cap_bucket_rows(std::mem::take(&mut bucket.rows));
        stats.bucket_rows_capped += before - capped.len();
        bucket.rows = capped;
    }
    BucketedCandidates {
        row_index,
        buckets,
        stats,
    }
}

/// v4 `runConsolidatePhase(corpus, holder, opts & {matureAfterDays, now})`:
/// "run the phase over the corpus, then bring the corpus in step with the
/// plan" — DEBUG `Consolidation candidates bucketed {module, characterId,
/// candidates, demotedCandidates, digests, buckets}`.
#[allow(clippy::too_many_arguments)]
pub async fn run_consolidate_phase<C: CompletionProvider, E: EmbeddingProvider>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    corpus: &mut MaintenanceCorpus,
    holder: &Value,
    mature_after_days: f64,
    opts: RunConsolidationOptions,
) -> Result<ConsolidationRun, DbError> {
    let now_ms = opts.now_ms.unwrap_or_else(crate::clock::now_unix_ms);
    let mature_cutoff_ms = now_ms as f64 - mature_after_days * DAY_MS;
    let holder_id = str_of(holder, "id").unwrap_or_default().to_string();
    let bucketed = bucket_candidates(&corpus.all(), &holder_id, mature_cutoff_ms);
    tracing::debug!(
        module = MODULE,
        characterId = %holder_id,
        candidates = bucketed.stats.candidates,
        demotedCandidates = bucketed.stats.demoted_candidates,
        digests = bucketed.stats.digests_loaded,
        buckets = bucketed.buckets.len(),
        "Consolidation candidates bucketed"
    );
    let run = consolidate(
        db,
        completion,
        embedding,
        executor,
        ConsolidationInput {
            holder: holder.clone(),
            row_index: bucketed.row_index,
            buckets: bucketed.buckets,
            stats: bucketed.stats,
        },
        opts,
    )
    .await?;
    apply_consolidation_to_corpus(
        corpus,
        &run.plan,
        &run.below_min_considered,
        &crate::clock::iso_from_unix_ms(now_ms),
    );
    Ok(run)
}

fn object(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// v4 `applyConsolidationToCorpus(corpus, plan, belowMinConsidered, nowIso)`:
/// "apply a consolidation plan to the corpus as the writes would leave it".
pub fn apply_consolidation_to_corpus(
    corpus: &mut MaintenanceCorpus,
    plan: &ConsolidationWritePlan,
    below_min_considered: &[String],
    now_iso: &str,
) {
    for create in &plan.creates {
        let mut row = match create.data_json() {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        row.insert("id".into(), json!(create.digest.id));
        row.insert(
            "embedding".into(),
            create
                .digest
                .embedding
                .as_ref()
                .map(|e| {
                    Value::Object(
                        e.iter()
                            .enumerate()
                            .map(|(i, f)| (i.to_string(), crate::db::js_number_to_json(*f as f64)))
                            .collect(),
                    )
                })
                .unwrap_or(Value::Null),
        );
        row.insert("tier".into(), json!("hot"));
        row.insert("createdAt".into(), json!(now_iso));
        row.insert("updatedAt".into(), json!(now_iso));
        corpus.put(Value::Object(row));
    }
    for update in &plan.updates {
        let mut patch = match &update.patch_json {
            Value::Object(m) => m.clone(),
            _ => Map::new(),
        };
        if let Some(e) = &update.digest.embedding {
            patch.insert(
                "embedding".into(),
                Value::Object(
                    e.iter()
                        .enumerate()
                        .map(|(i, f)| (i.to_string(), crate::db::js_number_to_json(*f as f64)))
                        .collect(),
                ),
            );
        }
        corpus.patch(&update.digest.id, &patch);
    }
    for mv in &plan.tier_moves {
        let patch = object([
            ("tier", json!("cold")),
            ("supersededById", json!(mv.superseded_by_id)),
            ("supersededAt", json!(now_iso)),
            ("consolidatedAt", json!(now_iso)),
        ]);
        for id in &mv.ids {
            corpus.patch(id, &patch);
        }
    }
    let stamp = object([("consolidatedAt", json!(now_iso))]);
    let mut seen: Vec<&String> = Vec::new();
    for id in plan.considered.iter().chain(below_min_considered.iter()) {
        if !seen.contains(&id) {
            seen.push(id);
            corpus.patch(id, &stamp);
        }
    }
    for rewrite in &plan.link_rewrites {
        corpus.patch(
            &rewrite.id,
            &object([("relatedMemoryIds", json!(rewrite.related_memory_ids))]),
        );
    }
}
