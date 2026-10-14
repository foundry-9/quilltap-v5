//! The anchor-gate probe — measurement only (v4 `lib/memory/anchor-gate-probe.ts`,
//! `3f7320138` — memory-recall-and-housekeeping-fixes F2; untouched by
//! `d58548051`, so it samples and searches BOTH tiers). Ported line for line
//! (P4.D269).
//!
//! v4's header, carried: since the episodic spine, gate and stored embeddings
//! carry an anchor line (dates, entities). Hypothesis: two tellings of one fact
//! on different days, with different entity sets, now embed below the REINFORCE
//! band, so restatements land as INSERT / INSERT_RELATED rows instead of
//! reinforcing. The probe takes a character's most recent rows (each one
//! survived the gate as a new row), and for each:
//!
//! - **anchored**: embeds it the way the gate did (summary + content + anchor
//!   line) and scores it against the anchored embeddings of OLDER rows — the
//!   ones the gate could have seen when this row was written;
//! - **anchor-free**: embeds it without the anchor line and scores it against
//!   anchor-free embeddings of the same older neighbours.
//!
//! It reports how many rows cross [`MERGE_THRESHOLD`] (0.85) and
//! [`NEAR_DUPLICATE_THRESHOLD`] (0.90) each way. **Nothing is written: no rows,
//! no vectors, no access stamps.**
//!
//! Caveats (v4's): content may have gained `[+]` footnotes since the row was
//! written, and the store holds the row's CURRENT vector, so the anchored side
//! is an approximation of what the gate saw — the anchor-free side is computed
//! the same way, so the comparison between them is like for like.
//!
//! What "re-embed" means here (the recorded description got it wrong): a
//! NEIGHBOUR is re-embedded only when it carries an anchor line or lacks a
//! same-width stored embedding; an anchor-free neighbour whose stored
//! `memories.embedding` (NOT the vector-store entry) has the probe's width is
//! scored on that stored vector, costing no call.
//!
//! Faithfulness notes:
//! - `embeddingsGenerated` counts BEFORE each call, so a failed call counts.
//! - The per-id anchor-free cache fills on success only.
//! - A row is mutated IN PLACE: a throw inside the per-row `try` keeps every
//!   field set before it (`anchoredBest` set, `anchorFreeBest` partial).
//! - "Older" is a strict `<` on `Date.parse(createdAt)` (a tie is not older; an
//!   unparseable `createdAt` makes nothing older; an orphan vector id whose row
//!   never loaded is not older). The best anchor-free score replaces only on a
//!   strict `>` (the first maximum wins a tie).
//! - The two reads are v4's FALLBACK reads (`findRecent`, `findByIds` — each a
//!   `safeQuery` over `findByFilter`, whose own fallback answers first), so a
//!   failed read yields an empty sample, never an error.
//! - v4's `CharacterVectorStore.load` catches every failure and starts EMPTY
//!   (`vector-store.ts:150-162`) and its repository reads fall back, so
//!   `getCharacterVectorStore` never rejects and v4's route 500 (`Anchor gate
//!   probe failed`) is reachable only by an unexpected throw — pinned NEGATIVE
//!   (P4.D269 R-G). v5's `load` returns its `Err`, which is this function's
//!   only `Err` and the route's 500 (unit-pinned).
//! - v4's `search` switches to a bounded heap on a ≥ 1000-entry store; v5's is
//!   always the linear scan, so tie order can differ only there (recorded,
//!   unreachable in any fixture).

use std::collections::{HashMap, HashSet};

use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::db::fallback;
use crate::db::runtime::Db;
use crate::db::vector_store::{CharacterVectorStore, VectorSearchResult};
use crate::db::{memories_read, DbError};
use crate::embedding_vector::cosine_similarity;
use crate::episodic::{build_memory_embedding_text, js_date_parse_ms};
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
use crate::services::memory_gate::{MERGE_THRESHOLD, NEAR_DUPLICATE_THRESHOLD};
use crate::services::memory_service::{anchor_view_of, embedding_vec_of};

