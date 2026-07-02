//! The **memory housekeeping** service — v4 `lib/memory/housekeeping.ts`
//! (`runHousekeeping` / `getHousekeepingPreview` / `needsHousekeeping`): the
//! retention sweep the `MEMORY_HOUSEKEEPING` job runs (v4's original reason for
//! the child-process writer).
//!
//! Three passes over a character's memories, then one gated apply:
//!
//! 1. **Retention policy** — every memory is scored by the blended protection
//!    gate (`source == "MANUAL"` is a hard override; otherwise
//!    [`calculate_protection_score`] `>= PROTECTION_THRESHOLD`), and an
//!    unprotected memory is deleted only when it is BOTH below the importance
//!    floor AND old AND inactive (no `lastAccessedAt`, or inactive past the
//!    window).
//! 2. **Similarity merge** (opt-in `merge_similar`) — searches the character's
//!    stored vector index against itself (no model call: the embeddings are
//!    already persisted) and folds near-duplicates (`>= merge_threshold`) into
//!    the more-important / newer survivor. Note the merge pass does NOT consult
//!    the protection gate — faithful to v4.
//! 3. **Cap enforcement** — if still over `max_memories`, deletes the
//!    lowest-effective-weight unprotected memories from the tail. Skipped
//!    entirely when every remaining memory is protected (v4's cheap pre-check).
//!
//! The apply (skipped on `dry_run`) deletes through the chokepoint
//! (`delete_many_with_unlink`, so neighbours' `relatedMemoryIds` are scrubbed)
//! and then removes the deleted ids from the vector store (non-fatal, like v4's
//! try/catch). Reasons are formatted with the ported JS `toFixed`
//! ([`crate::jsnum::to_fixed`]) so the detail strings match v4 byte-for-byte at
//! equal wall-clock.
//!
//! v4's event-loop yields (`setImmediate` every 250-row batch / 500 items) are
//! Node scheduling concerns with no DB effect; the batched read shape
//! ([`memories_read::find_by_character_id_in_batches`], `ORDER BY id ASC`) is
//! kept because it fixes the memory walk order both sides share.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::vector_store::CharacterVectorStore;
use crate::db::{memories_read, DbError};
use crate::jsnum::to_fixed;
use crate::memory_weighting::{
    calculate_effective_weight, calculate_protection_score, MemoryInputs,
    DEFAULT_PROTECTION_CONFIG, DEFAULT_WEIGHTING_CONFIG,
};

/// Protection score below which a memory is a deletion candidate.
pub const PROTECTION_THRESHOLD: f64 = 0.5;

/// Page size for the batched memory load (v4 `LOAD_BATCH_SIZE`).
const LOAD_BATCH_SIZE: i64 = 250;

/// Milliseconds per "month" in v4's age arithmetic (30 days).
const MS_PER_MONTH: f64 = 1000.0 * 60.0 * 60.0 * 24.0 * 30.0;

/// Housekeeping options (v4 `HousekeepingOptions`); `None` takes the v4 default.
#[derive(Debug, Clone, Default)]
pub struct HousekeepingOptions {
    /// Maximum number of memories to keep (default 2000).
    pub max_memories: Option<usize>,
    /// Delete unimportant memories older than this many months (default 6).
    pub max_age_months: Option<f64>,
    /// ... and not accessed in this many months (default 6).
    pub max_inactive_months: Option<f64>,
    /// Importance floor below which old/inactive memories go (default 0.3).
    pub min_importance: Option<f64>,
    /// Merge semantically similar memories (default false).
    pub merge_similar: Option<bool>,
    /// Similarity threshold for merging (default 0.9).
    pub merge_threshold: Option<f64>,
    /// Preview without applying (default false).
    pub dry_run: Option<bool>,
}

/// The resolved defaults (v4 `DEFAULT_OPTIONS`).
struct ResolvedOptions {
    max_memories: usize,
    max_age_months: f64,
    max_inactive_months: f64,
    min_importance: f64,
    merge_similar: bool,
    merge_threshold: f64,
    dry_run: bool,
}

