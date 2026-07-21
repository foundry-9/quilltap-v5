//! Frozen memory archive cache (v4 `lib/memory/frozen-archive-cache.ts`).
//!
//! Per-character memory pool that stays byte-stable across turns within a single
//! `compactionGeneration`. The archive is the cache-friendly bulk of what a
//! character "remembers" generally — the top N memories ranked by effective
//! weight at generation start, then sorted by memory id so the formatted output
//! is deterministic across turns (which is exactly the identical-prefix a
//! provider prefix cache wants to see).
//!
//! Cache key: `(characterId, compactionGeneration)`. Eviction: when a new
//! generation appears for a character, the prior entry for that character is
//! dropped. The cache is a **process-global** map (v4's module-level `Map`);
//! losing it on restart costs one recompute on the next turn. Modeled exactly
//! like [`crate::services::housekeeping_outcome_cache`] — a
//! `OnceLock<Mutex<HashMap>>` static keyed by characterId.
//!
//! Returns the raw memory `Value` rows (v4's `Memory[]`); the caller
//! (build_context) converts them into its own injector type — this module does
//! not depend on any build_context type.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::collation::locale_compare;
use crate::db::memories_read;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::memory_weighting::{calculate_effective_weight, DEFAULT_WEIGHTING_CONFIG};

/// Default size of the frozen archive. v4's Phase-3a target.
pub const FROZEN_ARCHIVE_SIZE: usize = 25;

/// Pool size to draw from when ranking. Higher than the archive size so the
/// effective-weight re-rank has room to surface long-tail high-importance rows
/// that fell below the raw-importance cutoff (v4 `FROZEN_ARCHIVE_POOL_FACTOR`).
const FROZEN_ARCHIVE_POOL_FACTOR: usize = 4;

struct CacheEntry {
    generation: i64,
    memories: Vec<Value>,
}

fn cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Look up (or compute and cache) the frozen memory archive for a character at a
/// given compaction generation. Returns at most `size` memories (default
/// [`FROZEN_ARCHIVE_SIZE`]), sorted ascending by `memory.id` so the formatted
/// output is deterministic across turns.
///
/// `now_ms` is the wall clock the effective-weight re-rank uses (v4's implicit
/// `Date.now()`), injected here so the caller owns the clock.
pub fn get_or_compute_frozen_archive(
    db: &Db,
    character_id: &str,
    compaction_generation: i64,
    size: Option<usize>,
    now_ms: f64,
) -> Result<Vec<Value>, DbError> {
    let size = size.unwrap_or(FROZEN_ARCHIVE_SIZE);

    {
        let guard = cache().lock().expect("frozen archive cache lock");
        if let Some(entry) = guard.get(character_id) {
            if entry.generation == compaction_generation {
                return Ok(entry.memories.clone());
            }
        }
    }

    let memories = compute_frozen_archive(db, character_id, size, now_ms)?;

    cache().lock().expect("frozen archive cache lock").insert(
        character_id.to_string(),
        CacheEntry {
            generation: compaction_generation,
            memories: memories.clone(),
        },
    );

    Ok(memories)
}

/// Drop any cached archive for a character. Useful when the character's memory
/// corpus has been edited from outside the chat-summary pipeline (manual
/// deletion, housekeeping sweep, import). v4 `invalidateFrozenArchive`.
pub fn invalidate_frozen_archive(character_id: &str) {
    cache()
        .lock()
        .expect("frozen archive cache lock")
        .remove(character_id);
}

/// Test-only helper: fully reset the cache. v4 `resetFrozenArchiveCacheForTests`.
pub fn reset_frozen_archive_cache_for_tests() {
    cache().lock().expect("frozen archive cache lock").clear();
}

/// v4 `computeFrozenArchive`: pull the top-N by raw importance, re-rank by
/// effective weight (importance × time decay) descending, slice to the archive
/// size, then sort by id so ordering is stable across turns.
fn compute_frozen_archive(
    db: &Db,
    character_id: &str,
    size: usize,
    now_ms: f64,
) -> Result<Vec<Value>, DbError> {
    // `Math.max(size, size * FROZEN_ARCHIVE_POOL_FACTOR)`.
    let pool_size = size.max(size * FROZEN_ARCHIVE_POOL_FACTOR);

    let candidates = {
        let cid = character_id.to_string();
        db.read_main(move |conn| memories_read::find_most_important(conn, &cid, pool_size as i64))?
    };
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    rank_frozen_archive(candidates, size, now_ms)
}

/// The pure ranking (the tier-1-checkable core, split out so a differential can
/// drive it over a committed corpus of memory rows without a DB): re-rank
/// `candidates` by effective weight descending, take `size`, then sort ascending
/// by `id` via `localeCompare`.
///
/// v4's rank sort is `b.effectiveWeight - a.effectiveWeight` (descending). JS
/// `Array.prototype.sort` is stable; Rust's `sort_by` is stable too, so ties
/// preserve `findMostImportant`'s incoming order identically before the final
/// id sort.
pub(crate) fn rank_frozen_archive(
    candidates: Vec<Value>,
    size: usize,
    now_ms: f64,
) -> Result<Vec<Value>, DbError> {
    let mut ranked: Vec<(f64, Value)> = candidates
        .into_iter()
        .map(|memory| {
            let ew = calculate_effective_weight(
                &memory_inputs(&memory),
                &DEFAULT_WEIGHTING_CONFIG,
                now_ms,
            )
            .effective_weight;
            (ew, memory)
        })
        .collect();

    // Descending by effective weight (v4 `b.effectiveWeight - a.effectiveWeight`).
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(size);

    let mut out: Vec<Value> = ranked.into_iter().map(|(_, m)| m).collect();

    // Ascending by id via `a.id.localeCompare(b.id)`.
    out.sort_by(|a, b| {
        let ai = a.get("id").and_then(Value::as_str).unwrap_or("");
        let bi = b.get("id").and_then(Value::as_str).unwrap_or("");
        locale_compare(ai, bi)
    });

    Ok(out)
}