/// The tracing target for the module's own lines (v4
/// `createServiceLogger('AnchorGateProbe')`, which renders `{ service:
/// 'AnchorGateProbe', module: 'service' }` — the child fields are not
/// call-site keys).
pub const LOG_TARGET: &str = "quilltap::services::anchor_gate_probe";

/// Neighbours pulled from each search when looking for a row's best match.
pub const PROBE_TOP_K: usize = 5;

/// Serialize an `f64` the way `JSON.stringify` writes a JS number (an
/// integer-valued double collapses: `1.0` → `1`).
fn js_f64<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    crate::db::js_number_to_json(*v).serialize(s)
}

fn js_opt_f64<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(f) => js_f64(f, s),
        None => s.serialize_none(),
    }
}

/// One probed row (v4 `AnchorGateProbeRow`; key order = wire order). The four
/// nullable fields always serialize (`null` when unset).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorGateProbeRow {
    pub memory_id: String,
    pub summary: String,
    pub created_at: String,
    pub has_anchor_line: bool,
    /// Best cosine against older rows, anchored text vs. anchored vectors.
    #[serde(serialize_with = "js_opt_f64")]
    pub anchored_best: Option<f64>,
    pub anchored_best_id: Option<String>,
    /// Best cosine against older rows, anchor-free text on both sides.
    #[serde(serialize_with = "js_opt_f64")]
    pub anchor_free_best: Option<f64>,
    pub anchor_free_best_id: Option<String>,
}

/// v4 `AnchorGateProbeBand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorGateProbeBand {
    /// Rows whose best match reaches MERGE_THRESHOLD (would REINFORCE or better).
    pub reinforce: usize,
    /// Rows whose best match reaches NEAR_DUPLICATE_THRESHOLD.
    pub near_duplicate: usize,
}

/// v4's `thresholds: { reinforce, nearDuplicate }`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorGateProbeThresholds {
    #[serde(serialize_with = "js_f64")]
    pub reinforce: f64,
    #[serde(serialize_with = "js_f64")]
    pub near_duplicate: f64,
}

/// v4 `AnchorGateProbeResult` (key order = wire order).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorGateProbeResult {
    pub character_id: String,
    pub sampled: usize,
    /// Rows that actually carry an anchor line (only these can differ).
    pub anchored_rows: usize,
    pub thresholds: AnchorGateProbeThresholds,
    pub anchored: AnchorGateProbeBand,
    pub anchor_free: AnchorGateProbeBand,
    /// Rows below MERGE_THRESHOLD anchored but at/above it anchor-free.
    pub crossed_only_without_anchors: usize,
    pub embeddings_generated: usize,
    pub rows: Vec<AnchorGateProbeRow>,
}

/// v4 `RunAnchorGateProbeInput`.
#[derive(Debug, Clone, Default)]
pub struct RunAnchorGateProbeInput {
    pub character_id: String,
    pub user_id: String,
    /// How many of the character's most recent rows to probe (default 50,
    /// max 200). An `f64` because v4's is a JS number; the route admits only
    /// an integer, so a fraction never reaches the read.
    pub limit: Option<f64>,
    pub embedding_profile_id: Option<String>,
}

fn str_field<'a>(memory: &'a Value, key: &str) -> &'a str {
    memory.get(key).and_then(Value::as_str).unwrap_or("")
}

/// `buildMemoryEmbeddingText(summary, content, memory)` — the gate's text.
fn anchored_text(memory: &Value) -> String {
    build_memory_embedding_text(
        str_field(memory, "summary"),
        str_field(memory, "content"),
        Some(&anchor_view_of(memory)),
    )
}

/// `buildMemoryEmbeddingText(summary, content)` — no anchor line.
fn anchor_free_text(memory: &Value) -> String {
    build_memory_embedding_text(
        str_field(memory, "summary"),
        str_field(memory, "content"),
        None,
    )
}

/// v4 `hasAnchors(memory)`: a TEXT inequality of the two renderings, not a
/// field-presence test (a row with anchor fields that render no line is
/// `false`).
fn has_anchors(memory: &Value) -> bool {
    anchored_text(memory) != anchor_free_text(memory)
}