impl HousekeepingOptions {
    fn resolve(&self) -> ResolvedOptions {
        ResolvedOptions {
            max_memories: self.max_memories.unwrap_or(2000),
            max_age_months: self.max_age_months.unwrap_or(6.0),
            max_inactive_months: self.max_inactive_months.unwrap_or(6.0),
            min_importance: self.min_importance.unwrap_or(0.3),
            merge_similar: self.merge_similar.unwrap_or(false),
            merge_threshold: self.merge_threshold.unwrap_or(0.9),
            dry_run: self.dry_run.unwrap_or(false),
        }
    }
}

/// One housekeeping action record (v4 `HousekeepingDetail`).
#[derive(Debug, Clone, PartialEq)]
pub struct HousekeepingDetail {
    pub memory_id: String,
    /// `"deleted"` / `"merged"` / `"kept"`.
    pub action: &'static str,
    pub reason: String,
    pub summary: Option<String>,
}

/// The sweep's result (v4 `HousekeepingResult`).
#[derive(Debug, Clone, Default)]
pub struct HousekeepingResult {
    pub deleted: i64,
    pub merged: usize,
    pub kept: i64,
    pub total_before: usize,
    pub total_after: i64,
    /// The effective cap used for this sweep.
    pub cap_used: usize,
    pub deleted_ids: Vec<String>,
    /// Source memories that were merged into others.
    pub merged_ids: Vec<String>,
    pub details: Vec<HousekeepingDetail>,
}

/// The per-memory view the passes read, pre-parsed once from the net JSON.
struct Mem {
    id: String,
    summary: Option<String>,
    source: String,
    importance: f64,
    created_at_ms: f64,
    inputs: MemoryInputs,
    last_accessed_at_ms: Option<f64>,
}

fn parse_mem(v: &Value) -> Mem {
    let str_of = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let num_of = |k: &str| v.get(k).and_then(Value::as_f64);
    let ms_of = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .and_then(crate::clock::iso_to_ms)
            .map(|ms| ms as f64)
    };
    let created_at_ms = ms_of("createdAt").unwrap_or(f64::NAN);
    let last_accessed_at_ms = ms_of("lastAccessedAt");
    let graph_degree = v
        .get("relatedMemoryIds")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    let importance = num_of("importance").unwrap_or(0.0);
    Mem {
        id: str_of("id").unwrap_or_default(),
        summary: str_of("summary"),
        source: str_of("source").unwrap_or_default(),
        importance,
        created_at_ms,
        inputs: MemoryInputs {
            importance,
            reinforced_importance: num_of("reinforcedImportance"),
            created_at_ms,
            last_reinforced_at_ms: ms_of("lastReinforcedAt"),
            last_accessed_at_ms,
            reinforcement_count: num_of("reinforcementCount").map(|c| c as u64),
            graph_degree,
        },
        last_accessed_at_ms,
    }
}

/// v4 `isProtectedMemory`: MANUAL is a hard override; otherwise the blended
/// protection score decides.
fn is_protected(mem: &Mem, now_ms: f64) -> bool {
    if mem.source == "MANUAL" {
        return true;
    }
    calculate_protection_score(&mem.inputs, &DEFAULT_PROTECTION_CONFIG, now_ms).score
        >= PROTECTION_THRESHOLD
}

/// v4 `shouldDeleteMemory`: the retention-policy check for one memory.
/// Returns `(should_delete, reason)`.
fn should_delete(
    mem: &Mem,
    now_ms: f64,
    opts: &ResolvedOptions,
    protected: bool,
) -> (bool, String) {
    if protected {
        return (false, "protected".to_string());
    }

    let effective_importance = mem.inputs.reinforced_importance.unwrap_or(mem.importance);
    if effective_importance < opts.min_importance {
        let age_months = (now_ms - mem.created_at_ms) / MS_PER_MONTH;
        if age_months >= opts.max_age_months {
            let pct = to_fixed(mem.importance * 100.0, 0);
            match mem.last_accessed_at_ms {
                None => {
                    return (
                        true,
                        format!(
                            "Low importance ({pct}%) and old ({} months)",
                            to_fixed(age_months, 1)
                        ),
                    );
                }
                Some(accessed) => {
                    let inactive_months = (now_ms - accessed) / MS_PER_MONTH;
                    if inactive_months >= opts.max_inactive_months {
                        return (
                            true,
                            format!(
                                "Low importance ({pct}%), old ({} months), and inactive ({} months)",
                                to_fixed(age_months, 1),
                                to_fixed(inactive_months, 1)
                            ),
                        );
                    }
                }
            }
        }
    }

    (false, "within retention policy".to_string())
}

