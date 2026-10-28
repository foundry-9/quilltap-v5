//! Frozen memory archive cache (v4 `lib/memory/recall/archive.ts` at the
//! `83334b9a5` pin — `3f7320138` F5 + `d58548051`'s digest composition, Phase
//! A's salience fill, Phase E's `exclude` — moved from
//! `frozen-archive-cache.ts`).
//!
//! Per-character memory pool that stays byte-stable across turns within a
//! single `compactionGeneration`. The archive is the cache-friendly bulk of
//! what a character "remembers" generally. Composition (consolidation spec §5):
//! the top hot digests about each *other* character present this turn, the top
//! self digests, then a fill of the most SALIENT hot rows (reinforced
//! importance × time decay, no floor) up to the budget-sized N — finally
//! sorted by memory id so the formatted output is deterministic. Cold rows
//! never enter (the repository reads are hot-only).
//!
//! An `exclude` set (the recall replay's `asOf` corpus) computes WITHOUT
//! touching the cache — a live turn never passes one.
//!
//! Cache key: `(characterId, chatId)`, with the chat's `compactionGeneration`
//! (and the archive size, and the set of other characters present) as the
//! freshness check. Keying on the chat matters: every new chat starts at
//! generation 0, so a character-only key handed a new chat whatever
//! generation-0 archive an earlier chat had cached. Within one chat and
//! generation the archive is byte-stable. Presence is a FRESHNESS check, not
//! part of the key: a presence change REPLACES that chat's entry.
//!
//! Eviction: LRU-bounded at [`FROZEN_ARCHIVE_CACHE_MAX_ENTRIES`]. Corpus-wide
//! edits — housekeeping sweeps, deduplication, consolidation — call
//! [`invalidate_frozen_archive`], which drops every chat's entry for that
//! character. Ordinary per-turn memory writes deliberately do NOT invalidate:
//! that would rebuild the archive every turn and defeat the prefix cache. v4's
//! cache lives in the parent process and a sweep in the job child is announced
//! from the parent's job-completion hook; v5 runs jobs in-process, and the same
//! hook ([`invalidate_frozen_archives_for_completed_job`]) is called from the
//! job runner's completion region. After a restart the next turn rebuilds on
//! miss.
//!
//! Why an in-memory cache rather than a persisted message: re-posting a
//! message would either grow history per turn or require dedup logic. The
//! archive content is cheap to compute and lives at the front of the LLM
//! context tail (along with the dynamic head); inlining the same archive bytes
//! per turn means provider prefix caches see identical prefix segments — which
//! is exactly the cache hit we want.
//!
//! The process-global cache is ONE [`FrozenArchiveCache`] behind a
//! `OnceLock<Mutex<…>>` (v4's module-level `Map`). Its mutex is taken with
//! `lock().unwrap_or_else(PoisonError::into_inner)`, never `.expect`, so the job
//! pump's completion hook can never panic here (P4.D268 R-B).
//!
//! Returns the raw memory `Value` rows (v4's `Memory[]`); the caller
//! (build_context) converts them into its own injector type.
//!
//! ## Lines (v4's root `logger`; every key camelCase, v4's order)
//!
//! - DEBUG `[FrozenArchive] Composed archive` `{characterId, size,
//!   presentCharacters, digests, filled}` — every compute, BEFORE the next.
//! - DEBUG `[FrozenArchive] Computed archive` `{characterId, chatId,
//!   compactionGeneration, size, presentCharacters, returned, cacheEntries}` —
//!   every MISS, after the LRU eviction. A hit logs nothing.
//! - DEBUG `[FrozenArchive] Invalidated archives for character` `{characterId,
//!   dropped}` / `[FrozenArchive] Invalidated all archives` `{dropped}` — only
//!   when something was dropped.
//! - DEBUG `Invalidated frozen memory archives after job` `{jobId, type,
//!   characterId}` (v4's `jobs:dispatcher` child → `quilltap::jobs`) — only for
//!   a non-empty string `characterId`; a payload without one invalidates
//!   NOTHING and logs nothing (v4 `104e986c7`). v4's catch WARN `Failed to
//!   invalidate frozen memory archives after job` guards a dynamic `import()`
//!   v5 does not have — unreachable, pinned NEGATIVE.

use std::sync::{Mutex, OnceLock, PoisonError};

use serde_json::Value;

use crate::collation::locale_compare;
use crate::db::memories_read::{self, DigestSubject};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::memory_scoring::sort_by_salience;

/// Default size of the frozen archive (callers normally pass a budget-sized one).
pub const FROZEN_ARCHIVE_SIZE: usize = 25;

/// Most (character, chat) archives held at once.
pub const FROZEN_ARCHIVE_CACHE_MAX_ENTRIES: usize = 64;

/// Digests drawn per other character present this turn.
pub const FROZEN_ARCHIVE_DIGESTS_PER_PRESENT: i64 = 3;

/// Self digests drawn.
pub const FROZEN_ARCHIVE_SELF_DIGESTS: i64 = 5;

/// Pool size to draw from when ranking. Higher than the archive size so the
/// salience re-rank has room to surface long-tail high-importance rows
/// that fell below the reinforced-importance cutoff (v4
/// `FROZEN_ARCHIVE_POOL_FACTOR`).
const FROZEN_ARCHIVE_POOL_FACTOR: usize = 4;

/// v4 `JOBS_INVALIDATING_FROZEN_ARCHIVE` (`job-dispatcher.ts:663` at the
/// `83334b9a5` pin — P4.D288, the job-type list: the one maintenance job).
const JOBS_INVALIDATING_FROZEN_ARCHIVE: [&str; 1] = ["MEMORY_MAINTENANCE"];

struct CacheEntry {
    character_id: String,
    generation: i64,
    size: usize,
    /// Sorted, joined ids of the other characters present when computed.
    presence: String,
    memories: Vec<Value>,
}