/// v4 `band(pick)` over the rows.
fn band(
    rows: &[AnchorGateProbeRow],
    pick: impl Fn(&AnchorGateProbeRow) -> Option<f64>,
) -> AnchorGateProbeBand {
    AnchorGateProbeBand {
        reinforce: rows
            .iter()
            .filter(|r| pick(r).unwrap_or(0.0) >= MERGE_THRESHOLD)
            .count(),
        near_duplicate: rows
            .iter()
            .filter(|r| pick(r).unwrap_or(0.0) >= NEAR_DUPLICATE_THRESHOLD)
            .count(),
    }
}

/// v4's `crossedOnlyWithoutAnchors`.
fn crossed_only_without_anchors(rows: &[AnchorGateProbeRow]) -> usize {
    rows.iter()
        .filter(|r| {
            r.anchored_best.unwrap_or(0.0) < MERGE_THRESHOLD
                && r.anchor_free_best.unwrap_or(0.0) >= MERGE_THRESHOLD
        })
        .count()
}

/// The run's mutable state — v4's closure-captured counters and maps.
struct ProbeState<'a, P> {
    provider: &'a P,
    input: &'a RunAnchorGateProbeInput,
    embeddings_generated: usize,
    /// `Date.parse(createdAt)` per id; `None` is v4's `NaN`.
    created_at_by_id: HashMap<String, Option<i64>>,
    memory_cache: HashMap<String, Value>,
    anchor_free_cache: HashMap<String, Vec<f32>>,
}