/// Insertion-ordered delete set (JS `Set` keeps insertion order, and
/// `deletedIds = Array.from(deleteSet)` depends on it).
#[derive(Default)]
struct DeleteSet {
    ids: Vec<String>,
    index: HashSet<String>,
}

impl DeleteSet {
    fn insert(&mut self, id: &str) {
        if self.index.insert(id.to_string()) {
            self.ids.push(id.to_string());
        }
    }
    fn contains(&self, id: &str) -> bool {
        self.index.contains(id)
    }
}

/// Run housekeeping on a character's memories (v4 `runHousekeeping`).
pub async fn run_housekeeping(
    db: &Db,
    character_id: &str,
    options: &HousekeepingOptions,
) -> Result<HousekeepingResult, DbError> {
    let opts = options.resolve();
    let now_ms = crate::clock::now_unix_ms() as f64;

    // Batched load (ORDER BY id ASC pagination) — this order is pass 3's walk.
    let char_id = character_id.to_string();
    let batches = db.read_main(move |conn| {
        memories_read::find_by_character_id_in_batches(conn, &char_id, LOAD_BATCH_SIZE)
    })?;
    let memories: Vec<Mem> = batches.iter().flatten().map(parse_mem).collect();
    let total_before = memories.len();

    let mut result = HousekeepingResult {
        total_after: total_before as i64,
        cap_used: opts.max_memories,
        ..Default::default()
    };
    result.total_before = total_before;

    if memories.is_empty() {
        return Ok(result);
    }

    // Importance descending, then createdAt DESCENDING on ties (the v4 comment
    // says "creation date ascending" but the code subtracts the other way —
    // port the code, not the comment). Stable sort matches V8.
    let mut sorted: Vec<&Mem> = memories.iter().collect();
    sorted.sort_by(|a, b| {
        b.importance
            .partial_cmp(&a.importance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                b.created_at_ms
                    .partial_cmp(&a.created_at_ms)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    let mut delete_set = DeleteSet::default();
    let mut merge_pairs: Vec<(String, String)> = Vec::new(); // (sourceId, targetId)
    let mut merge_source_set: HashSet<String> = HashSet::new();
    // Protection is expensive; pass 1 caches for the cap pass (v4 protectedMap).
    let mut protected_map: HashMap<String, bool> = HashMap::new();

    // Pass 1: retention policy.
    for mem in &sorted {
        let protected = is_protected(mem, now_ms);
        protected_map.insert(mem.id.clone(), protected);
        let (doomed, reason) = should_delete(mem, now_ms, &opts, protected);
        if doomed {
            delete_set.insert(&mem.id);
        }
        result.details.push(HousekeepingDetail {
            memory_id: mem.id.clone(),
            action: if doomed { "deleted" } else { "kept" },
            reason,
            summary: mem.summary.clone(),
        });
    }

    // Pass 2: similarity merge over the stored vector index (opt-in; no model
    // call). Non-fatal: a store load failure skips the pass, like v4's catch.
    if opts.merge_similar {
        let remaining: Vec<&Mem> = sorted
            .iter()
            .copied()
            .filter(|m| !delete_set.contains(&m.id))
            .collect();
        let mem_by_id: HashMap<&str, &Mem> =
            remaining.iter().map(|m| (m.id.as_str(), *m)).collect();

        let char_id = character_id.to_string();
        let store = db.read_main(move |conn| CharacterVectorStore::load(conn, &char_id));
        if let Ok(store) = store {
            let entry_by_id: HashMap<&str, &[f32]> = store.all_entries().collect();

            for mem in &remaining {
                if delete_set.contains(&mem.id) {
                    continue;
                }
                let Some(embedding) = entry_by_id.get(mem.id.as_str()) else {
                    continue;
                };
                let matches = store.search(embedding, 10);

                for m in &matches {
                    if m.id == mem.id {
                        continue;
                    }
                    if m.score < opts.merge_threshold {
                        continue;
                    }
                    if delete_set.contains(&m.id) {
                        continue;
                    }
                    if merge_source_set.contains(&m.id) {
                        continue;
                    }
                    let Some(match_mem) = mem_by_id.get(m.id.as_str()) else {
                        continue;
                    };

                    let keep_current = mem.importance > match_mem.importance
                        || (mem.importance == match_mem.importance
                            && mem.created_at_ms > match_mem.created_at_ms);

                    let similarity_pct = to_fixed(m.score * 100.0, 0);
                    if keep_current {
                        merge_pairs.push((match_mem.id.clone(), mem.id.clone()));
                        merge_source_set.insert(match_mem.id.clone());
                        delete_set.insert(&match_mem.id);
                        result.details.push(HousekeepingDetail {
                            memory_id: match_mem.id.clone(),
                            action: "merged",
                            reason: format!(
                                "Similar to memory {} ({similarity_pct}% similarity)",
                                mem.id
                            ),
                            summary: match_mem.summary.clone(),
                        });
                    } else {
                        merge_pairs.push((mem.id.clone(), match_mem.id.clone()));
                        merge_source_set.insert(mem.id.clone());
                        delete_set.insert(&mem.id);
                        result.details.push(HousekeepingDetail {
                            memory_id: mem.id.clone(),
                            action: "merged",
                            reason: format!(
                                "Similar to memory {} ({similarity_pct}% similarity)",
                                match_mem.id
                            ),
                            summary: mem.summary.clone(),
                        });
                        break;
                    }
                }
            }
        }
    }

    // Pass 3: enforce the hard cap — over the LOAD order, not the sorted order
    // (v4 filters `memories`, not `sortedMemories`, here). Cheap pre-check: if
    // every remaining memory is protected, skip the scoring pass entirely.
    let remaining_after_deletion: Vec<&Mem> = memories
        .iter()
        .filter(|m| !delete_set.contains(&m.id))
        .collect();
    let has_deletion_candidate = remaining_after_deletion.len() > opts.max_memories
        && remaining_after_deletion.iter().any(|m| {
            !protected_map
                .get(&m.id)
                .copied()
                .unwrap_or_else(|| is_protected(m, now_ms))
        });
    if has_deletion_candidate {
        let mut scored: Vec<(&Mem, f64)> = remaining_after_deletion
            .iter()
            .map(|m| {
                let ew = calculate_effective_weight(&m.inputs, &DEFAULT_WEIGHTING_CONFIG, now_ms);
                (*m, ew.effective_weight)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let excess = remaining_after_deletion.len() - opts.max_memories;
        let mut deleted_for_limit = 0usize;
        for (mem, _) in scored.iter().rev() {
            if deleted_for_limit >= excess {
                break;
            }
            if delete_set.contains(&mem.id) {
                continue;
            }
            let protected = protected_map
                .get(&mem.id)
                .copied()
                .unwrap_or_else(|| is_protected(mem, now_ms));
            if protected {
                continue;
            }
            delete_set.insert(&mem.id);
            result.details.push(HousekeepingDetail {
                memory_id: mem.id.clone(),
                action: "deleted",
                reason: format!("Exceeded memory limit ({})", opts.max_memories),
                summary: mem.summary.clone(),
            });
            deleted_for_limit += 1;
        }
    }

    let deleted_ids = delete_set.ids;

    if !opts.dry_run && !deleted_ids.is_empty() {
        // Delete through the chokepoint so neighbours' relatedMemoryIds are
        // scrubbed before the rows go away.
        let ids = deleted_ids.clone();
        let deleted_count = db
            .write(move |writers| writers.main().memories().delete_many_with_unlink(&ids))
            .await?;

        // Vector-store cleanup — non-fatal (v4 logs a warn and keeps the result).
        let char_id = character_id.to_string();
        let ids = deleted_ids.clone();
        let _ = db
            .write(move |writers| {
                let main = writers.main();
                let mut store = CharacterVectorStore::load(main.connection(), &char_id)?;
                for id in &ids {
                    store.remove_vector(id);
                }
                store.flush(&main.vector_indices())?;
                Ok(())
            })
            .await;

        result.deleted = deleted_count;
        result.merged = merge_pairs.len();
        result.deleted_ids = deleted_ids.clone();
        result.merged_ids = merge_pairs.iter().map(|(src, _)| src.clone()).collect();
    } else if opts.dry_run {
        result.deleted = deleted_ids.len() as i64;
        result.merged = merge_pairs.len();
        result.deleted_ids = deleted_ids.clone();
        result.merged_ids = merge_pairs.iter().map(|(src, _)| src.clone()).collect();
    }

    result.kept = total_before as i64 - deleted_ids.len() as i64;
    result.total_after = result.kept;

    Ok(result)
}

/// Preview housekeeping without changing anything (v4 `getHousekeepingPreview`).
pub async fn get_housekeeping_preview(
    db: &Db,
    character_id: &str,
    options: &HousekeepingOptions,
) -> Result<HousekeepingResult, DbError> {
    let mut opts = options.clone();
    opts.dry_run = Some(true);
    run_housekeeping(db, character_id, &opts).await
}

/// Whether housekeeping is needed (v4 `needsHousekeeping`): at/over 80% of the
/// cap, or a preview finds something to delete.
pub async fn needs_housekeeping(
    db: &Db,
    character_id: &str,
    options: &HousekeepingOptions,
) -> Result<bool, DbError> {
    let max_memories = options.max_memories.unwrap_or(2000);

    let char_id = character_id.to_string();
    let count = db.read_main(move |conn| memories_read::count_by_character_id(conn, &char_id))?;
    if count as f64 >= (max_memories as f64) * 0.8 {
        return Ok(true);
    }

    if count > 0 {
        let preview = get_housekeeping_preview(db, character_id, options).await?;
        return Ok(preview.deleted > 0);
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memories::{CreateOptions, MemCreate};
    use crate::db::vector_indices::VectorEntryInput;
    use crate::db::Writer;
    use tempfile::{tempdir, TempDir};

    /// A throwaway base64 pepper keys the fresh encrypted DB (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR: &str = "char-hk";
    const OLD: &str = "2020-01-01T00:00:00.000Z";

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

    struct Seed {
        id: &'static str,
        importance: f64,
        source: &'static str,
        created_at: &'static str,
        last_accessed_at: Option<&'static str>,
        vector: Option<Vec<f32>>,
    }

    fn make_db(seeds: &[Seed]) -> (TempDir, Db) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
            for s in seeds {
                w.memories()
                    .create(
                        &MemCreate {
                            character_id: CHAR.to_string(),
                            about_character_id: None,
                            chat_id: None,
                            project_id: None,
                            content: format!("content {}", s.id),
                            summary: format!("summary {}", s.id),
                            keywords: vec![],
                            tags: vec![],
                            importance: s.importance,
                            embedding: None,
                            source: s.source.to_string(),
                            witnessed_context: None,
                            source_message_id: None,
                            last_accessed_at: s.last_accessed_at.map(str::to_string),
                            reinforcement_count: 1.0,
                            last_reinforced_at: None,
                            related_memory_ids: vec![],
                            reinforced_importance: s.importance,
                        },
                        &CreateOptions {
                            id: s.id.to_string(),
                            created_at: s.created_at.to_string(),
                            updated_at: s.created_at.to_string(),
                        },
                    )
                    .unwrap();
                if let Some(vec) = &s.vector {
                    let vi = w.vector_indices();
                    vi.save_meta(CHAR, vec.len() as f64).unwrap();
                    vi.add_entry(&VectorEntryInput {
                        id: s.id.to_string(),
                        character_id: CHAR.to_string(),
                        embedding: Some(vec.clone()),
                    })
                    .unwrap();
                }
            }
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn count(db: &Db, table: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        db.read_main(move |c| Ok(c.query_row(&sql, [], |r| r.get(0))?))
            .unwrap()
    }

    /// Retention policy: an old low-importance AUTO memory goes; MANUAL and
    /// recent/important ones stay. The dry run reports the same without writing.
    #[tokio::test]
    async fn retention_deletes_old_unimportant_and_dry_run_writes_nothing() {
        let seeds = [
            Seed {
                id: "hk-old-low",
                importance: 0.1,
                source: "AUTO",
                created_at: OLD,
                last_accessed_at: None,
                vector: Some(vec![1.0, 0.0]),
            },
            Seed {
                id: "hk-manual",
                importance: 0.1,
                source: "MANUAL",
                created_at: OLD,
                last_accessed_at: None,
                vector: None,
            },
            Seed {
                id: "hk-important",
                importance: 0.9,
                source: "AUTO",
                created_at: OLD,
                last_accessed_at: None,
                vector: None,
            },
        ];

        let (_dir, db) = make_db(&seeds);
        let preview = get_housekeeping_preview(&db, CHAR, &HousekeepingOptions::default())
            .await
            .unwrap();
        assert_eq!(preview.deleted, 1);
        assert_eq!(preview.deleted_ids, vec!["hk-old-low".to_string()]);
        assert_eq!(count(&db, "memories"), 3, "dry run writes nothing");

        let result = run_housekeeping(&db, CHAR, &HousekeepingOptions::default())
            .await
            .unwrap();
        assert_eq!(result.deleted, 1);
        assert_eq!(result.kept, 2);
        assert_eq!(result.total_after, 2);
        assert_eq!(count(&db, "memories"), 2);
        assert_eq!(count(&db, "vector_entries"), 0, "deleted vector removed");
        let detail = result
            .details
            .iter()
            .find(|d| d.memory_id == "hk-old-low")
            .unwrap();
        assert_eq!(detail.action, "deleted");
        assert!(detail.reason.starts_with("Low importance (10%) and old ("));
        let manual = result
            .details
            .iter()
            .find(|d| d.memory_id == "hk-manual")
            .unwrap();
        assert_eq!(manual.reason, "protected");
    }

    /// The merge pass folds a near-duplicate into the more important memory
    /// using only stored vectors, and reports it as merged (not policy-deleted).
    #[tokio::test]
    async fn merge_pass_folds_near_duplicates() {
        let recent = "2026-06-30T00:00:00.000Z";
        let seeds = [
            Seed {
                id: "hk-keep",
                importance: 0.8,
                source: "AUTO",
                created_at: recent,
                last_accessed_at: None,
                vector: Some(vec![1.0, 0.0]),
            },
            Seed {
                id: "hk-dup",
                importance: 0.6,
                source: "AUTO",
                created_at: recent,
                last_accessed_at: None,
                vector: Some(vec![0.95, 0.312_25]),
            },
            Seed {
                id: "hk-other",
                importance: 0.7,
                source: "AUTO",
                created_at: recent,
                last_accessed_at: None,
                vector: Some(vec![0.0, 1.0]),
            },
        ];

        let (_dir, db) = make_db(&seeds);
        let opts = HousekeepingOptions {
            merge_similar: Some(true),
            ..Default::default()
        };
        let result = run_housekeeping(&db, CHAR, &opts).await.unwrap();
        assert_eq!(result.merged, 1);
        assert_eq!(result.merged_ids, vec!["hk-dup".to_string()]);
        assert_eq!(result.deleted, 1, "merge sources are deleted rows");
        assert_eq!(count(&db, "memories"), 2);
        assert_eq!(count(&db, "vector_entries"), 2);
        // Pass 1 records hk-dup as "kept" (it passes retention); the merge pass
        // then appends the "merged" detail — v4 keeps both entries.
        let merged = result
            .details
            .iter()
            .find(|d| d.memory_id == "hk-dup" && d.action == "merged")
            .unwrap();
        assert_eq!(merged.reason, "Similar to memory hk-keep (95% similarity)");
    }

    /// Cap enforcement deletes the lowest-effective-weight unprotected memories
    /// from the tail, and needsHousekeeping reflects the 80% watermark.
    #[tokio::test]
    async fn cap_enforcement_and_needs_housekeeping() {
        let recent = "2026-06-30T00:00:00.000Z";
        let seeds: Vec<Seed> = [0.9, 0.8, 0.7, 0.6, 0.5]
            .iter()
            .enumerate()
            .map(|(i, &imp)| Seed {
                id: Box::leak(format!("hk-cap-{i}").into_boxed_str()),
                importance: imp,
                source: "AUTO",
                created_at: recent,
                last_accessed_at: None,
                vector: None,
            })
            .collect();

        let (_dir, db) = make_db(&seeds);
        let opts = HousekeepingOptions {
            max_memories: Some(3),
            ..Default::default()
        };
        assert!(needs_housekeeping(&db, CHAR, &opts).await.unwrap());

        let result = run_housekeeping(&db, CHAR, &opts).await.unwrap();
        assert_eq!(result.deleted, 2);
        assert_eq!(
            result.deleted_ids,
            vec!["hk-cap-4".to_string(), "hk-cap-3".to_string()],
            "lowest effective weight first (tail of the score-desc order)"
        );
        assert_eq!(result.cap_used, 3);
        assert_eq!(count(&db, "memories"), 3);
        for d in &result.details {
            if d.action == "deleted" {
                assert_eq!(d.reason, "Exceeded memory limit (3)");
            }
        }
    }
}
