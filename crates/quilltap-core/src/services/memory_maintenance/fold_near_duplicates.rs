//! Phase 1 — fold near-duplicates: v4
//! `lib/memory/maintenance/fold-near-duplicates.ts` (`104e986c7`, at the
//! `83334b9a5` pin).
//!
//! v4's why, carried: "Deterministic and LLM-free. Two rows fold when they
//! are both hot AUTO rows with an embedding, about the same subject, of the
//! same kind, of embeddings the same length, on the same occasion (the
//! admission date guard), and at least `nearDuplicate` (0.90) apart by
//! cosine. MANUAL rows, digests and cold rows are never touched (bug 201,
//! decision 4). Grouping is survivor-first rather than transitive: rows are
//! visited by `protection` (the worth-of-keeping score) descending, and each
//! unclaimed row claims every later unclaimed row that is a near-duplicate *of
//! it*. A chain A≈B≈C where A and C are not near-duplicates cannot fold C into
//! A." It replaces the old dedup tool (`services/memory_dedup.rs` — its
//! transitive O(n²) scan, its own survivor score and its 0.80 default).
//!
//! The writes run under admission's per-character lock
//! ([`with_admission_lock`]) "so a fold and an admission never interleave on
//! one row, and the survivor's count, footnotes and links land as deltas
//! through `applyFold` (the one combine)".
//!
//! NO-PORT (recorded): v4's job-child throw in `applyNearDuplicateFolds`, the
//! host-RPC `foldNearDuplicates` hop and `notifyChild('vectorStore')` (+ its
//! DEBUG) — v5 runs one process; the `YIELD_EVERY` event-loop yields —
//! scheduling.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use super::corpus::MaintenanceCorpus;
use super::demote::{created_ms, protection_of};
use super::types::NearDuplicateGroup;
use crate::admission::decide::occasions_are_distinct;
use crate::admission::fold::{fold_into, FoldSource, FOLD_CAPS};
use crate::admission::thresholds::AdmissionThresholds;
use crate::db::runtime::Db;
use crate::db::{memories_read, DbError};
use crate::jsnum::math_min;
use crate::memory_scoring::js_sort::js_array_sort_by;
use crate::memory_scoring::reinforced_importance;
use crate::model::embedding::EmbeddingProvider;
use crate::policy_protection::policy_protection_of;
use crate::services::admission::{apply_fold, with_admission_lock};
use crate::services::memory_retire::{retire_and_announce, RetireOptions, RetireReason};

const MODULE: &str = "memory:maintenance:fold";

/// v4 `FOLD_MAX_BUCKET_ROWS`: "most rows compared per bucket in one run (the
/// comparison is quadratic). A bucket past this compares its newest rows —
/// where new repetitions arrive — and the rest wait for a later run."
pub const FOLD_MAX_BUCKET_ROWS: usize = 2000;

fn str_of<'a>(m: &'a Value, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str)
}

fn id_of(m: &Value) -> String {
    str_of(m, "id").unwrap_or_default().to_string()
}

/// The row's `Float32Array` (v5's `{"0": v, …}` object) as f32s in index
/// order; `None` when absent, not an object, or empty.
pub fn embedding_of(memory: &Value) -> Option<Vec<f32>> {
    let obj = memory.get("embedding")?.as_object()?;
    if obj.is_empty() {
        return None;
    }
    let mut out = vec![0f32; obj.len()];
    for (k, v) in obj {
        let i: usize = k.parse().ok()?;
        *out.get_mut(i)? = v.as_f64()? as f32;
    }
    Some(out)
}

/// v4 `isFoldEligible(m)`: "a row the phase may fold: hot, AUTO (so never a
/// digest or a MANUAL row), with a vector" — `policyProtectionOf(m) === null
/// && source === 'AUTO' && embedding instanceof Float32Array && length > 0`.
pub fn is_fold_eligible(memory: &Value) -> bool {
    policy_protection_of(str_of(memory, "source"), str_of(memory, "tier")).is_none()
        && str_of(memory, "source") == Some("AUTO")
        && memory
            .get("embedding")
            .and_then(Value::as_object)
            .is_some_and(|o| !o.is_empty())
}