impl<P: EmbeddingProvider + Sync> ProbeState<'_, P> {
    /// v4 `embed(text)`: the counter moves FIRST, so a failed call counts.
    async fn embed(&mut self, text: &str) -> Result<Vec<f32>, String> {
        self.embeddings_generated += 1;
        self.provider
            .generate_embedding_for_user(
                text,
                &self.input.user_id,
                self.input.embedding_profile_id.as_deref(),
                // lexical_text: None until P4.D290 (the anchor probe — v4 `anchor-gate-probe.ts:136-141`) lands it (C1-10).
                EmbeddingCallOptions::background(),
            )
            .await
            .map(|r| r.embedding)
            .map_err(|e| e.message)
    }

    /// v4 `anchorFreeEmbedding(memory)`: per-id cache, filled on success only.
    async fn anchor_free_embedding(&mut self, memory: &Value) -> Result<Vec<f32>, String> {
        let id = str_field(memory, "id").to_string();
        if let Some(cached) = self.anchor_free_cache.get(&id) {
            return Ok(cached.clone());
        }
        let vec = self.embed(&anchor_free_text(memory)).await?;
        self.anchor_free_cache.insert(id, vec.clone());
        Ok(vec)
    }

    fn remember(&mut self, memory: Value) {
        let id = str_field(&memory, "id").to_string();
        self.created_at_by_id.insert(
            id.clone(),
            js_date_parse_ms(str_field(&memory, "createdAt")),
        );
        self.memory_cache.insert(id, memory);
    }

    /// v4 `loadMemories(ids)`: fetch the ids not yet cached (duplicates kept,
    /// as v4's `filter` keeps them) through the fallback read.
    fn load_memories(&mut self, db: &Db, ids: &[String]) {
        let missing: Vec<String> = ids
            .iter()
            .filter(|id| !self.memory_cache.contains_key(*id))
            .cloned()
            .collect();
        if missing.is_empty() {
            return;
        }
        let loaded = fallback::find_by_filter_or_empty("memories", || {
            db.read_main(move |c| memories_read::find_by_ids(c, &missing))
        });
        for m in loaded {
            self.remember(m);
        }
    }

    /// v4's `isOlder(id)` for the row created at `created_ms`.
    fn is_older(&self, id: &str, memory_id: &str, created_ms: Option<i64>) -> bool {
        if id == memory_id {
            return false;
        }
        match (self.created_at_by_id.get(id), created_ms) {
            (Some(Some(t)), Some(created)) => *t < created,
            _ => false,
        }
    }

    /// The per-row body inside v4's `try` — mutates `row` in place, so a throw
    /// keeps every field set before it.
    async fn probe_row(
        &mut self,
        db: &Db,
        store: &CharacterVectorStore,
        store_dimensions: Option<usize>,
        memory: &Value,
        anchored: bool,
        row: &mut AnchorGateProbeRow,
    ) -> Result<(), String> {
        let memory_id = row.memory_id.clone();
        let created_ms = js_date_parse_ms(&row.created_at);
        let anchored_vec = self.embed(&anchored_text(memory)).await?;
        if let Some(dims) = store_dimensions {
            if anchored_vec.len() != dims {
                tracing::warn!(
                    target: LOG_TARGET,
                    memoryId = %memory_id,
                    probe = anchored_vec.len(),
                    store = dims,
                    "Probe embedding dimension does not match the store; skipping row"
                );
                return Ok(());
            }
        }
        let free_vec = if anchored {
            self.anchor_free_embedding(memory).await?
        } else {
            anchored_vec.clone()
        };

        // Over-fetch so filtering out self and newer rows still leaves K.
        let fetch_k = PROBE_TOP_K * 4;
        let anchored_hits = store.search(&anchored_vec, fetch_k);
        let free_hits = if anchored {
            store.search(&free_vec, fetch_k)
        } else {
            anchored_hits.clone()
        };
        let hit_ids: Vec<String> = anchored_hits
            .iter()
            .chain(free_hits.iter())
            .map(|h| h.id.clone())
            .collect();
        self.load_memories(db, &hit_ids);

        let older_anchored: Vec<&VectorSearchResult> = anchored_hits
            .iter()
            .filter(|h| self.is_older(&h.id, &memory_id, created_ms))
            .take(PROBE_TOP_K)
            .collect();
        if let Some(first) = older_anchored.first() {
            row.anchored_best = Some(first.score);
            row.anchored_best_id = Some(first.id.clone());
        }

        // Anchor-free side: the union of both searches' older neighbours, in
        // insertion order (v4's `new Set`), scored like for like.
        let mut seen = HashSet::new();
        let neighbour_ids: Vec<String> = older_anchored
            .iter()
            .map(|h| h.id.clone())
            .chain(
                free_hits
                    .iter()
                    .filter(|h| self.is_older(&h.id, &memory_id, created_ms))
                    .take(PROBE_TOP_K)
                    .map(|h| h.id.clone()),
            )
            .filter(|id| seen.insert(id.clone()))
            .collect();
        for id in neighbour_ids {
            let Some(neighbour) = self.memory_cache.get(&id).cloned() else {
                continue;
            };
            let neighbour_vec = if has_anchors(&neighbour) {
                self.anchor_free_embedding(&neighbour).await?
            } else {
                match embedding_vec_of(&neighbour).filter(|v| v.len() == free_vec.len()) {
                    Some(stored) => stored,
                    None => self.anchor_free_embedding(&neighbour).await?,
                }
            };
            let score = cosine_similarity(&free_vec, &neighbour_vec).map_err(|e| e.message())?;
            if row.anchor_free_best.is_none_or(|best| score > best) {
                row.anchor_free_best = Some(score);
                row.anchor_free_best_id = Some(id);
            }
        }
        Ok(())
    }
}

