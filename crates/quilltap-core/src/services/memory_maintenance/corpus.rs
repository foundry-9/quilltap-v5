//! A character's corpus, read once per maintenance run — v4
//! `lib/memory/maintenance/corpus.ts` (`104e986c7`, at the `83334b9a5` pin).
//!
//! v4's why, carried: "`loadCorpus` is the one paged read of a character's
//! memories (it replaces housekeeping's and consolidation's two load loops).
//! `MaintenanceCorpus` holds what it read and is kept in step with every
//! phase's plan as the run goes — dry run and live alike — so a later phase
//! plans against what the earlier ones did. That matters in the job child,
//! whose reads would not otherwise see what an earlier phase buffered, and it
//! is what makes a dry run report the same later-phase counts a live run does
//! (invariant 10)."
//!
//! **Order is a comparand.** The fold's buckets, the demotion ties and the
//! consolidation buckets all iterate the corpus in READ order (`ORDER BY id
//! ASC`); the map is insertion-ordered (`serde_json::Map`, preserve_order) and
//! a removal is a `shift_remove`, never a `swap_remove` (which would move the
//! last row into the gap). `put` of a known id keeps its place; a new id
//! appends — v4's `Map.set`.
//!
//! v4 yields to the event loop between pages (`setImmediate`) — scheduling,
//! no DB effect; NO-PORT. v4's batch loader TRUNCATES the corpus on a failing
//! page (its inner `findByFilter` fallback logs `Error finding entities by
//! filter` and answers `[]`, which ends the generator — the outer `Error
//! loading memory batch` fires only inside the strict scope); v5's reader logs
//! v4's line and FAILS the run, so no phase plans from a partial corpus — the
//! planner's ruling P-5, `MAINTENANCE_BATCH_LOADER_STRICT` (P4.168), pinned
//! both ways by `memory_maintenance_tier2_equivalence`.

use serde_json::{Map, Value};

use crate::db::runtime::Db;
use crate::db::{memories_read, DbError};

const MODULE: &str = "memory:maintenance:corpus";

/// v4 `CORPUS_PAGE_SIZE` — rows per page.
pub const CORPUS_PAGE_SIZE: i64 = 250;

/// v4 `tierOf(m)`: `m.tier === 'cold' ? 'cold' : 'hot'` (a NULL / absent tier
/// reads hot).
pub fn tier_of(memory: &Value) -> &'static str {
    if memory.get("tier").and_then(Value::as_str) == Some("cold") {
        "cold"
    } else {
        "hot"
    }
}

/// True for a hot row (v4 `tierOf(m) === 'hot'`).
pub fn is_hot(memory: &Value) -> bool {
    tier_of(memory) == "hot"
}

/// v4 `MaintenanceCorpus` — the run's view of a character's corpus: the rows
/// in read order, keyed by id.
#[derive(Clone, Debug, Default)]
pub struct MaintenanceCorpus {
    pub character_id: String,
    rows: Map<String, Value>,
}

fn id_of(row: &Value) -> String {
    row.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl MaintenanceCorpus {
    /// An empty corpus (v4 `new MaintenanceCorpus(characterId)` — the run
    /// that asks for the mirror alone reads nothing).
    pub fn new(character_id: &str) -> Self {
        Self {
            character_id: character_id.to_string(),
            rows: Map::new(),
        }
    }

    /// A corpus over `rows`, in order (v4's constructor).
    pub fn from_rows(character_id: &str, rows: impl IntoIterator<Item = Value>) -> Self {
        let mut corpus = Self::new(character_id);
        for row in rows {
            corpus.put(row);
        }
        corpus
    }

    /// v4 `MaintenanceCorpus.load(characterId)`: the whole corpus (both
    /// tiers, every source) through `findByCharacterIdInBatches(characterId,
    /// CORPUS_PAGE_SIZE)`, then DEBUG `Maintenance corpus loaded {module,
    /// characterId, rows}`.
    pub fn load(db: &Db, character_id: &str) -> Result<Self, DbError> {
        let cid = character_id.to_string();
        let pages = db.read_main(|c| {
            memories_read::find_by_character_id_in_batches(c, &cid, CORPUS_PAGE_SIZE)
        })?;
        let corpus = Self::from_rows(character_id, pages.into_iter().flatten());
        tracing::debug!(
            module = MODULE,
            characterId = %character_id,
            rows = corpus.size(),
            "Maintenance corpus loaded"
        );
        Ok(corpus)
    }

    pub fn size(&self) -> usize {
        self.rows.len()
    }

    pub fn get(&self, id: &str) -> Option<&Value> {
        self.rows.get(id)
    }

    /// Every row, in order (v4 `all()`).
    pub fn all(&self) -> Vec<&Value> {
        self.rows.values().collect()
    }

    /// The hot rows, in order (v4 `hot()`).
    pub fn hot(&self) -> Vec<&Value> {
        self.rows.values().filter(|m| is_hot(m)).collect()
    }

    /// Replace or add a row (v4 `put`): a known id keeps its place.
    pub fn put(&mut self, row: Value) {
        let id = id_of(&row);
        self.rows.insert(id, row);
    }

    /// Merge a patch into a row that is still here (v4 `patch` —
    /// `{...row, ...patch}`; a gone id is a no-op).
    pub fn patch(&mut self, id: &str, patch: &Map<String, Value>) {
        if let Some(Value::Object(row)) = self.rows.get_mut(id) {
            for (k, v) in patch {
                row.insert(k.clone(), v.clone());
            }
        }
    }

    /// v4 `remove(ids)`: drop the rows, then strip every removed id from every
    /// survivor's `relatedMemoryIds` ("a retired row leaves no link behind —
    /// retireMemories scrubs them").
    pub fn remove(&mut self, ids: &[String]) {
        for id in ids {
            self.rows.shift_remove(id.as_str());
        }
        if ids.is_empty() {
            return;
        }
        let gone: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        for row in self.rows.values_mut() {
            let Some(Value::Array(links)) = row.get("relatedMemoryIds") else {
                continue;
            };
            if !links
                .iter()
                .any(|l| l.as_str().is_some_and(|s| gone.contains(s)))
            {
                continue;
            }
            let kept: Vec<Value> = links
                .iter()
                .filter(|l| !l.as_str().is_some_and(|s| gone.contains(s)))
                .cloned()
                .collect();
            if let Value::Object(obj) = row {
                obj.insert("relatedMemoryIds".into(), Value::Array(kept));
            }
        }
    }
}
