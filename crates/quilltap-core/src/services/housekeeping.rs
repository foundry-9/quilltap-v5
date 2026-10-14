//! The **memory housekeeping** service — v4 `lib/memory/housekeeping.ts`
//! (`runHousekeeping` / `getHousekeepingPreview` / `needsHousekeeping`) at the
//! `01a83539d` pin: the sweep the `MEMORY_HOUSEKEEPING` job and the Memories
//! API's `?action=housekeep` run.
//!
//! Since v4 `d58548051` housekeeping keeps the HOT tier within its cap by
//! DEMOTING rows to the cold tier; it never destroys history by policy. Over
//! the character's hot rows (cold rows are the archive — counted as
//! `coldCount`, never demoted):
//!
//! 1. **Retention policy** — every hot memory is scored by the blended
//!    protection gate (`MANUAL` and `CONSOLIDATED` are hard overrides;
//!    otherwise [`calculate_protection_score`] `>= PROTECTION_THRESHOLD`), and
//!    an unprotected memory is demoted only when it is BOTH below the
//!    importance floor AND old AND inactive.
//! 2. **Similarity merge** — RETIRED in favour of consolidation; `mergeSimilar`
//!    is accepted and ignored (one DEBUG).
//! 3. **Cap enforcement** — if still over `max_memories`, demotes the
//!    lowest-effective-weight unprotected rows from the tail, rewriting the
//!    pass-1 detail in place.
//!
//! The only deletion is the opt-in **retention sweep**
//! (`memoryConsolidation.coldRetentionDays`): superseded cold non-MANUAL rows
//! older than the window go through the chokepoint
//! (`delete_many_with_unlink`), then out of the vector store (non-fatal, v4's
//! WARN). A sweep that changed anything invalidates the character's frozen
//! archive. Reasons are formatted with the ported JS `toFixed`
//! ([`crate::jsnum::to_fixed`]) so the detail strings match v4 byte-for-byte at
//! equal wall-clock.
//!
//! v4's event-loop yields (`setImmediate` every 250-row batch / 500 items) are
//! Node scheduling concerns with no DB effect; the batched read shape
//! ([`memories_read::find_by_character_id_in_batches`], `ORDER BY id ASC`) is
//! kept because it fixes the memory walk order both sides share.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::db::memories::{CountSource, TierExtra};
use crate::db::runtime::Db;
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
    /// Maximum number of HOT memories to keep (default 2000).
    pub max_memories: Option<usize>,
    /// Demote unimportant memories older than this many months (default 6).
    pub max_age_months: Option<f64>,
    /// ... and not accessed in this many months (default 6).
    pub max_inactive_months: Option<f64>,
    /// Importance floor below which old/inactive memories go (default 0.3).
    pub min_importance: Option<f64>,
    /// Retired in favour of consolidation: accepted, ignored (default false;
    /// `true` logs one DEBUG — R-F).
    pub merge_similar: Option<bool>,
    /// Retired with the merge pass: accepted, read by nothing (v4's `0.9`
    /// survives in its `DEFAULT_OPTIONS` only — R-F).
    pub merge_threshold: Option<f64>,
    /// Preview without applying (default false).
    pub dry_run: Option<bool>,
    /// The F8 `countSource` label on the retention delete's batch lines (R-C):
    /// `None` → [`CountSource::Repository`] (v4's parent — the Memories API);
    /// the `MEMORY_HOUSEKEEPING` job passes [`CountSource::ResolvedIds`] (v4's
    /// child).
    pub count_source: Option<CountSource>,
}

/// The resolved defaults (v4 `DEFAULT_OPTIONS`).
struct ResolvedOptions {
    max_memories: usize,
    max_age_months: f64,
    max_inactive_months: f64,
    min_importance: f64,
    merge_similar: bool,
    dry_run: bool,
    count_source: CountSource,
}

impl HousekeepingOptions {
    fn resolve(&self) -> ResolvedOptions {
        ResolvedOptions {
            max_memories: self.max_memories.unwrap_or(2000),
            max_age_months: self.max_age_months.unwrap_or(6.0),
            max_inactive_months: self.max_inactive_months.unwrap_or(6.0),
            min_importance: self.min_importance.unwrap_or(0.3),
            merge_similar: self.merge_similar.unwrap_or(false),
            dry_run: self.dry_run.unwrap_or(false),
            count_source: self.count_source.unwrap_or(CountSource::Repository),
        }
    }
}

/// One housekeeping action record (v4 `HousekeepingDetail`).
#[derive(Debug, Clone, PartialEq)]
pub struct HousekeepingDetail {
    pub memory_id: String,
    /// `"deleted"` / `"demoted"` / `"merged"` / `"kept"` (`"merged"` is
    /// retired — no path emits it since `d58548051`).
    pub action: &'static str,
    pub reason: String,
    pub summary: Option<String>,
}