/// Build [`crate::memory_weighting::MemoryInputs`] from a memory `Value` — the
/// ISO→ms + graph-degree mapping, identical to the memory-service / housekeeping
/// `memory_inputs` reader.
fn memory_inputs(v: &Value) -> crate::memory_weighting::MemoryInputs {
    let num = |k: &str| v.get(k).and_then(Value::as_f64);
    let ms = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .and_then(crate::clock::iso_to_ms)
            .map(|m| m as f64)
    };
    crate::memory_weighting::MemoryInputs {
        importance: num("importance").unwrap_or(0.0),
        reinforced_importance: num("reinforcedImportance"),
        created_at_ms: ms("createdAt").unwrap_or(f64::NAN),
        last_reinforced_at_ms: ms("lastReinforcedAt"),
        last_accessed_at_ms: ms("lastAccessedAt"),
        reinforcement_count: num("reinforcementCount").map(|c| c as u64),
        graph_degree: v
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0),
        // Episodic spine (v4 8bf3cb5f): the declared kind + the event clock.
        kind_episodic: v.get("kind").and_then(Value::as_str) == Some("episodic"),
        occurred_at_ms: crate::episodic::event_time_ms(v.get("occurredAt").and_then(Value::as_str)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // A fixed reference clock (UTC 2026-06-27T12:00:00Z), matching the oracle.
    const NOW_MS: f64 = 1_782_993_600_000.0;

    fn mem(id: &str, importance: f64, created_at: &str) -> Value {
        json!({ "id": id, "importance": importance, "createdAt": created_at })
    }

    #[test]
    fn ranks_by_effective_weight_then_sorts_by_id() {
        // Three memories with distinct effective weights (all fresh → decay ~1,
        // so effective weight ~ importance). Ranked DESC by weight, sliced to 2,
        // then re-sorted ASC by id via localeCompare.
        let candidates = vec![
            mem("bbb", 0.9, "2026-06-27T00:00:00.000Z"), // highest weight
            mem("aaa", 0.3, "2026-06-27T00:00:00.000Z"), // lowest weight → dropped by slice(2)
            mem("ccc", 0.6, "2026-06-27T00:00:00.000Z"), // middle weight
        ];
        let ranked = rank_frozen_archive(candidates, 2, NOW_MS).unwrap();
        // Top-2 by weight are bbb (0.9) and ccc (0.6); aaa (0.3) is dropped.
        // Final ascending id sort → [bbb, ccc].
        let ids: Vec<&str> = ranked
            .iter()
            .map(|m| m.get("id").and_then(Value::as_str).unwrap())
            .collect();
        assert_eq!(ids, vec!["bbb", "ccc"]);
    }

    #[test]
    fn final_sort_uses_locale_compare_lowercase_before_uppercase() {
        // localeCompare (en-US tertiary) orders lowercase before uppercase:
        // a < A. A code-unit sort would put 'A' (0x41) before 'a' (0x61).
        let candidates = vec![
            mem("A", 0.5, "2026-06-27T00:00:00.000Z"),
            mem("a", 0.5, "2026-06-27T00:00:00.000Z"),
        ];
        let ranked = rank_frozen_archive(candidates, 25, NOW_MS).unwrap();
        let ids: Vec<&str> = ranked
            .iter()
            .map(|m| m.get("id").and_then(Value::as_str).unwrap())
            .collect();
        assert_eq!(ids, vec!["a", "A"], "localeCompare puts lowercase first");
    }

    #[test]
    fn empty_candidates_yield_empty() {
        assert!(rank_frozen_archive(Vec::new(), 25, NOW_MS)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cache_returns_same_generation_and_evicts_on_new_generation() {
        // Drive the cache directly (no DB) via a manual insert, since the public
        // entry point needs a Db to compute a miss. This tests the generation
        // key / eviction logic in isolation.
        reset_frozen_archive_cache_for_tests();
        let cid = "frozen-cache-test-char";

        cache().lock().unwrap().insert(
            cid.to_string(),
            CacheEntry {
                generation: 1,
                memories: vec![mem("m1", 0.5, "2026-06-27T00:00:00.000Z")],
            },
        );

        // Same generation → cached hit.
        {
            let guard = cache().lock().unwrap();
            let entry = guard.get(cid).unwrap();
            assert_eq!(entry.generation, 1);
            assert_eq!(entry.memories.len(), 1);
        }

        // A new generation replaces the prior entry (eviction).
        cache().lock().unwrap().insert(
            cid.to_string(),
            CacheEntry {
                generation: 2,
                memories: Vec::new(),
            },
        );
        {
            let guard = cache().lock().unwrap();
            let entry = guard.get(cid).unwrap();
            assert_eq!(entry.generation, 2, "new generation replaces old");
            assert!(entry.memories.is_empty());
        }

        // Invalidate drops the entry entirely.
        invalidate_frozen_archive(cid);
        assert!(cache().lock().unwrap().get(cid).is_none());

        reset_frozen_archive_cache_for_tests();
    }
}