/// v4 `bucketKey(m)`: `` `${aboutCharacterId || characterId}|${kind ??
/// 'semantic'}|${embedding.length}` `` — an EMPTY `aboutCharacterId` reads as
/// the holder (`||`); a null kind reads `semantic` (`??`).
fn bucket_key(memory: &Value, dims: usize) -> String {
    let subject = match str_of(memory, "aboutCharacterId") {
        Some(about) if !about.is_empty() => about,
        _ => str_of(memory, "characterId").unwrap_or_default(),
    };
    let kind = match memory.get("kind") {
        None | Some(Value::Null) => "semantic".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    format!("{subject}|{kind}|{dims}")
}

/// v4 `planNearDuplicateFolds(rows, thresholds, nowMs)` → `{eligible,
/// deferred, groups}`. Pure.
pub struct PlannedFolds {
    pub eligible: usize,
    pub deferred: usize,
    pub groups: Vec<NearDuplicateGroup>,
}

/// One fold bucket: its key and its rows, each with its decoded vector.
type FoldBucket<'a> = (String, Vec<(&'a Value, Vec<f32>)>);

/// Work out the near-duplicate groups of a corpus (v4
/// `planNearDuplicateFolds`).
pub fn plan_near_duplicate_folds(
    rows: &[&Value],
    thresholds: &AdmissionThresholds,
    now_ms: f64,
) -> PlannedFolds {
    // Buckets in first-seen order; each row with its decoded vector.
    let mut buckets: Vec<FoldBucket<'_>> = Vec::new();
    let mut eligible = 0usize;
    for row in rows {
        if !is_fold_eligible(row) {
            continue;
        }
        let Some(embedding) = embedding_of(row) else {
            continue;
        };
        eligible += 1;
        let key = bucket_key(row, embedding.len());
        match buckets.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push((*row, embedding)),
            None => buckets.push((key, vec![(*row, embedding)])),
        }
    }

    let mut groups: Vec<NearDuplicateGroup> = Vec::new();
    let mut deferred = 0usize;
    for (_, all) in buckets {
        if all.len() < 2 {
            continue;
        }
        let mut list = all;
        if list.len() > FOLD_MAX_BUCKET_ROWS {
            let total = list.len();
            js_array_sort_by(&mut list, |a, b| created_ms(b.0) - created_ms(a.0));
            list.truncate(FOLD_MAX_BUCKET_ROWS);
            deferred += total - list.len();
        }
        // "Strongest first, so the row most worth keeping survives; ties to
        // the oldest."
        let mut ordered: Vec<(&Value, Vec<f32>, f64)> = list
            .into_iter()
            .map(|(m, e)| {
                let score = protection_of(m, now_ms).score.score;
                (m, e, score)
            })
            .collect();
        js_array_sort_by(&mut ordered, |a, b| {
            let d = b.2 - a.2;
            if d != 0.0 && !d.is_nan() {
                d
            } else {
                created_ms(a.0) - created_ms(b.0)
            }
        });
        let mut claimed: HashSet<String> = HashSet::new();
        for i in 0..ordered.len() {
            let survivor_id = id_of(ordered[i].0);
            if claimed.contains(&survivor_id) {
                continue;
            }
            let mut absorbed_ids: Vec<String> = Vec::new();
            let mut min_cosine = 1.0f64;
            for j in (i + 1)..ordered.len() {
                let other_id = id_of(ordered[j].0);
                if claimed.contains(&other_id) {
                    continue;
                }
                if occasions_are_distinct(
                    str_of(ordered[i].0, "occurredAt"),
                    str_of(ordered[j].0, "occurredAt"),
                    thresholds.date_guard_days,
                ) {
                    continue;
                }
                // Same bucket ⇒ same length: the dimension guard never fires.
                let cosine =
                    crate::embedding_vector::cosine_similarity(&ordered[i].1, &ordered[j].1)
                        .unwrap_or(f64::NAN);
                // JS `cosine < threshold` — a NaN cosine is NOT below it.
                if cosine < thresholds.near_duplicate {
                    continue;
                }
                absorbed_ids.push(other_id);
                min_cosine = math_min(min_cosine, cosine);
            }
            if !absorbed_ids.is_empty() {
                claimed.insert(survivor_id.clone());
                for id in &absorbed_ids {
                    claimed.insert(id.clone());
                }
                groups.push(NearDuplicateGroup {
                    survivor_id,
                    absorbed_ids,
                    min_cosine,
                });
            }
        }
    }
    PlannedFolds {
        eligible,
        deferred,
        groups,
    }
}