/// The sweep's result (v4 `HousekeepingResult`; every serializer writes v4's
/// key order: `deleted, demoted, merged, kept, totalBefore, totalAfter,
/// coldCount, capUsed, deletedIds, demotedIds, mergedIds, details`).
#[derive(Debug, Clone, Default)]
pub struct HousekeepingResult {
    /// Rows deleted — the retention sweep only (expired, superseded cold rows).
    pub deleted: i64,
    /// Hot rows moved to the cold tier.
    pub demoted: i64,
    /// Retired: always 0.
    pub merged: usize,
    pub kept: i64,
    /// HOT rows before the sweep (the number the cap is measured against).
    pub total_before: usize,
    /// HOT rows after the sweep.
    pub total_after: i64,
    /// Cold (archived) rows the character holds, untouched by demotion.
    pub cold_count: usize,
    /// The effective cap used for this sweep.
    pub cap_used: usize,
    pub deleted_ids: Vec<String>,
    /// Ids moved to the cold tier (pass-1 then pass-3 insertion order).
    pub demoted_ids: Vec<String>,
    /// Retired: always empty.
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
    /// v4's services-side `m.tier === 'cold'` (§R.4(b)): NULL / absent is hot.
    cold: bool,
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
            // Episodic spine (v4 8bf3cb5f): the declared kind + the event clock.
            kind_episodic: v.get("kind").and_then(Value::as_str) == Some("episodic"),
            occurred_at_ms: crate::episodic::event_time_ms(
                v.get("occurredAt").and_then(Value::as_str),
            ),
        },
        last_accessed_at_ms,
        cold: v.get("tier").and_then(Value::as_str) == Some("cold"),
    }
}

/// v4 `isProtectedMemory`: MANUAL is explicit user intent and CONSOLIDATED
/// digests are the surviving record of their archived members — neither is
/// ever demoted by policy; otherwise the blended protection score decides.
fn is_protected(mem: &Mem, now_ms: f64) -> bool {
    if mem.source == "MANUAL" || mem.source == "CONSOLIDATED" {
        return true;
    }
    calculate_protection_score(&mem.inputs, &DEFAULT_PROTECTION_CONFIG, now_ms).score
        >= PROTECTION_THRESHOLD
}

/// v4 `shouldDeleteMemory`: the retention-policy check for one memory — since
/// `d58548051` its verdict means DEMOTE (the reasons are unchanged). Returns
/// `(should_demote, reason)`.
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

/// Insertion-ordered demote set (JS `Set` keeps insertion order, and
/// `demotedIds = Array.from(demoteSet)` depends on it — pass-1 ids, then
/// pass-3's).
#[derive(Default)]
struct DemoteSet {
    ids: Vec<String>,
    index: HashSet<String>,
}

impl DemoteSet {
    fn insert(&mut self, id: &str) {
        if self.index.insert(id.to_string()) {
            self.ids.push(id.to_string());
        }
    }
    fn contains(&self, id: &str) -> bool {
        self.index.contains(id)
    }
}

/// JS `new Date(ms).toISOString()` for the retention cutoff (R-J): `None` is
/// v4's `RangeError: Invalid time value` (a non-finite value, or one outside
/// ECMAScript's ±8.64e15 ms time range); years outside 0–9999 render in the
/// extended `±YYYYYY` form, as V8 prints them.
fn js_iso_string(ms: f64) -> Option<String> {
    if !ms.is_finite() || ms.abs() > 8.64e15 {
        return None;
    }
    // TimeClip: ToIntegerOrInfinity truncates toward zero. The civil split is
    // `clock`'s own; only the year prefix is re-rendered in V8's form (the
    // tail is the fixed 20-char `-MM-DDTHH:MM:SS.mmmZ`).
    let iso = crate::clock::iso_from_unix_ms(ms.trunc() as i64);
    let (year, tail) = iso.split_at(iso.len() - 20);
    let y: i64 = year.parse().ok()?;
    let year = if (0..=9999).contains(&y) {
        format!("{y:04}")
    } else if y < 0 {
        format!("-{:06}", -y)
    } else {
        format!("+{y:06}")
    };
    Some(format!("{year}{tail}"))
}