/// v4's insertion-ordered `Map`: index 0 is the least recently used. At most
/// [`FROZEN_ARCHIVE_CACHE_MAX_ENTRIES`] + 1 entries ever, so the scans are
/// linear.
#[derive(Default)]
pub(crate) struct FrozenArchiveCache {
    entries: Vec<(String, CacheEntry)>,
    /// Bumped by every [`Self::clear`] — with `character_epochs`, the stamp a
    /// lock-free compute checks before it caches (see [`get_or_compute_global`]).
    clear_epoch: u64,
    /// Bumped per character by every [`Self::invalidate_character`].
    character_epochs: std::collections::HashMap<String, u64>,
}

/// v4 `cacheKey(characterId, chatId)`.
fn cache_key(character_id: &str, chat_id: &str) -> String {
    format!("{character_id}\u{0}{chat_id}")
}

/// v4 `[...new Set(presentCharacterIds ?? [])].filter(id => id && id !==
/// characterId).sort()` — first-seen dedupe, drop `""` and the responder, then
/// JS's default sort (UTF-16 code units).
fn normalize_present(character_id: &str, present_character_ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut present: Vec<String> = present_character_ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .filter(|id| !id.is_empty() && id.as_str() != character_id)
        .cloned()
        .collect();
    present.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    present
}

impl FrozenArchiveCache {
    fn position(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|(k, _)| k == key)
    }

    /// v4 `getOrComputeFrozenArchive`'s cache half. `compute` receives the
    /// normalized present ids and is called only on a miss; an `Err` is
    /// propagated and NOT cached. [`Self::lookup`] then [`Self::insert`] — the
    /// global path takes the lock for each separately (P4.168, B1-2), so this
    /// one-lock composition serves the cache-local unit tests only.
    #[cfg(test)]
    fn get_or_compute_with(
        &mut self,
        character_id: &str,
        chat_id: &str,
        compaction_generation: i64,
        size: Option<usize>,
        present_character_ids: &[String],
        compute: impl FnOnce(usize, &[String]) -> Result<Vec<Value>, DbError>,
    ) -> Result<Vec<Value>, DbError> {
        let size = size.unwrap_or(FROZEN_ARCHIVE_SIZE);
        let present = normalize_present(character_id, present_character_ids);
        if let Some(hit) = self.lookup(character_id, chat_id, compaction_generation, size, &present)
        {
            return Ok(hit);
        }
        let memories = compute(size, &present)?;
        self.insert(
            character_id,
            chat_id,
            compaction_generation,
            size,
            &present,
            memories.clone(),
        );
        Ok(memories)
    }

    /// v4's `cache.get(key)` + the validity check (`archive.ts:104-111`): a
    /// hit at the same generation / size / presence refreshes recency and
    /// answers the cached rows. `present` is already normalized.
    fn lookup(
        &mut self,
        character_id: &str,
        chat_id: &str,
        compaction_generation: i64,
        size: usize,
        present: &[String],
    ) -> Option<Vec<Value>> {
        let idx = self.position(&cache_key(character_id, chat_id))?;
        let cached = &self.entries[idx].1;
        if cached.generation != compaction_generation
            || cached.size != size
            || cached.presence != present.join(",")
        {
            return None;
        }
        // Refresh recency.
        let entry = self.entries.remove(idx);
        let memories = entry.1.memories.clone();
        self.entries.push(entry);
        Some(memories)
    }

    /// v4's `cache.set(key, …)` after the compute (`archive.ts:115`): replace
    /// the key's entry, trim to [`FROZEN_ARCHIVE_CACHE_MAX_ENTRIES`] (least
    /// recent first), and log the DEBUG.
    fn insert(
        &mut self,
        character_id: &str,
        chat_id: &str,
        compaction_generation: i64,
        size: usize,
        present: &[String],
        memories: Vec<Value>,
    ) {
        let key = cache_key(character_id, chat_id);
        if let Some(idx) = self.position(&key) {
            self.entries.remove(idx);
        }
        let returned = memories.len();
        self.entries.push((
            key,
            CacheEntry {
                character_id: character_id.to_string(),
                generation: compaction_generation,
                size,
                presence: present.join(","),
                memories,
            },
        ));
        while self.entries.len() > FROZEN_ARCHIVE_CACHE_MAX_ENTRIES {
            self.entries.remove(0);
        }

        tracing::debug!(
            target: "quilltap::memory",
            characterId = %character_id,
            chatId = %chat_id,
            compactionGeneration = compaction_generation,
            size = size as u64,
            presentCharacters = present.len() as u64,
            returned = returned as u64,
            cacheEntries = self.entries.len() as u64,
            "[FrozenArchive] Computed archive"
        );
    }

    /// v4 `invalidateFrozenArchive`'s body: drop every chat's entry for the
    /// character; returns how many were dropped.
    pub(crate) fn invalidate_character(&mut self, character_id: &str) -> usize {
        *self
            .character_epochs
            .entry(character_id.to_string())
            .or_default() += 1;
        let before = self.entries.len();
        self.entries.retain(|(_, e)| e.character_id != character_id);
        before - self.entries.len()
    }

    /// The invalidation stamp for `character_id`: changes whenever an
    /// invalidation that would drop its entries runs.
    fn epoch(&self, character_id: &str) -> (u64, u64) {
        (
            self.clear_epoch,
            self.character_epochs
                .get(character_id)
                .copied()
                .unwrap_or(0),
        )
    }

    /// v4 `invalidateAllFrozenArchives`'s body; returns how many were dropped.
    pub(crate) fn clear(&mut self) -> usize {
        self.clear_epoch += 1;
        let dropped = self.entries.len();
        self.entries.clear();
        dropped
    }
}

fn cache_mutex() -> &'static Mutex<FrozenArchiveCache> {
    static CACHE: OnceLock<Mutex<FrozenArchiveCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(FrozenArchiveCache::default()))
}

fn cache() -> std::sync::MutexGuard<'static, FrozenArchiveCache> {
    cache_mutex().lock().unwrap_or_else(PoisonError::into_inner)
}