/// v4 `AppliedNearDuplicateFolds` — what the apply did with each group.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppliedNearDuplicateFolds {
    /// Survivors as they now stand, with their vectors, one per group that
    /// landed.
    pub survivors: Vec<Value>,
    /// Rows retired (folded into a survivor).
    pub retired_ids: Vec<String>,
    pub groups_failed: usize,
    /// Groups not attempted because the deadline passed.
    pub groups_deferred: usize,
}

/// v4 `ApplyNearDuplicateFoldsRequest`, plus the run's clock: `deadline`
/// answers v4's `Date.now() >= deadlineAt` (`None` → no deadline).
pub struct ApplyNearDuplicateFoldsRequest<'a> {
    pub character_id: &'a str,
    pub groups: &'a [NearDuplicateGroup],
    pub user_id: &'a str,
    pub deadline_passed: Option<&'a (dyn Fn() -> bool + Send + Sync)>,
    pub host_zone_fallback: &'a str,
}

/// v4 `foldOneGroup` — under admission's lock: re-read the group's rows (the
/// holder's only), fold, retire the absorbed rows. `Ok(None)` when the group
/// no longer applies or its fold did not land.
async fn fold_one_group<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: Option<&P>,
    character_id: &str,
    group: &NearDuplicateGroup,
    user_id: &str,
    host_zone_fallback: &str,
) -> Result<Option<(Value, Vec<String>)>, DbError> {
    with_admission_lock(character_id, async {
        let mut ids = vec![group.survivor_id.clone()];
        ids.extend(group.absorbed_ids.iter().cloned());
        let read = db.read_main(|c| memories_read::find_by_ids(c, &ids))?;
        let rows: Map<String, Value> = read
            .into_iter()
            .filter(|m| str_of(m, "characterId") == Some(character_id))
            .map(|m| (id_of(&m), m))
            .collect();
        let survivor = rows.get(&group.survivor_id);
        let absorbed: Vec<&Value> = group
            .absorbed_ids
            .iter()
            .filter_map(|id| rows.get(id))
            .filter(|m| is_fold_eligible(m))
            .collect();
        let Some(survivor) = survivor.filter(|s| is_fold_eligible(s) && !absorbed.is_empty())
        else {
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                survivorId = %group.survivor_id,
                survivorPresent = rows.contains_key(&group.survivor_id),
                absorbedStill = absorbed.len(),
                "Near-duplicate group no longer applies; skipped"
            );
            return Ok(None);
        };
        let sources: Vec<FoldSource> = absorbed.iter().map(|m| FoldSource::from_value(m)).collect();
        let plan = fold_into(survivor, &sources, &FOLD_CAPS);
        let applied = match apply_fold(
            db,
            provider,
            survivor,
            &plan,
            Some(user_id),
            host_zone_fallback,
        )
        .await
        {
            Ok(applied) => applied,
            Err(error) => {
                tracing::warn!(
                    module = MODULE,
                    characterId = %character_id,
                    survivorId = %group.survivor_id,
                    error = %crate::db::fallback::error_text(&error),
                    "Near-duplicate fold failed; its rows are kept"
                );
                None
            }
        };
        let Some(applied) = applied else {
            return Ok(None);
        };
        // "Inside the lock, so no admission can reinforce an absorbed row
        // between the fold and its retirement. The chokepoint scrubs every
        // link to the folded rows (the survivor's new ones included) and
        // evicts their vectors." Its effects run after the commit (R-AF).
        let report = retire_and_announce(
            db,
            absorbed.iter().map(|m| id_of(m)).collect(),
            RetireOptions::new(RetireReason::NearDuplicateFold),
        )
        .await?;
        Ok(Some((applied, report.retired_ids)))
    })
    .await
}