/// Run housekeeping on a character's memories (v4 `runHousekeeping`,
/// `housekeeping.ts:195-436` at `01a83539d` — the `d58548051` rewrite).
///
/// Housekeeping keeps the HOT tier within its cap by moving the least valuable
/// rows to the COLD tier (the archive). It never destroys history by policy:
/// the only deletion is the retention sweep, which removes cold rows that a
/// digest has superseded once they are older than
/// `memoryConsolidation.coldRetentionDays` (default: never).
///
/// - Pass 1 demotes low-importance, old, inactive rows.
/// - Pass 2 (`mergeSimilar`) is retired in favour of consolidation; the option
///   is still accepted for back-compat but does nothing (one DEBUG, R-F).
/// - Pass 3 demotes the lowest-weighted unprotected rows until the hot tier
///   fits the cap — MUTATING the pass-1 detail in place.
/// - Retention deletes expired, superseded, cold non-MANUAL rows (R-A: v4's
///   SQL is `source != 'MANUAL'`, so a superseded CONSOLIDATED digest goes too
///   — faithful at `01a83539d`).
///
/// MANUAL rows and CONSOLIDATED digests are never demoted here. A sweep that
/// changed anything invalidates the character's frozen archive.
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
    let all: Vec<Mem> = batches.iter().flatten().map(parse_mem).collect();
    // The cap, the passes and the counts below all speak of the hot tier; cold
    // rows are the archive and are only touched by the retention sweep.
    let all_count = all.len();
    let memories: Vec<Mem> = all.into_iter().filter(|m| !m.cold).collect();
    let cold_count = all_count - memories.len();
    let total_before = memories.len();

    let mut result = HousekeepingResult {
        total_before,
        total_after: total_before as i64,
        cold_count,
        cap_used: opts.max_memories,
        ..Default::default()
    };

    tracing::debug!(
        characterId = character_id,
        hot = total_before,
        cold = cold_count,
        cap = opts.max_memories,
        dryRun = opts.dry_run,
        "[Housekeeping] Starting sweep"
    );

    if opts.merge_similar {
        tracing::debug!(
            characterId = character_id,
            "[Housekeeping] mergeSimilar is retired in favour of consolidation; skipping the merge pass"
        );
    }

    let mut demote_set = DemoteSet::default();

    if !memories.is_empty() {
        // Importance descending, then createdAt DESCENDING on ties (the v4
        // comment says "creation date ascending" but the code subtracts the
        // other way — port the code, not the comment). Stable sort matches V8.
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

        // Protection is expensive; pass 1 caches for the cap pass (v4
        // `protectedMap`).
        let mut protected_map: HashMap<String, bool> = HashMap::new();

        // Pass 1: the retention policy — its verdict now means DEMOTE.
        for mem in &sorted {
            let protected = is_protected(mem, now_ms);
            protected_map.insert(mem.id.clone(), protected);
            let (demote, reason) = should_delete(mem, now_ms, &opts, protected);
            if demote {
                demote_set.insert(&mem.id);
            }
            result.details.push(HousekeepingDetail {
                memory_id: mem.id.clone(),
                action: if demote { "demoted" } else { "kept" },
                reason,
                summary: mem.summary.clone(),
            });
        }

        // Pass 3: enforce the hot-tier cap — over the LOAD order, not the
        // sorted order (v4 filters `memories`, not `sortedMemories`). Cheap
        // pre-check: if every remaining memory is protected, skip the scoring.
        let remaining: Vec<&Mem> = memories
            .iter()
            .filter(|m| !demote_set.contains(&m.id))
            .collect();
        let has_candidate = remaining.len() > opts.max_memories
            && remaining.iter().any(|m| {
                !protected_map
                    .get(&m.id)
                    .copied()
                    .unwrap_or_else(|| is_protected(m, now_ms))
            });
        if has_candidate {
            let mut scored: Vec<(&Mem, f64)> = remaining
                .iter()
                .map(|m| {
                    let ew =
                        calculate_effective_weight(&m.inputs, &DEFAULT_WEIGHTING_CONFIG, now_ms);
                    (*m, ew.effective_weight)
                })
                .collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let excess = remaining.len() - opts.max_memories;
            let mut demoted_for_limit = 0usize;
            for (mem, _) in scored.iter().rev() {
                if demoted_for_limit >= excess {
                    break;
                }
                if demote_set.contains(&mem.id) {
                    continue;
                }
                let protected = protected_map
                    .get(&mem.id)
                    .copied()
                    .unwrap_or_else(|| is_protected(mem, now_ms));
                if protected {
                    continue;
                }
                demote_set.insert(&mem.id);
                let reason = format!("Exceeded hot-memory limit ({})", opts.max_memories);
                // Pass 1 recorded a detail for EVERY hot row, so v4's `find`
                // always hits and the detail is mutated in place (its `else
                // push` arm is dead).
                match result.details.iter_mut().find(|d| d.memory_id == mem.id) {
                    Some(existing) => {
                        existing.action = "demoted";
                        existing.reason = reason;
                    }
                    None => result.details.push(HousekeepingDetail {
                        memory_id: mem.id.clone(),
                        action: "demoted",
                        reason,
                        summary: mem.summary.clone(),
                    }),
                }
                demoted_for_limit += 1;
            }
        }
    }

    let demoted_ids = demote_set.ids;

    // Retention: the only deletion. Cold, superseded, non-MANUAL rows past the
    // window. The setting is read on EVERY sweep, dry runs included.
    let settings = db.read_main(|conn| {
        Ok(crate::db::instance_settings::get_memory_consolidation_settings(conn))
    })?;
    let mut expired_ids: Vec<String> = Vec::new();
    if let Some(days) = settings.cold_retention_days.filter(|d| *d > 0) {
        // R-J: v4's `new Date(now - days * 86_400_000).toISOString()` in f64;
        // out of range is v4's `RangeError: Invalid time value` (the job's
        // per-character ERROR, the API's 500) — never a panic or a saturated
        // date.
        let cutoff = js_iso_string(now_ms - days as f64 * 86_400_000.0)
            .ok_or_else(|| DbError::Internal("Invalid time value".to_string()))?;
        let char_id = character_id.to_string();
        expired_ids = db
            .read_main(move |conn| memories_read::find_expired_cold_ids(conn, &char_id, &cutoff))?;
        for id in &expired_ids {
            result.details.push(HousekeepingDetail {
                memory_id: id.clone(),
                action: "deleted",
                reason: format!(
                    "Superseded archive row older than the {days}-day retention window"
                ),
                summary: None,
            });
        }
    }

    let mut vector_store_dirty = false;
    if !opts.dry_run && !demoted_ids.is_empty() {
        let (cid, ids) = (character_id.to_string(), demoted_ids.clone());
        db.write(move |writers| {
            writers
                .main()
                .memories()
                .update_tier_bulk(&cid, &ids, "cold", TierExtra::default())
        })
        .await?;
        // R-B (NO-PORT by architecture): v4 re-stamps its cached vector store
        // here (`setTier`, + the WARN `[Housekeeping] Failed to re-stamp vector
        // tier after demotion`). v5 keeps no cached store — every path calls
        // `CharacterVectorStore::load`, which stamps `cold` from the column —
        // so the column write IS the re-stamp.
        result.demoted = demoted_ids.len() as i64;
        result.demoted_ids = demoted_ids.clone();
        vector_store_dirty = true;
    } else if opts.dry_run {
        result.demoted = demoted_ids.len() as i64;
        result.demoted_ids = demoted_ids.clone();
    }

    if !opts.dry_run && !expired_ids.is_empty() {
        // Delete through the chokepoint so neighbours' relatedMemoryIds are
        // scrubbed before the rows go away. R-C: the label is the PATH's — the
        // MEMORY_HOUSEKEEPING job mirrors v4's child (`resolved-ids`), the
        // Memories API v4's parent (`repository`).
        let ids = expired_ids.clone();
        let count_source = opts.count_source;
        let deleted_count = db
            .write(move |writers| {
                writers
                    .main()
                    .memories()
                    .delete_many_with_unlink(&ids, count_source)
            })
            .await?;

        // P4.D288 (C1-3, mechanical): the vector-mirror cleanup is gone with the
        // mirror — the rows deleted above were the only copy of their vectors
        // (the retire path replaces this delete in unit 9).
        result.deleted = deleted_count;
        result.deleted_ids = expired_ids.clone();
        vector_store_dirty = true;
    } else if opts.dry_run {
        result.deleted = expired_ids.len() as i64;
        result.deleted_ids = expired_ids.clone();
    }

    if vector_store_dirty {
        // The corpus just changed under every cached archive for this
        // character. (P4.D268's job-completion hook ALSO invalidates on every
        // non-dry MEMORY_HOUSEKEEPING completion, changed or not.)
        crate::services::frozen_archive::invalidate_frozen_archive(character_id);
    }

    result.kept = total_before as i64 - demoted_ids.len() as i64;
    result.total_after = result.kept;

    tracing::debug!(
        characterId = character_id,
        dryRun = opts.dry_run,
        demoted = result.demoted,
        deleted = result.deleted,
        hotAfter = result.total_after,
        "[Housekeeping] Sweep computed"
    );

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