/// Look up (or compute and cache) the frozen memory archive for a character in
/// a chat at a given compaction generation (v4 `getOrComputeFrozenArchive`).
/// Returns at most `size` memories (default [`FROZEN_ARCHIVE_SIZE`]), sorted
/// ascending by `memory.id` so the formatted output is deterministic across
/// turns. `present_character_ids` are the characters present this turn (the
/// responder is ignored if listed).
///
/// `now_ms` is the clock the salience re-rank uses (v4's implicit `Date.now()`
/// inside `sortBySalience`), injected so the caller owns the clock. `exclude`
/// (the replay's `asOf` corpus) leaves those rows out and computes WITHOUT
/// touching the cache (v4 `options.exclude`).
#[allow(clippy::too_many_arguments)]
pub fn get_or_compute_frozen_archive(
    db: &Db,
    character_id: &str,
    chat_id: &str,
    compaction_generation: i64,
    size: Option<usize>,
    present_character_ids: &[String],
    now_ms: f64,
    exclude: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<Value>, DbError> {
    if let Some(exclude) = exclude {
        let size = size.unwrap_or(FROZEN_ARCHIVE_SIZE);
        let present = normalize_present(character_id, present_character_ids);
        return compute_frozen_archive(db, character_id, size, &present, now_ms, Some(exclude));
    }
    get_or_compute_global(
        character_id,
        chat_id,
        compaction_generation,
        size,
        present_character_ids,
        |size, present| compute_frozen_archive(db, character_id, size, present, now_ms, None),
    )
}

/// The global cache's get-or-compute with the lock held ONLY for the lookup
/// and the insert, never across `compute`'s DB reads (P4.168, B1-2 — it was
/// held for the whole call, serializing every turn's archive compute
/// process-wide). Two turns may both miss and both compute, the later insert
/// winning (harmless: the same key, the same inputs).
///
/// An invalidation landing MID-compute must not be overwritten by that
/// compute's insert (the §3 review of the `322a5488e` follow-ups unification):
/// v4's `get` → `await computeFrozenArchive(…)` → `set` (`archive.ts:104-115`)
/// awaits only synchronous better-sqlite3 reads, so an invalidation — a
/// separate event-loop task — cannot land between them, while v5's compute
/// runs unlocked on a pool thread beside the job runner. The lookup captures
/// the character's invalidation stamp; the insert is skipped if it moved, so
/// the stale archive is answered to this turn only and the next turn
/// recomputes, as in v4.
fn get_or_compute_global(
    character_id: &str,
    chat_id: &str,
    compaction_generation: i64,
    size: Option<usize>,
    present_character_ids: &[String],
    compute: impl FnOnce(usize, &[String]) -> Result<Vec<Value>, DbError>,
) -> Result<Vec<Value>, DbError> {
    let size = size.unwrap_or(FROZEN_ARCHIVE_SIZE);
    let present = normalize_present(character_id, present_character_ids);
    let (hit, epoch) = {
        let mut c = cache();
        let hit = c.lookup(character_id, chat_id, compaction_generation, size, &present);
        (hit, c.epoch(character_id))
    };
    if let Some(hit) = hit {
        return Ok(hit);
    }
    let memories = compute(size, &present)?;
    let mut c = cache();
    if c.epoch(character_id) == epoch {
        c.insert(
            character_id,
            chat_id,
            compaction_generation,
            size,
            &present,
            memories.clone(),
        );
    }
    Ok(memories)
}

/// Drop every chat's cached archive for a character (v4
/// `invalidateFrozenArchive`). Call after corpus-wide edits (housekeeping
/// deletes, deduplication, consolidation) — never after an ordinary per-turn
/// write.
pub fn invalidate_frozen_archive(character_id: &str) {
    let dropped = cache().invalidate_character(character_id);
    if dropped > 0 {
        tracing::debug!(
            target: "quilltap::memory",
            characterId = %character_id,
            dropped = dropped as u64,
            "[FrozenArchive] Invalidated archives for character"
        );
    }
}

/// Drop every cached archive — a sweep across all of a user's characters (v4
/// `invalidateAllFrozenArchives`).
pub fn invalidate_all_frozen_archives() {
    let dropped = cache().clear();
    if dropped > 0 {
        tracing::debug!(
            target: "quilltap::memory",
            dropped = dropped as u64,
            "[FrozenArchive] Invalidated all archives"
        );
    }
}

/// Corpus-wide memory jobs change what a character's frozen memory archive
/// should hold (v4 `invalidateFrozenArchivesForCompletedJob`,
/// `job-dispatcher.ts:671-695`). v4 announces a sweep that ran in its job child
/// at the parent's commit moment; v5's jobs run in-process, so the job runner's
/// completion region calls this once the job's work is committed. Ordinary
/// per-turn memory writes deliberately do NOT invalidate — that would rebuild
/// the archive every turn and defeat the prefix cache it exists for.
///
/// Fires on every non-dry `MEMORY_MAINTENANCE` (P4.D288's list)
/// completion, changed or not. Only a JSON `true` `dryRun` skips. A non-empty
/// string `characterId` drops that character's chats and logs the DEBUG; a
/// missing, non-string or EMPTY one invalidates NOTHING and logs nothing (v4
/// `104e986c7`: `if (!characterId) return` — the pre user-wide arm, which
/// dropped every archive and logged `characterId: 'all'`, is gone).
pub fn invalidate_frozen_archives_for_completed_job(job_id: &str, job_type: &str, payload: &Value) {
    if !JOBS_INVALIDATING_FROZEN_ARCHIVE.contains(&job_type) {
        return;
    }
    if payload.get("dryRun") == Some(&Value::Bool(true)) {
        return;
    }
    let Some(character_id) = payload
        .get("characterId")
        .and_then(Value::as_str)
        .filter(|cid| !cid.is_empty())
    else {
        return;
    };
    invalidate_frozen_archive(character_id);
    tracing::debug!(
        target: "quilltap::jobs",
        jobId = %job_id,
        type = %job_type,
        characterId = %character_id,
        "Invalidated frozen memory archives after job"
    );
}

/// Test-only helper: fully reset the cache. v4 `resetFrozenArchiveCacheForTests`.
pub fn reset_frozen_archive_cache_for_tests() {
    cache().clear();
}

/// v4 `computeFrozenArchive(characterId, size, presentCharacterIds, exclude)`
/// over the real repository reads.
fn compute_frozen_archive(
    db: &Db,
    character_id: &str,
    size: usize,
    present: &[String],
    now_ms: f64,
    exclude: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<Value>, DbError> {
    compose_frozen_archive(
        character_id,
        size,
        present,
        now_ms,
        exclude,
        |subject, limit| {
            let cid = character_id.to_string();
            db.read_main(move |conn| memories_read::find_hot_digests(conn, &cid, subject, limit))
        },
        |pool_size| {
            let cid = character_id.to_string();
            db.read_main(move |conn| memories_read::find_most_important(conn, &cid, pool_size))
        },
    )
}

/// The composition, over injected reads (so the unit tests can drive v4's
/// mocked-repository cases): digests first — what the character knows about
/// each person in the room (in the SORTED present order), then about themself;
/// each fetch is hot-only and ordered by reinforced importance, so membership
/// is deterministic. Then the fill: the cheap top-by-reinforced-importance
/// pull, re-ranked by salience (reinforced importance × time decay), skipping
/// what is already in. An `exclude`d row is never taken. Digests are fetched even when `size` is 0 (`take`
/// returns at once; the fill is skipped).
pub(crate) fn compose_frozen_archive(
    character_id: &str,
    size: usize,
    present: &[String],
    now_ms: f64,
    exclude: Option<&std::collections::HashSet<String>>,
    mut find_hot_digests: impl FnMut(DigestSubject, i64) -> Result<Vec<Value>, DbError>,
    find_most_important: impl FnOnce(i64) -> Result<Vec<Value>, DbError>,
) -> Result<Vec<Value>, DbError> {
    // `Math.max(size, size * FROZEN_ARCHIVE_POOL_FACTOR)`.
    let pool_size = size.max(size * FROZEN_ARCHIVE_POOL_FACTOR);

    let mut chosen: Vec<Value> = Vec::new();
    let mut chosen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut take = |memories: Vec<Value>, chosen: &mut Vec<Value>| {
        for m in memories {
            if chosen.len() >= size {
                return;
            }
            let id = m
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if exclude.is_some_and(|e| e.contains(&id)) {
                continue;
            }
            if chosen_ids.insert(id) {
                chosen.push(m);
            }
        }
    };
    for other_id in present {
        let digests = find_hot_digests(
            DigestSubject::About(other_id.clone()),
            FROZEN_ARCHIVE_DIGESTS_PER_PRESENT,
        )?;
        take(digests, &mut chosen);
    }
    let self_digests = find_hot_digests(DigestSubject::SelfSubject, FROZEN_ARCHIVE_SELF_DIGESTS)?;
    take(self_digests, &mut chosen);
    let digest_count = chosen.len();

    if chosen.len() < size {
        let candidates = find_most_important(pool_size as i64)?;
        let fresh: Vec<Value> = candidates
            .into_iter()
            .filter(|m| {
                let id = m.get("id").and_then(Value::as_str).unwrap_or("");
                !chosen
                    .iter()
                    .any(|c| c.get("id").and_then(Value::as_str).unwrap_or("") == id)
            })
            .collect();
        take(
            sort_by_salience(
                fresh,
                crate::services::recall::rank::salience_input_of,
                now_ms,
            ),
            &mut chosen,
        );
    }

    tracing::debug!(
        target: "quilltap::memory",
        characterId = %character_id,
        size = size as u64,
        presentCharacters = present.len() as u64,
        digests = digest_count as u64,
        filled = (chosen.len() - digest_count) as u64,
        "[FrozenArchive] Composed archive"
    );

    // Ascending by id via `a.id.localeCompare(b.id)`.
    chosen.sort_by(|a, b| {
        let ai = a.get("id").and_then(Value::as_str).unwrap_or("");
        let bi = b.get("id").and_then(Value::as_str).unwrap_or("");
        locale_compare(ai, bi)
    });
    Ok(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::global_capture::{capture, capture_async};
    use serde_json::json;
    use std::cell::RefCell;

    // A fixed reference clock (UTC 2026-06-27T12:00:00Z).
    const NOW_MS: f64 = 1_782_993_600_000.0;

    /// v4 `frozen-archive-cache.test.ts`'s `memory(id, importance = 0.7)`.
    fn memory(id: &str, importance: f64) -> Value {
        json!({
            "id": id, "characterId": "char-1", "content": format!("content of {id}"),
            "summary": format!("summary of {id}"), "keywords": [], "tags": [],
            "importance": importance, "source": "AUTO", "reinforcementCount": 1,
            "relatedMemoryIds": [], "reinforcedImportance": importance,
            "createdAt": "2025-01-01T00:00:00.000Z", "updatedAt": "2025-01-01T00:00:00.000Z",
        })
    }

    fn digest(id: &str) -> Value {
        let mut m = memory(id, 0.9);
        m["source"] = json!("CONSOLIDATED");
        m["consolidatedFrom"] = json!(["x", "y"]);
        m
    }

    type MostImportantFn = Box<dyn FnMut(&str, i64) -> Vec<Value>>;
    type DigestsFn = Box<dyn Fn(&str, &DigestSubject, i64) -> Vec<Value>>;

    /// v4's mocked `getRepositories().memories` — `findMostImportant` and
    /// `findHotDigests`, each recording its calls.
    struct FakeRepo {
        most_important: RefCell<MostImportantFn>,
        digests: DigestsFn,
        most_important_calls: RefCell<Vec<(String, i64)>>,
        digest_calls: RefCell<Vec<(String, String, i64)>>,
    }

    impl FakeRepo {
        fn new(most_important: impl FnMut(&str, i64) -> Vec<Value> + 'static) -> Self {
            FakeRepo {
                most_important: RefCell::new(Box::new(most_important)),
                digests: Box::new(|_, _, _| Vec::new()),
                most_important_calls: RefCell::new(Vec::new()),
                digest_calls: RefCell::new(Vec::new()),
            }
        }
        fn with_digests(
            mut self,
            f: impl Fn(&str, &DigestSubject, i64) -> Vec<Value> + 'static,
        ) -> Self {
            self.digests = Box::new(f);
            self
        }
        fn calls(&self) -> usize {
            self.most_important_calls.borrow().len()
        }
    }

    fn get(
        cache: &mut FrozenArchiveCache,
        repo: &FakeRepo,
        character_id: &str,
        chat_id: &str,
        generation: i64,
        size: Option<usize>,
        present: &[&str],
    ) -> Vec<String> {
        let present: Vec<String> = present.iter().map(|s| s.to_string()).collect();
        cache
            .get_or_compute_with(
                character_id,
                chat_id,
                generation,
                size,
                &present,
                |size, present| {
                    compose_frozen_archive(
                        character_id,
                        size,
                        present,
                        NOW_MS,
                        None,
                        |subject, limit| {
                            repo.digest_calls.borrow_mut().push((
                                character_id.to_string(),
                                subject.as_log_str().to_string(),
                                limit,
                            ));
                            Ok((repo.digests)(character_id, &subject, limit))
                        },
                        |pool| {
                            repo.most_important_calls
                                .borrow_mut()
                                .push((character_id.to_string(), pool));
                            Ok((repo.most_important.borrow_mut())(character_id, pool))
                        },
                    )
                },
            )
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap().to_string())
            .collect()
    }

    // ---- the eleven top-level `frozen-archive-cache.test.ts` cases ----

    #[test]
    fn hits_cache_when_called_twice_with_same_generation() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("zz-1", 0.7), memory("aa-1", 0.7)]);
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        assert_eq!(repo.calls(), 1);
    }

    #[test]
    fn recomputes_on_generation_bump() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("zz-1", 0.7), memory("aa-1", 0.7)]);
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        get(&mut c, &repo, "char-1", "chat-1", 1, None, &[]);
        assert_eq!(repo.calls(), 2);
    }

    #[test]
    fn returns_memories_sorted_ascending_by_id() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| {
            vec![
                memory("zz-1", 0.9),
                memory("aa-1", 0.8),
                memory("mm-1", 0.7),
            ]
        });
        let ids = get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        assert_eq!(ids, vec!["aa-1", "mm-1", "zz-1"]);
    }

    #[test]
    fn caps_result_at_frozen_archive_size() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| {
            (0..FROZEN_ARCHIVE_SIZE + 10)
                .map(|i| memory(&format!("id-{i:03}"), 0.5 + (i as f64 / 100.0)))
                .collect()
        });
        let ids = get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        assert_eq!(ids.len(), FROZEN_ARCHIVE_SIZE);
    }

    #[test]
    fn invalidate_forces_recompute() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("a-1", 0.7), memory("b-1", 0.7)]);
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        c.invalidate_character("char-1");
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        assert_eq!(repo.calls(), 2);
    }

    #[test]
    fn keeps_separate_entries_per_character() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|cid, _| {
            if cid == "char-1" {
                vec![memory("a-1", 0.7)]
            } else {
                vec![memory("b-1", 0.7)]
            }
        });
        let a = get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        let b = get(&mut c, &repo, "char-2", "chat-1", 0, None, &[]);
        assert_eq!(a[0], "a-1");
        assert_eq!(b[0], "b-1");
    }

    #[test]
    fn does_not_hand_a_new_chat_another_chats_generation_0_archive() {
        let mut c = FrozenArchiveCache::default();
        let mut n = 0;
        let repo = FakeRepo::new(move |_, _| {
            n += 1;
            vec![memory(if n == 1 { "old-1" } else { "new-1" }, 0.7)]
        });
        let first = get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        let second = get(&mut c, &repo, "char-1", "chat-2", 0, None, &[]);
        assert_eq!(repo.calls(), 2);
        assert_eq!(first[0], "old-1");
        assert_eq!(second[0], "new-1");
    }

    #[test]
    fn recomputes_when_the_requested_archive_size_changes() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("a-1", 0.7)]);
        get(&mut c, &repo, "char-1", "chat-1", 0, Some(25), &[]);
        get(&mut c, &repo, "char-1", "chat-1", 0, Some(25), &[]);
        get(&mut c, &repo, "char-1", "chat-1", 0, Some(40), &[]);
        assert_eq!(repo.calls(), 2);
        assert_eq!(
            repo.most_important_calls.borrow().last().cloned(),
            Some(("char-1".to_string(), 160))
        );
    }

    #[test]
    fn invalidate_drops_every_chat_for_that_character_and_no_other() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("a-1", 0.7)]);
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        get(&mut c, &repo, "char-1", "chat-2", 0, None, &[]);
        get(&mut c, &repo, "char-2", "chat-1", 0, None, &[]);
        assert_eq!(repo.calls(), 3);
        assert_eq!(c.invalidate_character("char-1"), 2);
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        get(&mut c, &repo, "char-1", "chat-2", 0, None, &[]);
        get(&mut c, &repo, "char-2", "chat-1", 0, None, &[]);
        assert_eq!(repo.calls(), 5);
    }

    #[test]
    fn invalidate_all_drops_everything() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("a-1", 0.7)]);
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        get(&mut c, &repo, "char-2", "chat-1", 0, None, &[]);
        assert_eq!(c.clear(), 2);
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        get(&mut c, &repo, "char-2", "chat-1", 0, None, &[]);
        assert_eq!(repo.calls(), 4);
    }

    #[test]
    fn evicts_the_least_recently_used_entry_past_the_bound() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("a-1", 0.7)]);
        for i in 0..FROZEN_ARCHIVE_CACHE_MAX_ENTRIES {
            get(&mut c, &repo, "char-1", &format!("chat-{i}"), 0, None, &[]);
        }
        // Touch chat-0 so chat-1 becomes the oldest.
        get(&mut c, &repo, "char-1", "chat-0", 0, None, &[]);
        get(&mut c, &repo, "char-1", "chat-overflow", 0, None, &[]);
        let calls_before = repo.calls();
        assert_eq!(c.entries.len(), FROZEN_ARCHIVE_CACHE_MAX_ENTRIES);

        get(&mut c, &repo, "char-1", "chat-0", 0, None, &[]);
        assert_eq!(repo.calls(), calls_before);

        get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        assert_eq!(repo.calls(), calls_before + 1);
    }

    // ---- the five `digest composition` cases ----

    #[test]
    fn takes_digests_per_present_character_then_self_digests_then_fills() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("m-1", 0.9), memory("m-2", 0.8)]).with_digests(
            |_, subject, _| match subject {
                DigestSubject::About(s) if s == "laura" => {
                    vec![digest("d-laura-1"), digest("d-laura-2")]
                }
                DigestSubject::SelfSubject => vec![digest("d-self-1")],
                _ => Vec::new(),
            },
        );
        let ids = get(&mut c, &repo, "char-1", "chat-1", 0, Some(10), &["laura"]);
        let calls = repo.digest_calls.borrow().clone();
        assert!(calls.contains(&("char-1".into(), "laura".into(), 3)));
        assert!(calls.contains(&("char-1".into(), "self".into(), 5)));
        assert_eq!(
            ids,
            vec!["d-laura-1", "d-laura-2", "d-self-1", "m-1", "m-2"]
        );
    }

    #[test]
    fn dedupes_a_digest_that_also_appears_in_the_fill_pool() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("dup-1", 0.95), memory("m-1", 0.5)])
            .with_digests(|_, subject, _| match subject {
                DigestSubject::SelfSubject => vec![digest("dup-1")],
                _ => Vec::new(),
            });
        let ids = get(&mut c, &repo, "char-1", "chat-1", 0, Some(10), &[]);
        assert_eq!(ids, vec!["dup-1", "m-1"]);
    }

    #[test]
    fn caps_the_whole_archive_at_size_with_digests_taking_priority() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("m-1", 0.7), memory("m-2", 0.7)]).with_digests(
            |_, subject, _| match subject {
                DigestSubject::SelfSubject => vec![digest("s-1"), digest("s-2"), digest("s-3")],
                _ => Vec::new(),
            },
        );
        let ids = get(&mut c, &repo, "char-1", "chat-1", 0, Some(4), &[]);
        assert_eq!(ids, vec!["m-1", "s-1", "s-2", "s-3"]);
    }

    #[test]
    fn is_deterministic_regardless_of_present_id_order_and_ignores_the_responder() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| Vec::new()).with_digests(|_, subject, _| match subject {
            DigestSubject::About(s) => vec![digest(&format!("d-{s}"))],
            _ => Vec::new(),
        });
        let a = get(
            &mut c,
            &repo,
            "char-1",
            "chat-a",
            0,
            Some(10),
            &["zed", "amy", "char-1"],
        );
        let b = get(
            &mut c,
            &repo,
            "char-1",
            "chat-b",
            0,
            Some(10),
            &["amy", "zed"],
        );
        assert_eq!(a, vec!["d-amy", "d-zed"]);
        assert_eq!(b, a);
    }

    #[test]
    fn recomputes_when_the_set_of_present_characters_changes() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| Vec::new());
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &["amy"]);
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &["amy"]);
        get(&mut c, &repo, "char-1", "chat-1", 0, None, &["amy", "bob"]);
        assert_eq!(repo.calls(), 2);
    }

    // ---- v5-local pins ----

    #[test]
    fn presence_sorts_by_utf16_code_units_and_drops_empty_and_duplicates() {
        // JS's comparator-less `.sort()` puts 'B' (0x42) before 'a' (0x61); the
        // first-seen dedupe keeps one 'a'.
        let ids: Vec<String> = ["a", "", "B", "a", "resp"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(normalize_present("resp", &ids), vec!["B", "a"]);
    }

    #[test]
    fn final_sort_uses_locale_compare_lowercase_before_uppercase() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("A", 0.5), memory("a", 0.5)]);
        let ids = get(&mut c, &repo, "char-1", "chat-1", 0, None, &[]);
        assert_eq!(ids, vec!["a", "A"], "localeCompare puts lowercase first");
    }

    #[test]
    fn size_zero_still_fetches_digests_and_skips_the_fill() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("m-1", 0.7)])
            .with_digests(|_, _, _| vec![digest("d-1")]);
        let ids = get(&mut c, &repo, "char-1", "chat-1", 0, Some(0), &["amy"]);
        assert!(ids.is_empty());
        assert_eq!(repo.digest_calls.borrow().len(), 2);
        assert_eq!(repo.calls(), 0);
    }

    #[test]
    fn an_error_is_propagated_and_not_cached() {
        let mut c = FrozenArchiveCache::default();
        let err = c.get_or_compute_with("c", "chat", 0, None, &[], |_, _| {
            Err(DbError::Internal("boom".into()))
        });
        assert!(err.is_err());
        assert!(c.entries.is_empty());
    }

    // ---- the lines: capture pins + silence legs ----

    #[test]
    fn composed_then_computed_lines_and_a_silent_hit() {
        let mut c = FrozenArchiveCache::default();
        let repo = FakeRepo::new(|_, _| vec![memory("m-1", 0.9)]).with_digests(|_, subject, _| {
            match subject {
                DigestSubject::About(_) => vec![digest("d-amy")],
                _ => Vec::new(),
            }
        });
        let (_, lines) = capture(|| {
            get(
                &mut c,
                &repo,
                "char-1",
                "chat-1",
                2,
                Some(10),
                &["amy", "char-1"],
            )
        });
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::memory [FrozenArchive] Composed archive characterId=char-1 size=10 \
                 presentCharacters=1 digests=1 filled=1"
                    .to_string(),
                "DEBUG quilltap::memory [FrozenArchive] Computed archive characterId=char-1 chatId=chat-1 \
                 compactionGeneration=2 size=10 presentCharacters=1 returned=2 cacheEntries=1"
                    .to_string(),
            ]
        );
        let (_, hit) = capture(|| get(&mut c, &repo, "char-1", "chat-1", 2, Some(10), &["amy"]));
        assert!(hit.is_empty(), "a hit logs nothing: {hit:?}");
    }

    // The global-cache tests below share one process-global map with every
    // other test in this binary; each uses ids no other test touches, and none
    // calls `invalidate_all` / `reset` except the job-hook tests, which seed
    // and assert only their own entries.
    static GLOBAL_SERIAL: Mutex<()> = Mutex::new(());

    fn seed_global(character_id: &str, chat_id: &str) {
        cache()
            .get_or_compute_with(character_id, chat_id, 0, None, &[], |_, _| {
                Ok(vec![memory("seed", 0.5)])
            })
            .unwrap();
    }

    fn global_has(character_id: &str, chat_id: &str) -> bool {
        cache()
            .position(&cache_key(character_id, chat_id))
            .is_some()
    }

    /// P4.168 (B1-2): the global get-or-compute holds no lock across the
    /// compute — a compute that itself reaches the cache (here an
    /// invalidation, as a concurrent turn's would) completes instead of
    /// deadlocking on the process-global `Mutex`.
    #[test]
    fn the_global_compute_runs_outside_the_cache_lock() {
        let _s = GLOBAL_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
        // Probe with `try_lock` from inside the compute: a held lock answers
        // `WouldBlock` (the assertion fails) where a blocking `lock` would
        // deadlock the binary on a regression.
        let got = get_or_compute_global("fa-lock-char", "chat-1", 0, None, &[], |_, _| {
            // Retried: another module's test in this binary may hold the lock
            // for an instant (its own invalidation); a regression holds it for
            // the whole compute, so every attempt fails.
            let free = (0..50).any(|_| {
                let ok = cache_mutex().try_lock().is_ok();
                if !ok {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                ok
            });
            Ok(vec![memory(if free { "free" } else { "held" }, 0.5)])
        })
        .expect("the compute succeeds");
        assert_eq!(
            got[0]["id"], "free",
            "the compute must not run under the cache lock"
        );
        assert!(
            global_has("fa-lock-char", "chat-1"),
            "the compute is cached"
        );
        invalidate_frozen_archive("fa-lock-char");
    }

    /// An invalidation landing while the compute runs (a job completing beside
    /// the turn) is not overwritten by the compute's insert: the stale result
    /// answers this call only, and the next call recomputes. Same for the
    /// all-archives sweep.
    #[test]
    fn an_invalidation_during_the_compute_is_not_overwritten() {
        let _s = GLOBAL_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
        let got = get_or_compute_global("fa-race-char", "chat-1", 0, None, &[], |_, _| {
            invalidate_frozen_archive("fa-race-char");
            Ok(vec![memory("stale", 0.5)])
        })
        .unwrap();
        assert_eq!(got[0]["id"], "stale", "this call is answered its compute");
        assert!(
            !global_has("fa-race-char", "chat-1"),
            "the pre-invalidation archive must not be cached"
        );
        let fresh = get_or_compute_global("fa-race-char", "chat-1", 0, None, &[], |_, _| {
            Ok(vec![memory("fresh", 0.5)])
        })
        .unwrap();
        assert_eq!(fresh[0]["id"], "fresh", "the next call recomputes");
        assert!(global_has("fa-race-char", "chat-1"));

        invalidate_frozen_archive("fa-race-char");
        get_or_compute_global("fa-race-all", "chat-1", 0, None, &[], |_, _| {
            invalidate_all_frozen_archives();
            Ok(vec![memory("stale", 0.5)])
        })
        .unwrap();
        assert!(!global_has("fa-race-all", "chat-1"), "the sweep wins too");
    }

    #[test]
    fn invalidation_lines_and_their_silence_legs() {
        let _s = GLOBAL_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
        seed_global("fa-inv-char", "chat-1");
        seed_global("fa-inv-char", "chat-2");
        let ((), lines) = capture(|| invalidate_frozen_archive("fa-inv-char"));
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::memory [FrozenArchive] Invalidated archives for character \
                 characterId=fa-inv-char dropped=2"
                    .to_string()
            ]
        );
        let ((), silent) = capture(|| invalidate_frozen_archive("fa-inv-char"));
        assert!(silent.is_empty(), "zero dropped logs nothing: {silent:?}");

        seed_global("fa-inv-all", "chat-1");
        let ((), all) = capture(invalidate_all_frozen_archives);
        assert_eq!(all.len(), 1);
        assert!(
            all[0].starts_with(
                "DEBUG quilltap::memory [FrozenArchive] Invalidated all archives dropped="
            ),
            "{all:?}"
        );
        let ((), silent_all) = capture(invalidate_all_frozen_archives);
        assert!(
            silent_all.is_empty(),
            "an empty cache logs nothing: {silent_all:?}"
        );
    }

    // ---- the four `job-dispatcher-frozen-archive.test.ts` cases + edges ----

    fn hook(job_type: &str, payload: Value) -> Vec<String> {
        capture(|| invalidate_frozen_archives_for_completed_job("job-1", job_type, &payload)).1
    }

    #[test]
    fn job_hook_cases_and_edges() {
        let _s = GLOBAL_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);

        // drops the swept character after a single-character sweep
        seed_global("fa-hook-1", "chat-1");
        seed_global("fa-hook-2", "chat-1");
        let lines = hook("MEMORY_MAINTENANCE", json!({ "characterId": "fa-hook-1" }));
        assert!(!global_has("fa-hook-1", "chat-1"));
        assert!(
            global_has("fa-hook-2", "chat-1"),
            "another character survives"
        );
        assert_eq!(
            lines.last().unwrap(),
            "DEBUG quilltap::jobs Invalidated frozen memory archives after job jobId=job-1 \
             type=MEMORY_MAINTENANCE characterId=fa-hook-1"
        );

        // a payload with no characterId invalidates NOTHING and logs nothing
        // (v4 `104e986c7` — the pre user-wide arm is gone)
        let lines = hook("MEMORY_MAINTENANCE", json!({ "reason": "scheduled" }));
        assert!(global_has("fa-hook-2", "chat-1"));
        assert!(lines.is_empty(), "{lines:?}");
        invalidate_frozen_archive("fa-hook-2");

        // leaves the cache alone for a dry run — and ONLY a JSON `true` is dry
        seed_global("fa-hook-3", "chat-1");
        assert!(hook(
            "MEMORY_MAINTENANCE",
            json!({ "characterId": "fa-hook-3", "dryRun": true })
        )
        .is_empty());
        assert!(global_has("fa-hook-3", "chat-1"));
        let lines = hook(
            "MEMORY_MAINTENANCE",
            json!({ "characterId": "fa-hook-3", "dryRun": "true" }),
        );
        assert!(
            !global_has("fa-hook-3", "chat-1"),
            "dryRun \"true\" is not dry"
        );
        assert!(lines.last().unwrap().ends_with("characterId=fa-hook-3"));

        // never invalidates for ordinary per-turn memory work
        seed_global("fa-hook-4", "chat-1");
        assert!(hook("MEMORY_EXTRACTION", json!({ "characterId": "fa-hook-4" })).is_empty());
        assert!(global_has("fa-hook-4", "chat-1"));

        // MEMORY_MAINTENANCE per character
        let lines = hook("MEMORY_MAINTENANCE", json!({ "characterId": "fa-hook-4" }));
        assert!(!global_has("fa-hook-4", "chat-1"));
        assert!(lines
            .last()
            .unwrap()
            .contains("type=MEMORY_MAINTENANCE characterId=fa-hook-4"));

        // `characterId: ""` is a string but falsy → NOTHING, no line
        seed_global("fa-hook-5", "chat-1");
        let lines = hook("MEMORY_MAINTENANCE", json!({ "characterId": "" }));
        assert!(global_has("fa-hook-5", "chat-1"));
        assert!(lines.is_empty(), "{lines:?}");
        invalidate_frozen_archive("fa-hook-5");

        // a non-string characterId → null → NOTHING, no line
        seed_global("fa-hook-6", "chat-1");
        let lines = hook("MEMORY_MAINTENANCE", json!({ "characterId": 7 }));
        assert!(global_has("fa-hook-6", "chat-1"));
        assert!(lines.is_empty(), "{lines:?}");
        invalidate_frozen_archive("fa-hook-6");

        // v4's catch WARN is unreachable in v5 (pinned NEGATIVE, R-B).
        assert!(!lines
            .iter()
            .any(|l| l.contains("Failed to invalidate frozen memory archives")));
    }

    // ---- the in-process job runner end to end ----

    mod runner {
        use super::*;
        use crate::db::background_jobs::{
            BackgroundJob, BackgroundJobsRepository, BjCreate, CreateOptions,
        };
        use crate::services::job_runner::{
            HandlerRegistry, JobFuture, JobHandler, JobOutcome, JobRunner,
        };

        const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        const DDL: &str = "CREATE TABLE background_jobs (\
            id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, status TEXT NOT NULL, \
            payload TEXT NOT NULL, priority REAL NOT NULL, attempts REAL NOT NULL, \
            maxAttempts REAL NOT NULL, lastError TEXT, scheduledAt TEXT NOT NULL, \
            startedAt TEXT, completedAt TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

        struct Done;
        impl JobHandler for Done {
            fn handle<'a>(&'a self, _db: &'a Db, _job: &'a BackgroundJob) -> JobFuture<'a> {
                Box::pin(async { JobOutcome::Completed(None) })
            }
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_completed_housekeeping_job_drops_the_characters_archives() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("main.db");
            {
                let w = crate::db::Writer::open_writable(&path, PEPPER).unwrap();
                w.connection().execute_batch(DDL).unwrap();
            }
            let db = Db::open_main(&path, PEPPER).unwrap();
            let create = BjCreate {
                user_id: "u1".into(),
                job_type: "MEMORY_MAINTENANCE".into(),
                status: Some("PENDING".into()),
                payload: json!({ "characterId": "fa-runner-char" }),
                priority: 0.0,
                attempts: 0.0,
                max_attempts: 3.0,
                last_error: None,
                scheduled_at: "2020-01-01T00:00:00.000Z".into(),
                started_at: None,
                completed_at: None,
            };
            let opts = CreateOptions {
                id: "fa-runner-job".into(),
                created_at: "2020-01-01T00:00:00.000Z".into(),
                updated_at: "2020-01-01T00:00:00.000Z".into(),
            };
            db.write(move |ws| {
                BackgroundJobsRepository::new(ws.main().connection()).create(&create, &opts)
            })
            .await
            .unwrap();

            let mut registry = HandlerRegistry::new();
            registry.register("MEMORY_MAINTENANCE", Box::new(Done));
            let runner = JobRunner::new(db, registry);

            {
                let _s = GLOBAL_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                seed_global("fa-runner-char", "chat-1");
                seed_global("fa-runner-other", "chat-1");
            }
            let (_, lines) = capture_async(runner.pump_claim()).await;
            let _s = GLOBAL_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
            assert!(
                !global_has("fa-runner-char", "chat-1"),
                "the swept character's archive is gone"
            );
            assert!(
                global_has("fa-runner-other", "chat-1"),
                "another character's survives"
            );
            assert!(
                lines.iter().any(|l| l
                    == "DEBUG quilltap::jobs Invalidated frozen memory archives after job \
                        jobId=fa-runner-job type=MEMORY_MAINTENANCE characterId=fa-runner-char"),
                "{lines:?}"
            );
            invalidate_frozen_archive("fa-runner-other");
        }
    }
}