/// v4 `applyNearDuplicateFolds(request)`: "apply planned folds, one group at a
/// time. Each group re-reads its rows first: a row gone, gone cold, or no
/// longer AUTO since planning is left out, and a group whose survivor is no
/// longer eligible is skipped whole. Absorbed rows are retired only after
/// their survivor has taken them in. Past `deadlineAt` no further group is
/// started; the rest are deferred."
pub async fn apply_near_duplicate_folds<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: Option<&P>,
    request: ApplyNearDuplicateFoldsRequest<'_>,
) -> Result<AppliedNearDuplicateFolds, DbError> {
    let mut result = AppliedNearDuplicateFolds::default();
    let groups = request.groups;
    if groups.is_empty() {
        return Ok(result);
    }
    for (i, group) in groups.iter().enumerate() {
        if request.deadline_passed.is_some_and(|passed| passed()) {
            result.groups_deferred = groups.len() - i;
            tracing::info!(
                module = MODULE,
                characterId = %request.character_id,
                deferred = result.groups_deferred,
                "Near-duplicate fold budget spent; the remaining groups wait for the next run"
            );
            break;
        }
        match fold_one_group(
            db,
            provider,
            request.character_id,
            group,
            request.user_id,
            request.host_zone_fallback,
        )
        .await?
        {
            None => result.groups_failed += 1,
            Some((survivor, retired)) => {
                result.survivors.push(survivor);
                result.retired_ids.extend(retired);
            }
        }
    }
    tracing::debug!(
        module = MODULE,
        characterId = %request.character_id,
        groups = groups.len(),
        landed = result.survivors.len(),
        failed = result.groups_failed,
        deferred = result.groups_deferred,
        retired = result.retired_ids.len(),
        "Near-duplicate folds applied"
    );
    Ok(result)
}

/// v4 `applyFoldsToCorpus(corpus, groups, applied)`: "bring the run's corpus
/// in step with the folds: live, from what the parent wrote; dry, from the
/// plan (the survivor's text and count as the fold would leave them; its
/// vector unchanged)."
pub fn apply_folds_to_corpus(
    corpus: &mut MaintenanceCorpus,
    groups: &[NearDuplicateGroup],
    applied: Option<&AppliedNearDuplicateFolds>,
) {
    if let Some(applied) = applied {
        for survivor in &applied.survivors {
            // "`applyFold` returns the row with its vector (the new one after
            // a re-embed)."
            let has_vector = survivor
                .get("embedding")
                .is_some_and(|e| !e.is_null() && e != &Value::Bool(false));
            if has_vector {
                corpus.put(survivor.clone());
            } else {
                let mut row = survivor.clone();
                let kept = corpus
                    .get(&id_of(survivor))
                    .and_then(|m| m.get("embedding"))
                    .cloned()
                    .unwrap_or(Value::Null);
                if let Value::Object(obj) = &mut row {
                    obj.insert("embedding".into(), kept);
                }
                corpus.put(row);
            }
        }
        corpus.remove(&applied.retired_ids);
        return;
    }
    for group in groups {
        let Some(survivor) = corpus.get(&group.survivor_id).cloned() else {
            continue;
        };
        let sources: Vec<FoldSource> = group
            .absorbed_ids
            .iter()
            .filter_map(|id| corpus.get(id))
            .map(FoldSource::from_value)
            .collect();
        let plan = fold_into(&survivor, &sources, &FOLD_CAPS);
        let count = match survivor.get("reinforcementCount") {
            None | Some(Value::Null) => 1.0,
            Some(v) => v.as_f64().unwrap_or(f64::NAN),
        };
        let reinforcement_count = count + plan.reinforcement_delta;
        let importance = survivor
            .get("importance")
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN);
        let mut row = match survivor.clone() {
            Value::Object(o) => o,
            _ => Map::new(),
        };
        if let Value::Object(patch) = serde_json::to_value(&plan.patch).unwrap_or(json!({})) {
            for (k, v) in patch {
                row.insert(k, v);
            }
        }
        row.insert("content".into(), Value::String(plan.content.clone()));
        let mut links: Vec<Value> = survivor
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        links.extend(plan.add_links.iter().map(|l| Value::String(l.clone())));
        row.insert("relatedMemoryIds".into(), Value::Array(links));
        row.insert(
            "reinforcementCount".into(),
            crate::db::js_number_to_json(reinforcement_count),
        );
        row.insert(
            "reinforcedImportance".into(),
            crate::db::js_number_to_json(reinforced_importance(importance, reinforcement_count)),
        );
        if let Some(at) = &plan.reinforced_at {
            row.insert("lastReinforcedAt".into(), Value::String(at.clone()));
        }
        corpus.put(Value::Object(row));
        corpus.remove(&group.absorbed_ids);
    }
}