/// Whether housekeeping is needed (v4 `needsHousekeeping`): the HOT count at or
/// over 80% of the cap, or a preview finds something to demote or delete.
pub async fn needs_housekeeping(
    db: &Db,
    character_id: &str,
    options: &HousekeepingOptions,
) -> Result<bool, DbError> {
    let max_memories = options.max_memories.unwrap_or(2000);

    let char_id = character_id.to_string();
    let count =
        db.read_main(move |conn| memories_read::count_hot_by_character_id(conn, &char_id))?;
    if count as f64 >= (max_memories as f64) * 0.8 {
        return Ok(true);
    }

    if count > 0 {
        let preview = get_housekeeping_preview(db, character_id, options).await?;
        return Ok(preview.demoted + preview.deleted > 0);
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memories::{CreateOptions, MemCreate};
    use crate::db::vector_store::CharacterVectorStore;
    use crate::db::Writer;
    use tempfile::{tempdir, TempDir};

    /// A throwaway base64 pepper keys the fresh encrypted DB (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const OLD: &str = "2020-01-01T00:00:00.000Z";
    const FUTURE: &str = "2099-01-01T00:00:00.000Z";

    const DDL: &str = "
        CREATE TABLE memories (
            id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
            projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
            importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
            occurredAt TEXT, narrativeTime TEXT, entities TEXT DEFAULT '[]', kind TEXT DEFAULT 'semantic',
            sourceMessageId TEXT, lastAccessedAt TEXT, reinforcementCount REAL,
            lastReinforcedAt TEXT, relatedMemoryIds TEXT, reinforcedImportance REAL,
            tier TEXT DEFAULT 'hot', supersededById TEXT, supersededAt TEXT, consolidatedFrom TEXT DEFAULT '[]', consolidatedAt TEXT, createdAt TEXT, updatedAt TEXT);
        CREATE TABLE instance_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    ";

    #[derive(Clone)]
    struct Seed {
        id: String,
        importance: f64,
        source: &'static str,
        created_at: &'static str,
        updated_at: &'static str,
        last_accessed_at: Option<&'static str>,
        tier: &'static str,
        superseded_by: Option<&'static str>,
        vector: Option<Vec<f32>>,
    }

    /// v4's `makeMemory` default: an old, low-importance, never-accessed hot
    /// AUTO row (`housekeeping-demotion.test.ts:24-50`).
    fn seed(id: &str) -> Seed {
        Seed {
            id: id.to_string(),
            importance: 0.2,
            source: "AUTO",
            created_at: OLD,
            updated_at: OLD,
            last_accessed_at: None,
            tier: "hot",
            superseded_by: None,
            vector: None,
        }
    }

    fn make_db(character: &str, seeds: &[Seed]) -> (TempDir, Db) {
        // Every test reaches the sweep's callsites through here, so the global
        // rig is installed before any of them registers (the `Interest` race).
        crate::test_support::global_capture::install();
        let dir = tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
            for s in seeds {
                w.memories()
                    .create(
                        &MemCreate {
                            character_id: character.to_string(),
                            about_character_id: None,
                            chat_id: None,
                            project_id: None,
                            content: format!("content {}", s.id),
                            summary: format!("summary {}", s.id),
                            keywords: vec![],
                            tags: vec![],
                            importance: s.importance,
                            // P4.D288: the row IS the index.
                            embedding: s.vector.clone(),
                            source: s.source.to_string(),
                            witnessed_context: None,
                            occurred_at: None,
                            narrative_time: None,
                            entities: Vec::new(),
                            kind: "semantic".to_string(),
                            source_message_id: None,
                            last_accessed_at: s.last_accessed_at.map(str::to_string),
                            reinforcement_count: 1.0,
                            last_reinforced_at: None,
                            related_memory_ids: vec![],
                            reinforced_importance: s.importance,
                            tier: s.tier.to_string(),
                            superseded_by_id: s.superseded_by.map(str::to_string),
                            superseded_at: None,
                            consolidated_from: Vec::new(),
                            consolidated_at: None,
                        },
                        &CreateOptions {
                            id: s.id.clone(),
                            created_at: s.created_at.to_string(),
                            updated_at: s.updated_at.to_string(),
                        },
                    )
                    .unwrap();
            }
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    async fn set_retention(db: &Db, days: i64) {
        db.write(move |w| {
            crate::db::instance_settings::set_memory_consolidation_settings(
                w.main().connection(),
                &serde_json::json!({ "coldRetentionDays": days }),
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }

    fn count(db: &Db, sql: &str) -> i64 {
        let sql = sql.to_string();
        db.read_main(move |c| Ok(c.query_row(&sql, [], |r| r.get(0))?))
            .unwrap()
    }

    fn tier(db: &Db, id: &str) -> String {
        let sql = format!("SELECT tier FROM memories WHERE id = '{id}'");
        db.read_main(move |c| Ok(c.query_row(&sql, [], |r| r.get(0))?))
            .unwrap()
    }

    fn only_line(lines: &[String], needle: &str) -> String {
        let hits: Vec<&String> = lines.iter().filter(|l| l.contains(needle)).collect();
        assert_eq!(hits.len(), 1, "expected exactly one {needle:?}: {lines:?}");
        hits[0].clone()
    }

    /// v4 `housekeeping-demotion.test.ts:93-102`: pass 1 DEMOTES a
    /// low-importance, old, inactive row instead of deleting it. R-B: a FRESH
    /// store load answers it cold — the column write is the re-stamp.
    #[tokio::test]
    async fn pass_one_demotes_instead_of_deleting() {
        const CHAR: &str = "hk-pass-one";
        let mut stale = seed("stale");
        stale.importance = 0.1;
        stale.vector = Some(vec![1.0, 0.0]);
        let (_d, db) = make_db(CHAR, &[stale]);
        let (r, lines) = crate::test_support::global_capture::capture_async(run_housekeeping(
            &db,
            CHAR,
            &HousekeepingOptions::default(),
        ))
        .await;
        let r = r.unwrap();
        assert_eq!(r.demoted, 1);
        assert_eq!(r.demoted_ids, vec!["stale".to_string()]);
        assert_eq!(r.deleted, 0);
        assert_eq!((r.kept, r.total_before, r.total_after), (0, 1, 0));
        assert_eq!(r.details.len(), 1);
        assert_eq!(r.details[0].action, "demoted");
        assert!(r.details[0]
            .reason
            .starts_with("Low importance (10%) and old ("));
        assert_eq!(tier(&db, "stale"), "cold");
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM memories"),
            1,
            "nothing deleted"
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM memories WHERE embedding IS NOT NULL"
            ),
            1
        );
        let hot = db
            .read_main(|c| Ok(CharacterVectorStore::load(c, CHAR).is_hot("stale")))
            .unwrap();
        assert!(!hot, "R-B: a fresh load answers the demoted row cold");
        let start = only_line(&lines, "[Housekeeping] Starting sweep");
        assert!(start.starts_with("DEBUG "), "{start:?}");
        assert!(
            start.contains(&format!(
                " characterId={CHAR} hot=1 cold=0 cap=2000 dryRun=false"
            )),
            "{start:?}"
        );
        let done = only_line(&lines, "[Housekeeping] Sweep computed");
        assert!(
            done.contains(&format!(
                " characterId={CHAR} dryRun=false demoted=1 deleted=0 hotAfter=0"
            )),
            "{done:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("mergeSimilar is retired")),
            "silence leg: {lines:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Failed to clean up vector store")),
            "{lines:?}"
        );
    }

    /// v4 `:104-114`: pass 3 demotes the excess over the cap, deletes nothing,
    /// and MUTATES the pass-1 detail in place (no second entry per demotion).
    #[tokio::test]
    async fn pass_three_demotes_the_excess_in_place() {
        const CHAR: &str = "hk-pass-three";
        let seeds: Vec<Seed> = (0..10)
            .map(|i| {
                let mut s = seed(&format!("h{i}"));
                s.importance = 0.8;
                s
            })
            .collect();
        let (_d, db) = make_db(CHAR, &seeds);
        let opts = HousekeepingOptions {
            max_memories: Some(3),
            ..Default::default()
        };
        let r = run_housekeeping(&db, CHAR, &opts).await.unwrap();
        assert_eq!(r.demoted, 7);
        assert_eq!((r.total_before, r.total_after), (10, 3));
        assert_eq!(r.deleted, 0);
        assert_eq!(
            r.details.len(),
            10,
            "one detail per hot row — mutated, not pushed"
        );
        let demoted: Vec<_> = r.details.iter().filter(|d| d.action == "demoted").collect();
        assert_eq!(demoted.len(), 7);
        for d in demoted {
            assert_eq!(d.reason, "Exceeded hot-memory limit (3)");
        }
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM memories WHERE tier = 'cold'"),
            7
        );
        assert_eq!(count(&db, "SELECT COUNT(*) FROM memories"), 10);
    }

    /// v4 `:116-127`: only hot rows count against the cap; the cold rows are
    /// reported as `coldCount` and never touched.
    #[tokio::test]
    async fn only_hot_rows_count_against_the_cap() {
        const CHAR: &str = "hk-hot-only";
        let mut seeds: Vec<Seed> = (0..3)
            .map(|i| {
                let mut s = seed(&format!("h{i}"));
                s.importance = 0.8;
                s
            })
            .collect();
        for i in 0..20 {
            let mut s = seed(&format!("c{i}"));
            s.tier = "cold";
            s.superseded_by = Some("d");
            seeds.push(s);
        }
        let (_d, db) = make_db(CHAR, &seeds);
        let opts = HousekeepingOptions {
            max_memories: Some(3),
            ..Default::default()
        };
        let r = run_housekeeping(&db, CHAR, &opts).await.unwrap();
        assert_eq!((r.total_before, r.cold_count, r.demoted), (3, 20, 0));
        assert_eq!(r.details.len(), 3, "cold rows get no detail");
    }

    /// v4 `:129-137`: MANUAL rows and CONSOLIDATED digests are never demoted,
    /// even over the cap.
    #[tokio::test]
    async fn never_demotes_manual_or_consolidated() {
        const CHAR: &str = "hk-protected";
        let mut manual = seed("manual");
        manual.importance = 0.1;
        manual.source = "MANUAL";
        let mut digest = seed("digest");
        digest.importance = 0.1;
        digest.source = "CONSOLIDATED";
        let (_d, db) = make_db(CHAR, &[manual, digest]);
        let opts = HousekeepingOptions {
            max_memories: Some(1),
            ..Default::default()
        };
        let r = run_housekeeping(&db, CHAR, &opts).await.unwrap();
        assert_eq!(r.demoted, 0);
        assert!(r.details.iter().all(|d| d.reason == "protected"));
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM memories WHERE tier = 'hot'"),
            2
        );
    }

    /// v4 `:139-145`: a dry run reports demotions but writes nothing.
    #[tokio::test]
    async fn a_dry_run_reports_demotions_but_writes_nothing() {
        const CHAR: &str = "hk-dry";
        let mut stale = seed("stale");
        stale.importance = 0.1;
        let (_d, db) = make_db(CHAR, &[stale]);
        let r = get_housekeeping_preview(&db, CHAR, &HousekeepingOptions::default())
            .await
            .unwrap();
        assert_eq!(r.demoted, 1);
        assert_eq!(r.demoted_ids, vec!["stale".to_string()]);
        assert_eq!(tier(&db, "stale"), "hot");
    }

    /// v4 `:147-152` + R-F: `mergeSimilar` is accepted and ignored — nothing is
    /// merged — and the retired pass logs its one DEBUG.
    #[tokio::test]
    async fn merge_similar_is_accepted_and_ignored() {
        const CHAR: &str = "hk-merge-retired";
        let mut a = seed("a");
        a.importance = 0.8;
        a.created_at = "2026-06-30T00:00:00.000Z";
        a.vector = Some(vec![1.0, 0.0]);
        let mut b = a.clone();
        b.id = "b".into();
        b.vector = Some(vec![1.0, 0.0]);
        let (_d, db) = make_db(CHAR, &[a, b]);
        let opts = HousekeepingOptions {
            merge_similar: Some(true),
            merge_threshold: Some(0.8),
            ..Default::default()
        };
        let (r, lines) =
            crate::test_support::global_capture::capture_async(run_housekeeping(&db, CHAR, &opts))
                .await;
        let r = r.unwrap();
        assert_eq!((r.merged, r.demoted), (0, 0));
        assert!(r.merged_ids.is_empty());
        assert_eq!(count(&db, "SELECT COUNT(*) FROM memories"), 2);
        let line = only_line(
            &lines,
            "[Housekeeping] mergeSimilar is retired in favour of consolidation; skipping the merge pass",
        );
        assert!(line.starts_with("DEBUG "), "{line:?}");
        assert!(line.contains(&format!(" characterId={CHAR}")), "{line:?}");
    }

    /// v4 `:154-159`: with retention unset, nothing is ever deleted — not even
    /// an ancient superseded cold row.
    #[tokio::test]
    async fn retention_unset_deletes_nothing() {
        const CHAR: &str = "hk-retention-unset";
        let mut cold = seed("old-cold");
        cold.tier = "cold";
        cold.superseded_by = Some("digest");
        let (_d, db) = make_db(CHAR, &[cold]);
        let r = run_housekeeping(&db, CHAR, &HousekeepingOptions::default())
            .await
            .unwrap();
        assert_eq!(r.deleted, 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM memories"), 1);
    }

    /// v4 `:161-172` + R-A: with retention set, exactly the expired, superseded,
    /// cold, NON-MANUAL rows go — a superseded CONSOLIDATED digest too
    /// (faithful to v4 at `01a83539d`) — through the chokepoint and out of the
    /// vector store; their details carry no `summary`. A zero-hot character
    /// still runs the sweep (no early return).
    #[tokio::test]
    async fn retention_deletes_only_expired_superseded_cold_non_manual_rows() {
        const CHAR: &str = "hk-retention";
        let cold =
            |id: &str, source: &'static str, sup: Option<&'static str>, updated: &'static str| {
                let mut s = seed(id);
                s.tier = "cold";
                s.source = source;
                s.superseded_by = sup;
                s.updated_at = updated;
                s.vector = Some(vec![1.0, 0.0]);
                s
            };
        let (_d, db) = make_db(
            CHAR,
            &[
                cold("auto-old", "AUTO", Some("dg"), OLD),
                cold("digest-old", "CONSOLIDATED", Some("dg2"), OLD),
                cold("manual-old", "MANUAL", Some("dg"), OLD),
                cold("unsuperseded-old", "AUTO", None, OLD),
                cold("auto-future", "AUTO", Some("dg"), FUTURE),
            ],
        );
        set_retention(&db, 90).await;
        let preview = get_housekeeping_preview(&db, CHAR, &HousekeepingOptions::default())
            .await
            .unwrap();
        assert_eq!(preview.deleted, 2, "a dry run counts the expired ids");
        assert_eq!(count(&db, "SELECT COUNT(*) FROM memories"), 5);

        let r = run_housekeeping(&db, CHAR, &HousekeepingOptions::default())
            .await
            .unwrap();
        let mut deleted = r.deleted_ids.clone();
        deleted.sort();
        assert_eq!(
            deleted,
            vec!["auto-old".to_string(), "digest-old".to_string()]
        );
        assert_eq!(r.deleted, 2);
        assert_eq!((r.total_before, r.cold_count), (0, 5));
        assert_eq!(r.details.len(), 2);
        for d in &r.details {
            assert_eq!(d.action, "deleted");
            assert_eq!(
                d.reason,
                "Superseded archive row older than the 90-day retention window"
            );
            assert_eq!(d.summary, None);
        }
        assert_eq!(count(&db, "SELECT COUNT(*) FROM memories"), 3);
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM memories WHERE embedding IS NOT NULL"
            ),
            3
        );
    }

    // P4.D288 (C1-3): `a_failed_vector_cleanup_warns` RETIRED with its subject —
    // the vector-mirror cleanup after a retention delete is gone with the mirror.

    #[tokio::test]
    async fn an_out_of_range_retention_cutoff_is_invalid_time_value() {
        const CHAR: &str = "hk-retention-range";
        let (_d, db) = make_db(CHAR, &[seed("x")]);
        set_retention(&db, 200_000_000).await;
        let err = run_housekeeping(&db, CHAR, &HousekeepingOptions::default())
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "Invalid time value");
        set_retention(&db, 100_000_000).await;
        run_housekeeping(&db, CHAR, &HousekeepingOptions::default())
            .await
            .expect("an extended-year cutoff is still a valid date");
    }

    /// JS `toISOString()` at the ends of the time range, and extended years.
    #[test]
    fn js_iso_string_matches_v8() {
        assert_eq!(js_iso_string(0.0).unwrap(), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            js_iso_string(-8.64e15).unwrap(),
            "-271821-04-20T00:00:00.000Z"
        );
        assert_eq!(
            js_iso_string(8.64e15).unwrap(),
            "+275760-09-13T00:00:00.000Z"
        );
        assert_eq!(
            js_iso_string(-62_198_755_200_000.0).unwrap(),
            "-000001-01-01T00:00:00.000Z"
        );
        assert_eq!(js_iso_string(8.64e15 + 1.0), None);
        assert_eq!(js_iso_string(f64::NAN), None);
    }

    /// R-I: a sweep that changed anything invalidates the character's frozen
    /// archive; a no-op sweep leaves it cached. The proof: seed the cache, add a
    /// row behind its back, and see whether the next lookup recomputes.
    fn seed_frozen_archive_for_test(db: &Db, character_id: &str) -> Vec<String> {
        crate::services::frozen_archive::get_or_compute_frozen_archive(
            db,
            character_id,
            "chat-seed-for-test",
            7,
            None,
            &[],
            crate::clock::now_unix_ms() as f64,
        )
        .unwrap()
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
    }

    async fn plant_hot_row(db: &Db, character_id: &str, id: &str) {
        let (cid, id) = (character_id.to_string(), id.to_string());
        db.write(move |w| {
            w.main().connection().execute(
                "INSERT INTO memories (id, characterId, content, summary, keywords, tags, \
                 importance, source, reinforcementCount, relatedMemoryIds, reinforcedImportance, \
                 tier, consolidatedFrom, createdAt, updatedAt) VALUES (?1, ?2, 'planted', \
                 'planted', '[]', '[]', 0.9, 'MANUAL', 1, '[]', 0.9, 'hot', '[]', ?3, ?3)",
                rusqlite::params![id, cid, OLD],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_dirty_sweep_invalidates_the_frozen_archive_and_a_noop_does_not() {
        const DIRTY: &str = "c0000000-0000-4000-8000-0000000000f1";
        // UUID ids (characters too): the archive's raw read drops a non-UUID row.
        let mut stale = seed("f0000000-0000-4000-8000-000000000001");
        stale.importance = 0.1;
        let mut keeper = seed("f0000000-0000-4000-8000-000000000002");
        keeper.source = "MANUAL";
        let (_d, db) = make_db(DIRTY, &[stale, keeper]);
        let before = seed_frozen_archive_for_test(&db, DIRTY);
        assert!(!before.is_empty(), "the probe archive is not empty");
        plant_hot_row(&db, DIRTY, "f0000000-0000-4000-8000-000000000003").await;
        run_housekeeping(&db, DIRTY, &HousekeepingOptions::default())
            .await
            .unwrap();
        assert_ne!(
            seed_frozen_archive_for_test(&db, DIRTY),
            before,
            "the demotion invalidated the cached archive"
        );

        const CLEAN: &str = "c0000000-0000-4000-8000-0000000000f2";
        let mut keeper = seed("f0000000-0000-4000-8000-000000000004");
        keeper.source = "MANUAL";
        let (_d2, db2) = make_db(CLEAN, &[keeper]);
        let before = seed_frozen_archive_for_test(&db2, CLEAN);
        assert!(!before.is_empty(), "the probe archive is not empty");
        plant_hot_row(&db2, CLEAN, "f0000000-0000-4000-8000-000000000005").await;
        run_housekeeping(&db2, CLEAN, &HousekeepingOptions::default())
            .await
            .unwrap();
        assert_eq!(
            seed_frozen_archive_for_test(&db2, CLEAN),
            before,
            "a sweep that changed nothing keeps the cache"
        );
    }

    /// `needsHousekeeping`: the HOT count at 80% of the cap trips it (the cold
    /// rows never count), and a character whose only work is retention trips
    /// it through the preview's `demoted + deleted`.
    #[tokio::test]
    async fn needs_housekeeping_counts_hot_rows_and_retention() {
        const CHAR: &str = "hk-needs";
        let mut seeds: Vec<Seed> = (0..3)
            .map(|i| {
                let mut s = seed(&format!("h{i}"));
                s.importance = 0.9;
                s.source = "MANUAL";
                s
            })
            .collect();
        for i in 0..5 {
            let mut s = seed(&format!("c{i}"));
            s.tier = "cold";
            seeds.push(s);
        }
        let (_d, db) = make_db(CHAR, &seeds);
        let cap = |n| HousekeepingOptions {
            max_memories: Some(n),
            ..Default::default()
        };
        assert!(
            needs_housekeeping(&db, CHAR, &cap(3)).await.unwrap(),
            "3 hot ≥ 2.4"
        );
        assert!(
            !needs_housekeeping(&db, CHAR, &cap(5)).await.unwrap(),
            "3 hot < 4 — the 5 cold rows do not count, and nothing is demotable"
        );

        const RET: &str = "hk-needs-retention";
        let mut keep = seed("keep");
        keep.source = "MANUAL";
        let mut expired = seed("expired");
        expired.tier = "cold";
        expired.superseded_by = Some("dg");
        let (_d2, db2) = make_db(RET, &[keep, expired]);
        assert!(!needs_housekeeping(&db2, RET, &cap(100)).await.unwrap());
        set_retention(&db2, 90).await;
        assert!(
            needs_housekeeping(&db2, RET, &cap(100)).await.unwrap(),
            "the only work is retention"
        );
    }
}