/// v4 `runAnchorGateProbe`. Reads run on the caller thread (`db.read_main`),
/// so every line here — the two WARNs, the INFO, and the fallback reads'
/// ERRORs — is capturable. The store is loaded once per run.
pub async fn run_anchor_gate_probe<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    input: &RunAnchorGateProbeInput,
) -> Result<AnchorGateProbeResult, DbError> {
    // `Math.min(200, Math.max(1, input.limit ?? 50))`.
    let limit = input.limit.unwrap_or(50.0).clamp(1.0, 200.0);
    let sample: Vec<Value> = {
        let cid = input.character_id.clone();
        let limit = limit as i64;
        fallback::find_by_filter_or_empty("memories", || {
            db.read_main(move |c| memories_read::find_recent(c, &cid, limit))
        })
    };
    let store = {
        let cid = input.character_id.clone();
        db.read_main(move |c| CharacterVectorStore::load(c, &cid))?
    };
    let store_dimensions = store.dimensions();

    let mut state = ProbeState {
        provider,
        input,
        embeddings_generated: 0,
        created_at_by_id: HashMap::new(),
        memory_cache: HashMap::new(),
        anchor_free_cache: HashMap::new(),
    };
    for m in &sample {
        state.remember(m.clone());
    }

    let mut rows: Vec<AnchorGateProbeRow> = Vec::with_capacity(sample.len());
    for memory in &sample {
        let anchored = has_anchors(memory);
        let mut row = AnchorGateProbeRow {
            memory_id: str_field(memory, "id").to_string(),
            summary: str_field(memory, "summary").to_string(),
            created_at: str_field(memory, "createdAt").to_string(),
            has_anchor_line: anchored,
            anchored_best: None,
            anchored_best_id: None,
            anchor_free_best: None,
            anchor_free_best_id: None,
        };
        if let Err(message) = state
            .probe_row(db, &store, store_dimensions, memory, anchored, &mut row)
            .await
        {
            tracing::warn!(
                target: LOG_TARGET,
                memoryId = %row.memory_id,
                error = %message,
                "Probe failed for row; continuing"
            );
        }
        rows.push(row);
    }

    let result = AnchorGateProbeResult {
        character_id: input.character_id.clone(),
        sampled: sample.len(),
        anchored_rows: rows.iter().filter(|r| r.has_anchor_line).count(),
        thresholds: AnchorGateProbeThresholds {
            reinforce: MERGE_THRESHOLD,
            near_duplicate: NEAR_DUPLICATE_THRESHOLD,
        },
        anchored: band(&rows, |r| r.anchored_best),
        anchor_free: band(&rows, |r| r.anchor_free_best),
        crossed_only_without_anchors: crossed_only_without_anchors(&rows),
        embeddings_generated: state.embeddings_generated,
        rows,
    };

    tracing::info!(
        target: LOG_TARGET,
        characterId = %result.character_id,
        sampled = result.sampled,
        anchoredRows = result.anchored_rows,
        anchoredJson = %serde_json::to_string(&result.anchored).unwrap_or_default(),
        anchorFreeJson = %serde_json::to_string(&result.anchor_free).unwrap_or_default(),
        crossedOnlyWithoutAnchors = result.crossed_only_without_anchors,
        embeddingsGenerated = result.embeddings_generated,
        "Anchor gate probe complete"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::model::embedding::{EmbeddingError, EmbeddingResult};
    use std::future::Future;

    /// Throwaway pepper for a fresh encrypted instance (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR: &str = "a0000000-0000-4000-8000-00000000000a";

    fn row(a: Option<f64>, f: Option<f64>) -> AnchorGateProbeRow {
        AnchorGateProbeRow {
            memory_id: "m".into(),
            summary: "s".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            has_anchor_line: true,
            anchored_best: a,
            anchored_best_id: a.map(|_| "n".into()),
            anchor_free_best: f,
            anchor_free_best_id: f.map(|_| "n".into()),
        }
    }

    /// v4's `band`: `(pick(r) ?? 0) >= threshold`, both bounds inclusive.
    #[test]
    fn bands_and_crossings_follow_v4s_arithmetic() {
        let rows = vec![
            row(None, None),
            row(Some(0.85), Some(0.849_999)),
            row(Some(0.9), Some(0.95)),
            row(Some(0.2), Some(0.85)),
            row(None, Some(0.9)),
        ];
        let anchored = band(&rows, |r| r.anchored_best);
        let free = band(&rows, |r| r.anchor_free_best);
        assert_eq!(
            anchored,
            AnchorGateProbeBand {
                reinforce: 2,
                near_duplicate: 1
            }
        );
        assert_eq!(
            free,
            AnchorGateProbeBand {
                reinforce: 3,
                near_duplicate: 2
            }
        );
        // Below 0.85 anchored (an absent best reads 0) but at/above it free.
        assert_eq!(crossed_only_without_anchors(&rows), 2);
    }

    /// `hasAnchors` is a TEXT inequality: anchor fields that render no line
    /// (an unparseable date, blank entities) are not an anchor line.
    #[test]
    fn has_anchors_compares_the_two_renderings() {
        let plain = serde_json::json!({ "summary": "s", "content": "c" });
        assert!(!has_anchors(&plain));
        let dated = serde_json::json!({ "summary": "s", "content": "c", "occurredAt": "2026-06-03T06:00:00.000Z" });
        assert!(has_anchors(&dated));
        let blank = serde_json::json!({ "summary": "s", "content": "c", "occurredAt": "soon", "entities": ["  "] });
        assert!(!has_anchors(&blank));
    }

    /// A row serializes all eight keys in v4's order, the four nullable ones
    /// as `null`, and an integral score the JS way.
    #[test]
    fn rows_serialize_in_v4s_wire_shape() {
        let r = row(Some(1.0), None);
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"memoryId":"m","summary":"s","createdAt":"2026-01-01T00:00:00.000Z","hasAnchorLine":true,"anchoredBest":1,"anchoredBestId":"n","anchorFreeBest":null,"anchorFreeBestId":null}"#
        );
        let t = AnchorGateProbeThresholds {
            reinforce: MERGE_THRESHOLD,
            near_duplicate: NEAR_DUPLICATE_THRESHOLD,
        };
        assert_eq!(
            serde_json::to_string(&t).unwrap(),
            r#"{"reinforce":0.85,"nearDuplicate":0.9}"#
        );
    }

    /// A scripted provider: `vec_for(text)` → the vector or the failure.
    struct Scripted<F>(F);
    impl<F: Fn(&str) -> Result<Vec<f32>, String> + Sync> EmbeddingProvider for Scripted<F> {
        fn generate_embedding_for_user(
            &self,
            text: &str,
            _user_id: &str,
            _profile_id: Option<&str>,
            _opts: EmbeddingCallOptions,
        ) -> impl Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
            let out = (self.0)(text);
            async move {
                out.map(|embedding| EmbeddingResult {
                    dimensions: embedding.len(),
                    embedding,
                    model: "scripted".into(),
                    provider: "SCRIPTED".into(),
                })
                .map_err(EmbeddingError::new)
            }
        }
    }

    /// A provisioned throwaway instance; `memories` / `meta_dims` plant rows.
    fn provisioned(memories: &[(&str, &str)], meta_dims: Option<i64>) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        {
            let w = crate::db::Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
            let c = w.connection();
            for (id, created_at) in memories {
                c.execute(
                    "INSERT INTO \"memories\" (\"id\", \"characterId\", \"content\", \"summary\", \
                     \"createdAt\", \"updatedAt\") VALUES (?1, ?2, 'c', 's', ?3, ?3)",
                    rusqlite::params![id, CHAR, created_at],
                )
                .unwrap();
            }
            if let Some(dims) = meta_dims {
                c.execute(
                    "INSERT INTO \"vector_indices\" (\"id\", \"characterId\", \"version\", \
                     \"dimensions\", \"createdAt\", \"updatedAt\") VALUES (?1, ?1, 1, ?2, \
                     '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
                    rusqlite::params![CHAR, dims],
                )
                .unwrap();
            }
        }
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    fn input() -> RunAnchorGateProbeInput {
        RunAnchorGateProbeInput {
            character_id: CHAR.into(),
            user_id: "u".into(),
            limit: None,
            embedding_profile_id: None,
        }
    }

    fn run<P: EmbeddingProvider + Sync>(db: &Db, p: &P) -> (AnchorGateProbeResult, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (result, lines) = crate::test_support::captured_with(|| {
            rt.block_on(run_anchor_gate_probe(db, p, &input()))
        });
        let ours: Vec<String> = lines
            .into_iter()
            .filter(|l| l.contains(LOG_TARGET))
            .collect();
        (result.unwrap(), ours)
    }

    /// The INFO line with v4's keys (`anchored` / `anchorFree` as `…Json`), and
    /// the silence leg: an empty character logs neither WARN.
    #[test]
    fn an_empty_character_logs_only_the_completion_line() {
        let (_dir, db) = provisioned(&[], None);
        let (result, lines) = run(&db, &Scripted(|_: &str| Ok(vec![1.0])));
        assert_eq!(result.sampled, 0);
        assert_eq!(result.embeddings_generated, 0);
        assert_eq!(
            lines,
            [format!(
                "INFO {LOG_TARGET} Anchor gate probe complete characterId={CHAR} sampled=0 \
                 anchoredRows=0 anchoredJson={{\"reinforce\":0,\"nearDuplicate\":0}} \
                 anchorFreeJson={{\"reinforce\":0,\"nearDuplicate\":0}} \
                 crossedOnlyWithoutAnchors=0 embeddingsGenerated=0"
            )]
        );
    }

    /// The counter moves BEFORE the call, so a failed call counts; the row is
    /// pushed with its nulls and v4's WARN names the error.
    #[test]
    fn a_failed_embed_counts_and_warns_and_keeps_the_row() {
        let (_dir, db) = provisioned(&[("m-1", "2026-01-01T00:00:00.000Z")], None);
        let (result, lines) = run(&db, &Scripted(|_: &str| Err("boom".to_string())));
        assert_eq!(result.sampled, 1);
        assert_eq!(result.embeddings_generated, 1, "counted before the call");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].anchored_best, None);
        assert_eq!(
            lines[0],
            format!("WARN {LOG_TARGET} Probe failed for row; continuing memoryId=m-1 error=boom")
        );
        assert!(!lines.iter().any(|l| l.contains("does not match the store")));
    }

    /// A probe vector of another width than the store's meta skips the row
    /// with v4's WARN (and the row is pushed once).
    #[test]
    fn a_dimension_mismatch_skips_the_row_with_v4s_warn() {
        let (_dir, db) = provisioned(&[("m-1", "2026-01-01T00:00:00.000Z")], Some(3));
        let (result, lines) = run(&db, &Scripted(|_: &str| Ok(vec![0.6, 0.8])));
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.embeddings_generated, 1);
        assert_eq!(
            lines[0],
            format!(
                "WARN {LOG_TARGET} Probe embedding dimension does not match the store; skipping row \
                 memoryId=m-1 probe=2 store=3"
            )
        );
        assert!(!lines.iter().any(|l| l.contains("Probe failed for row")));
    }

    /// v4's `isOlder` for an unparseable `createdAt` (v4's `NaN`): nothing is
    /// older than it and it is older than nothing. Unreachable through v4's
    /// own reads (its `findByFilter` Zod-drops such a row) — pinned here only.
    #[test]
    fn an_unparseable_created_at_is_never_older() {
        let (_dir, db) = provisioned(&[], None);
        let provider = Scripted(|_: &str| Ok(vec![1.0]));
        let inp = input();
        let mut state = ProbeState {
            provider: &provider,
            input: &inp,
            embeddings_generated: 0,
            created_at_by_id: HashMap::new(),
            memory_cache: HashMap::new(),
            anchor_free_cache: HashMap::new(),
        };
        state.remember(serde_json::json!({ "id": "bad", "createdAt": "not a date" }));
        state.remember(serde_json::json!({ "id": "old", "createdAt": "2020-01-01T00:00:00.000Z" }));
        let new_ms = js_date_parse_ms("2026-01-01T00:00:00.000Z");
        assert!(!state.is_older("bad", "row", new_ms));
        assert!(!state.is_older("old", "row", None));
        assert!(state.is_older("old", "row", new_ms));
        assert!(
            !state.is_older("old", "old", new_ms),
            "a row is never its own neighbour"
        );
        assert!(
            !state.is_older("orphan", "row", new_ms),
            "an unloaded id is not older"
        );
        drop(db);
    }
}
