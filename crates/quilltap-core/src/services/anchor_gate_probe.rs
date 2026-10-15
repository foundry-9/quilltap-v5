//! The anchor-gate probe — measurement only (v4 `lib/memory/anchor-gate-probe.ts`
//! at the `83334b9a5` pin: `3f7320138`'s F2 probe, rewritten onto admission's
//! own decider in `8af764514` and given the lexical-profile note in
//! `4ae0c4ca6`). Ported line for line (P4.D269; P4.D290 Tier 2).
//!
//! v4's header, carried: since the episodic spine, admission and stored
//! embeddings carry an anchor line (dates, entities). Hypothesis: two tellings
//! of one fact on different days, with different entity sets, now embed below
//! the reinforce band, so restatements land as new rows instead of
//! reinforcing. The probe takes a character's most recent rows (each one
//! survived admission as a new row), and for each:
//!
//! - **anchored**: embeds it the way admission did (summary + content + anchor
//!   line, with the lexical text a Built-in profile embeds instead) and scores
//!   it against the anchored embeddings of OLDER rows — the ones admission
//!   could have seen when this row was written;
//! - **anchor-free**: embeds it without the anchor line and scores it against
//!   anchor-free embeddings of the same older neighbours.
//!
//! For each side it asks admission's own decider ([`decide_admission`], with
//! the thresholds admission uses for the profile's provider —
//! [`admission_thresholds_for_provider`], re-picked from the FIRST embedding's
//! provider) what it would do with the row against those neighbours, and
//! reports how many rows would fold (reinforce, absorb or promote) and how many
//! would be absorbed as near-duplicates. **Nothing is written: no rows, no
//! vectors, no access stamps.** On a lexical (Built-in TF-IDF) profile the
//! anchor line is never embedded, so both sides embed the same text and the
//! comparison measures nothing — `lexicalProfile` and the `note` say so.
//!
//! Caveats (v4's): content may have gained `[+]` footnotes since the row was
//! written, and the store holds the row's CURRENT vector, so the anchored side
//! is an approximation of what admission saw — the anchor-free side is
//! computed the same way, so the comparison between them is like for like.
//!
//! Faithfulness notes:
//! - `embeddingsGenerated` counts BEFORE each call, so a failed call counts.
//! - The per-id anchor-free cache fills on success only.
//! - A row is mutated IN PLACE: a throw inside the per-row `try` keeps every
//!   field set before it, and the side decided before it keeps its decision
//!   for the bands.
//! - "Older" is a strict `<` on `Date.parse(createdAt)` (a tie is not older; an
//!   unparseable `createdAt` makes nothing older; an id whose row never loaded
//!   is not older). The best anchor-free score replaces only on a strict `>`.
//! - The probe does not follow superseded members to their digests (v4: "it
//!   measures text against text").
//! - The store is loaded ONCE per run (v4's per-character cache: the probe
//!   writes nothing, so its rows do not change mid-run); the skip test is v4's
//!   `memoryIndex.shape`: a non-empty store with NO entry of the probe's width.
//! - The two row reads are v4's FALLBACK reads (`findRecent`, `findByIds`), so
//!   a failed read yields an empty sample. The store load is infallible
//!   (P4.D288, C1-3 — v4's two fallbacks), so the route's 500 is reachable only
//!   through a read-pool failure; `PROBE_STORE_LOAD_FAILURE` (P4.D269 R-G)
//!   retired on measurement (LR-12).
//! - The memory-calendar zone (the anchor line's date, `hasAnchors`) is
//!   [`refresh_memory_time_zone`] over a host fallback the CALLER passes
//!   ([`run_anchor_gate_probe_in_zone`]) — core reads no ambient zone; v4's
//!   `refreshMemoryTimeZone()` falls back to its process zone.

use std::collections::{HashMap, HashSet};

use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::admission::{
    admission_reinforced, admission_thresholds_for_provider, decide_admission,
    is_near_duplicate_decision, AdmissionActionKind, AdmissionCandidateView, AdmissionDecision,
    AdmissionNeighbour, AdmissionThresholds,
};
use crate::db::fallback;
use crate::db::runtime::Db;
use crate::db::vector_store::{CharacterVectorStore, VectorSearchResult};
use crate::db::{memories_read, DbError};
use crate::embedding_scale::is_lexical_embedding_provider;
use crate::embedding_vector::cosine_similarity;
use crate::episodic::js_date_parse_ms;
use crate::memory_embedding_text::{
    embedding_text_for, lexical_embedding_text_for, AnchorEntities, EmbeddingTextSource,
};
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
use crate::services::memory_time_zone::refresh_memory_time_zone;

/// The tracing target for the module's own lines (v4
/// `createServiceLogger('AnchorGateProbe')`, which renders `{ service:
/// 'AnchorGateProbe', module: 'service' }` — the child fields are not
/// call-site keys).
pub const LOG_TARGET: &str = "quilltap::services::anchor_gate_probe";

/// Neighbours pulled from each search when looking for a row's best match.
pub const PROBE_TOP_K: usize = 5;

/// v4's `note` for a lexical profile, byte for byte.
pub const LEXICAL_PROFILE_NOTE: &str = "Built-in (TF-IDF) profile: anchor lines are never embedded, so both sides embed the same text; the comparison is not measurable here. Decisions show what admission would do.";

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

/// One probed row (v4 `AnchorGateProbeRow`; key order = wire order — v4's
/// object literal puts `anchorFreeAction` BEFORE `anchoredAction`). The six
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
    /// What admission would do with the row, anchor-free.
    pub anchor_free_action: Option<AdmissionActionKind>,
    /// What admission would do with the row, anchored.
    pub anchored_action: Option<AdmissionActionKind>,
}

/// v4 `AnchorGateProbeBand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorGateProbeBand {
    /// Rows admission would fold into an older row (reinforce, absorb or
    /// promote).
    pub reinforce: usize,
    /// Rows whose fold target is a near-duplicate (absorbed, or a cold row
    /// promoted and absorbed).
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
    /// Rows admission would insert anchored but fold anchor-free.
    pub crossed_only_without_anchors: usize,
    pub embeddings_generated: usize,
    /// True when the profile is lexical (Built-in TF-IDF), which never embeds
    /// the anchor line.
    pub lexical_profile: bool,
    /// Why the anchored/anchor-free comparison is not meaningful, when it is
    /// not.
    pub note: Option<String>,
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

fn opt_str<'a>(memory: &'a Value, key: &str) -> Option<&'a str> {
    memory.get(key).and_then(Value::as_str)
}

/// The anchored embedded-text view of a row.
fn anchored_source(memory: &Value) -> EmbeddingTextSource<'_> {
    EmbeddingTextSource {
        summary: str_field(memory, "summary"),
        content: str_field(memory, "content"),
        occurred_at: opt_str(memory, "occurredAt"),
        narrative_time: opt_str(memory, "narrativeTime"),
        entities: memory
            .get("entities")
            .and_then(Value::as_array)
            .map_or(AnchorEntities::None, |a| AnchorEntities::Json(a.as_slice())),
    }
}

/// v4 `embeddingTextFor({ summary, content })` — no anchors, so no line.
fn anchor_free_text(memory: &Value) -> String {
    lexical_embedding_text_for(&EmbeddingTextSource {
        summary: str_field(memory, "summary"),
        content: str_field(memory, "content"),
        ..Default::default()
    })
}

/// v4 `hasAnchors(memory, timeZone)`: a TEXT inequality of the two renderings,
/// not a field-presence test (anchor fields that render no line are `false`).
fn has_anchors(memory: &Value, time_zone: &str) -> bool {
    embedding_text_for(&anchored_source(memory), time_zone) != anchor_free_text(memory)
}

/// The row's stored vector (the `Float32Array`-object shape `memories_read`
/// emits), or `None` when absent or malformed — v4's `neighbour.embedding`.
fn stored_vector(memory: &Value) -> Option<Vec<f32>> {
    let obj = memory.get("embedding")?.as_object()?;
    let mut out = vec![0f32; obj.len()];
    for (k, v) in obj {
        let i: usize = k.parse().ok()?;
        if i >= out.len() {
            return None;
        }
        out[i] = v.as_f64()? as f32;
    }
    Some(out)
}

/// One row's decisions, kept for the bands (the rows carry only the kinds).
#[derive(Default)]
struct RowDecisions {
    anchored: Option<AdmissionDecision>,
    anchor_free: Option<AdmissionDecision>,
}

/// v4 `folds(decision)`.
fn folds(decision: Option<&AdmissionDecision>) -> bool {
    decision.is_some_and(|d| admission_reinforced(&d.action))
}

/// v4 `band(pick)` over the rows' decisions.
fn band(
    decisions: &[RowDecisions],
    thresholds: &AdmissionThresholds,
    pick: impl Fn(&RowDecisions) -> Option<&AdmissionDecision>,
) -> AnchorGateProbeBand {
    AnchorGateProbeBand {
        reinforce: decisions.iter().filter(|d| folds(pick(d))).count(),
        near_duplicate: decisions
            .iter()
            .filter(|d| pick(d).is_some_and(|d| is_near_duplicate_decision(d, thresholds)))
            .count(),
    }
}

/// The run's mutable state — v4's closure-captured counters and maps.
struct ProbeState<'a, P> {
    provider: &'a P,
    input: &'a RunAnchorGateProbeInput,
    embeddings_generated: usize,
    /// The first embedding's provider (v4's `provider`), and the thresholds
    /// it selected.
    provider_name: Option<String>,
    thresholds: &'static AdmissionThresholds,
    time_zone: String,
    /// `Date.parse(createdAt)` per id; `None` is v4's `NaN`.
    created_at_by_id: HashMap<String, Option<i64>>,
    memory_cache: HashMap<String, Value>,
    anchor_free_cache: HashMap<String, Vec<f32>>,
}

impl<P: EmbeddingProvider + Sync> ProbeState<'_, P> {
    /// v4 `embed(text, lexicalText?)`: the counter moves FIRST, so a failed
    /// call counts; the first SUCCESS fixes the provider and the thresholds.
    async fn embed(
        &mut self,
        text: &str,
        lexical_text: Option<String>,
    ) -> Result<Vec<f32>, String> {
        self.embeddings_generated += 1;
        let mut opts = EmbeddingCallOptions::background();
        opts.lexical_text = lexical_text;
        let result = self
            .provider
            .generate_embedding_for_user(
                text,
                &self.input.user_id,
                self.input.embedding_profile_id.as_deref(),
                opts,
            )
            .await
            .map_err(|e| e.message)?;
        if self.provider_name.is_none() {
            self.provider_name = Some(result.provider.clone());
            self.thresholds = admission_thresholds_for_provider(Some(&result.provider));
        }
        Ok(result.embedding)
    }

    /// v4 `anchorFreeEmbedding(memory)`: per-id cache, filled on success
    /// only; no lexical text (the anchor-free text IS the lexical text).
    async fn anchor_free_embedding(&mut self, memory: &Value) -> Result<Vec<f32>, String> {
        let id = str_field(memory, "id").to_string();
        if let Some(cached) = self.anchor_free_cache.get(&id) {
            return Ok(cached.clone());
        }
        let vec = self.embed(&anchor_free_text(memory), None).await?;
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

    /// v4 `decide(memory, scored)` — admission's own decision for a row
    /// against scored older neighbours (no digest following).
    fn decide(&self, memory: &Value, scored: &[(String, f64)]) -> AdmissionDecision {
        let neighbours: Vec<AdmissionNeighbour> = scored
            .iter()
            .filter_map(|(id, cosine)| {
                self.memory_cache.get(id).map(|m| AdmissionNeighbour {
                    memory: m.clone(),
                    cosine: *cosine,
                    via: None,
                    dangling_digest: false,
                })
            })
            .collect();
        decide_admission(
            &AdmissionCandidateView {
                holder_id: str_field(memory, "characterId"),
                about_id: opt_str(memory, "aboutCharacterId"),
                kind: opt_str(memory, "kind"),
                source: opt_str(memory, "source"),
                occurred_at: opt_str(memory, "occurredAt"),
            },
            &neighbours,
            self.thresholds,
        )
    }

    /// The per-row body inside v4's `try` — mutates `row` (and the row's
    /// decisions) in place, so a throw keeps every field set before it.
    async fn probe_row(
        &mut self,
        db: &Db,
        store: &CharacterVectorStore,
        memory: &Value,
        anchored: bool,
        row: &mut AnchorGateProbeRow,
        decisions: &mut RowDecisions,
    ) -> Result<(), String> {
        let memory_id = row.memory_id.clone();
        let created_ms = js_date_parse_ms(&row.created_at);
        let source = anchored_source(memory);
        let anchored_vec = self
            .embed(
                &embedding_text_for(&source, &self.time_zone),
                Some(lexical_embedding_text_for(&source)),
            )
            .await?;
        // v4 `memoryIndex.shape(characterId, anchoredVec.length)`.
        if store.size() > 0 && store.count_with_dimensions(anchored_vec.len()) == 0 {
            tracing::warn!(
                target: LOG_TARGET,
                memoryId = %memory_id,
                probe = anchored_vec.len(),
                store = %store.dimensions().map_or("null".to_string(), |d| d.to_string()),
                "Probe embedding dimension does not match the store; skipping row"
            );
            return Ok(());
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
        let anchored_scores: Vec<(String, f64)> = older_anchored
            .iter()
            .map(|h| (h.id.clone(), h.score))
            .collect();
        let anchored_decision = self.decide(memory, &anchored_scores);
        row.anchored_action = Some(anchored_decision.action.kind());
        decisions.anchored = Some(anchored_decision);

        // Anchor-free side: re-embed the union of both searches' older
        // neighbours without their anchor lines and score like for like.
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
        let mut anchor_free_scores: Vec<(String, f64)> = Vec::new();
        for id in neighbour_ids {
            let Some(neighbour) = self.memory_cache.get(&id).cloned() else {
                continue;
            };
            let neighbour_vec = if has_anchors(&neighbour, &self.time_zone) {
                self.anchor_free_embedding(&neighbour).await?
            } else {
                match stored_vector(&neighbour).filter(|v| v.len() == free_vec.len()) {
                    Some(stored) => stored,
                    None => self.anchor_free_embedding(&neighbour).await?,
                }
            };
            let score = cosine_similarity(&free_vec, &neighbour_vec).map_err(|e| e.message())?;
            anchor_free_scores.push((id.clone(), score));
            if row.anchor_free_best.is_none_or(|best| score > best) {
                row.anchor_free_best = Some(score);
                row.anchor_free_best_id = Some(id);
            }
        }
        let anchor_free_decision = self.decide(memory, &anchor_free_scores);
        row.anchor_free_action = Some(anchor_free_decision.action.kind());
        decisions.anchor_free = Some(anchor_free_decision);
        Ok(())
    }
}

/// [`run_anchor_gate_probe_in_zone`] with the memory calendar's host fallback
/// `"UTC"` — the route's entry (its body is P4.D298's and calls this unchanged
/// signature; the HANDOFF in the P4.D290 lane record moves the route onto the
/// zoned sibling with the engine's calendar name). The instance zone
/// (`chat_settings.timezone`) is read first either way; only an instance with
/// none falls to the fallback, where v4 falls to its process zone.
pub async fn run_anchor_gate_probe<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    input: &RunAnchorGateProbeInput,
) -> Result<AnchorGateProbeResult, DbError> {
    run_anchor_gate_probe_in_zone(db, provider, input, "UTC").await
}

/// v4 `runAnchorGateProbe` with the memory calendar's host fallback
/// `host_zone_fallback` (the host's calendar NAME — core reads no ambient
/// zone, `host_zone_sites_census`). Reads run on the caller thread
/// (`db.read_main`), so every line here — the two WARNs, the INFO, and the
/// fallback reads' ERRORs — is capturable. The store is loaded once per run.
pub async fn run_anchor_gate_probe_in_zone<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    input: &RunAnchorGateProbeInput,
    host_zone_fallback: &str,
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
    // Anchor dates render on the instance's memory calendar, as admission's do.
    let time_zone =
        refresh_memory_time_zone(db, host_zone_fallback, crate::clock::now_unix_ms() as f64);
    let store = {
        let cid = input.character_id.clone();
        db.read_main(move |c| Ok(CharacterVectorStore::load(c, &cid)))?
    };

    let mut state = ProbeState {
        provider,
        input,
        embeddings_generated: 0,
        provider_name: None,
        thresholds: admission_thresholds_for_provider(None),
        time_zone,
        created_at_by_id: HashMap::new(),
        memory_cache: HashMap::new(),
        anchor_free_cache: HashMap::new(),
    };
    for m in &sample {
        state.remember(m.clone());
    }

    let mut rows: Vec<AnchorGateProbeRow> = Vec::with_capacity(sample.len());
    let mut decisions: Vec<RowDecisions> = Vec::with_capacity(sample.len());
    for memory in &sample {
        let anchored = has_anchors(memory, &state.time_zone);
        let mut row = AnchorGateProbeRow {
            memory_id: str_field(memory, "id").to_string(),
            summary: str_field(memory, "summary").to_string(),
            created_at: str_field(memory, "createdAt").to_string(),
            has_anchor_line: anchored,
            anchored_best: None,
            anchored_best_id: None,
            anchor_free_best: None,
            anchor_free_best_id: None,
            anchor_free_action: None,
            anchored_action: None,
        };
        let mut decided = RowDecisions::default();
        if let Err(message) = state
            .probe_row(db, &store, memory, anchored, &mut row, &mut decided)
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
        decisions.push(decided);
    }

    let thresholds = state.thresholds;
    let lexical = is_lexical_embedding_provider(state.provider_name.as_deref());
    let result = AnchorGateProbeResult {
        character_id: input.character_id.clone(),
        sampled: sample.len(),
        anchored_rows: rows.iter().filter(|r| r.has_anchor_line).count(),
        thresholds: AnchorGateProbeThresholds {
            reinforce: thresholds.reinforce,
            near_duplicate: thresholds.near_duplicate,
        },
        anchored: band(&decisions, thresholds, |d| d.anchored.as_ref()),
        anchor_free: band(&decisions, thresholds, |d| d.anchor_free.as_ref()),
        crossed_only_without_anchors: decisions
            .iter()
            .filter(|d| !folds(d.anchored.as_ref()) && folds(d.anchor_free.as_ref()))
            .count(),
        embeddings_generated: state.embeddings_generated,
        lexical_profile: lexical,
        note: lexical.then(|| LEXICAL_PROFILE_NOTE.to_string()),
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

    /// `hasAnchors` is a TEXT inequality: anchor fields that render no line
    /// (an unparseable date, blank entities) are not an anchor line.
    #[test]
    fn has_anchors_compares_the_two_renderings() {
        let plain = serde_json::json!({ "summary": "s", "content": "c" });
        assert!(!has_anchors(&plain, "UTC"));
        let dated = serde_json::json!({ "summary": "s", "content": "c", "occurredAt": "2026-06-03T06:00:00.000Z" });
        assert!(has_anchors(&dated, "UTC"));
        let blank = serde_json::json!({ "summary": "s", "content": "c", "occurredAt": "soon", "entities": ["  "] });
        assert!(!has_anchors(&blank, "UTC"));
    }

    /// The row and result serialize in v4's key orders (`anchorFreeAction`
    /// BEFORE `anchoredAction`; `lexicalProfile` and `note` after
    /// `embeddingsGenerated`), the nullable fields as `null`, an integral
    /// score the JS way, the actions as their wire kinds.
    #[test]
    fn rows_and_results_serialize_in_v4s_wire_shape() {
        let r = AnchorGateProbeRow {
            memory_id: "m".into(),
            summary: "s".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            has_anchor_line: true,
            anchored_best: Some(1.0),
            anchored_best_id: Some("n".into()),
            anchor_free_best: None,
            anchor_free_best_id: None,
            anchor_free_action: Some(AdmissionActionKind::PromoteAndReinforce),
            anchored_action: None,
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"memoryId":"m","summary":"s","createdAt":"2026-01-01T00:00:00.000Z","hasAnchorLine":true,"anchoredBest":1,"anchoredBestId":"n","anchorFreeBest":null,"anchorFreeBestId":null,"anchorFreeAction":"promote-and-reinforce","anchoredAction":null}"#
        );
        let band = AnchorGateProbeBand {
            reinforce: 0,
            near_duplicate: 0,
        };
        let result = AnchorGateProbeResult {
            character_id: "c".into(),
            sampled: 0,
            anchored_rows: 0,
            thresholds: AnchorGateProbeThresholds {
                reinforce: 0.4,
                near_duplicate: 0.8,
            },
            anchored: band,
            anchor_free: band,
            crossed_only_without_anchors: 0,
            embeddings_generated: 0,
            lexical_profile: true,
            note: Some(LEXICAL_PROFILE_NOTE.into()),
            rows: vec![],
        };
        assert_eq!(
            serde_json::to_string(&result).unwrap(),
            format!(
                r#"{{"characterId":"c","sampled":0,"anchoredRows":0,"thresholds":{{"reinforce":0.4,"nearDuplicate":0.8}},"anchored":{{"reinforce":0,"nearDuplicate":0}},"anchorFree":{{"reinforce":0,"nearDuplicate":0}},"crossedOnlyWithoutAnchors":0,"embeddingsGenerated":0,"lexicalProfile":true,"note":"{LEXICAL_PROFILE_NOTE}","rows":[]}}"#
            )
        );
    }

    /// A scripted provider: `vec_for(text)` → the vector or the failure, under
    /// the provider name given.
    struct Scripted<F> {
        f: F,
        provider: &'static str,
    }
    impl<F: Fn(&str) -> Result<Vec<f32>, String> + Sync> EmbeddingProvider for Scripted<F> {
        fn generate_embedding_for_user(
            &self,
            text: &str,
            _user_id: &str,
            _profile_id: Option<&str>,
            _opts: EmbeddingCallOptions,
        ) -> impl Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
            let out = (self.f)(text);
            let provider = self.provider.to_string();
            async move {
                out.map(|embedding| EmbeddingResult {
                    dimensions: embedding.len(),
                    embedding,
                    model: "scripted".into(),
                    provider,
                })
                .map_err(EmbeddingError::new)
            }
        }
    }

    fn scripted<F: Fn(&str) -> Result<Vec<f32>, String> + Sync>(f: F) -> Scripted<F> {
        Scripted {
            f,
            provider: "SCRIPTED",
        }
    }

    /// A provisioned throwaway instance; `memories` plant rows, each with a
    /// raw Float32 vector of `dims` when given.
    fn provisioned(memories: &[(&str, &str)], dims: Option<usize>) -> (tempfile::TempDir, Db) {
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
            if let Some(dims) = dims {
                let blob = crate::embedding_blob::float32_to_blob_raw(&vec![0.5f32; dims]);
                c.execute(
                    "UPDATE \"memories\" SET \"embedding\" = ?1 WHERE \"characterId\" = ?2",
                    rusqlite::params![blob, CHAR],
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

    /// The INFO line with v4's keys, the empty sample's NEURAL thresholds,
    /// `lexicalProfile: false`, `note: null` (no embedding ever set the
    /// provider), and the silence leg: neither WARN.
    #[test]
    fn an_empty_character_logs_only_the_completion_line_on_the_neural_scale() {
        let (_dir, db) = provisioned(&[], None);
        let (result, lines) = run(
            &db,
            &Scripted {
                f: |_: &str| Ok(vec![1.0]),
                provider: "BUILTIN",
            },
        );
        assert_eq!(result.sampled, 0);
        assert_eq!(result.embeddings_generated, 0);
        assert_eq!(result.thresholds.reinforce, 0.85);
        assert!(!result.lexical_profile);
        assert_eq!(result.note, None);
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

    /// The thresholds and the note come from the FIRST embedding's provider:
    /// a Built-in profile decides on the TF-IDF scale and says the comparison
    /// is not measurable.
    #[test]
    fn a_builtin_first_embedding_selects_the_tfidf_scale_and_the_note() {
        let (_dir, db) = provisioned(
            &[
                ("m-old", "2026-01-01T00:00:00.000Z"),
                ("m-new", "2026-01-02T00:00:00.000Z"),
            ],
            Some(2),
        );
        let (result, _) = run(
            &db,
            &Scripted {
                f: |_: &str| Ok(vec![0.6, 0.8]),
                provider: "BUILTIN",
            },
        );
        assert!(result.lexical_profile);
        assert_eq!(result.note.as_deref(), Some(LEXICAL_PROFILE_NOTE));
        assert_eq!(result.thresholds.reinforce, 0.4);
        assert_eq!(result.thresholds.near_duplicate, 0.8);
        // The newer row decides against the older one: the stored [0.5, 0.5]
        // scores 0.7 against [0.6, 0.8] — a REINFORCE on the TF-IDF scale
        // (≥ 0.40, < 0.80), where the neural scale would insert; the same on
        // both sides (no anchor line, so one text).
        let newer = result.rows.iter().find(|r| r.memory_id == "m-new").unwrap();
        assert_eq!(newer.anchored_action, Some(AdmissionActionKind::Reinforce));
        assert_eq!(
            newer.anchor_free_action,
            Some(AdmissionActionKind::Reinforce)
        );
        assert_eq!(result.anchored.reinforce, 1);
        assert_eq!(result.anchored.near_duplicate, 0);
        assert_eq!(result.crossed_only_without_anchors, 0);
        // The oldest row has no older neighbour: it would insert.
        let older = result.rows.iter().find(|r| r.memory_id == "m-old").unwrap();
        assert_eq!(older.anchored_action, Some(AdmissionActionKind::Insert));
    }

    /// The counter moves BEFORE the call, so a failed call counts; the row is
    /// pushed with its nulls and v4's WARN names the error.
    #[test]
    fn a_failed_embed_counts_and_warns_and_keeps_the_row() {
        let (_dir, db) = provisioned(&[("m-1", "2026-01-01T00:00:00.000Z")], None);
        let (result, lines) = run(&db, &scripted(|_: &str| Err("boom".to_string())));
        assert_eq!(result.sampled, 1);
        assert_eq!(result.embeddings_generated, 1, "counted before the call");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].anchored_best, None);
        assert_eq!(result.rows[0].anchored_action, None);
        assert_eq!(
            lines[0],
            format!("WARN {LOG_TARGET} Probe failed for row; continuing memoryId=m-1 error=boom")
        );
        assert!(!lines.iter().any(|l| l.contains("does not match the store")));
    }

    /// A non-empty store with NO entry of the probe's width skips the row
    /// (`store` = the dominant width); a store of mixed widths with ONE match
    /// proceeds, and an empty store (no stored vectors) proceeds too.
    #[test]
    fn the_shape_skip_needs_a_non_empty_store_with_no_matching_width() {
        let (_dir, db) = provisioned(&[("m-1", "2026-01-01T00:00:00.000Z")], Some(3));
        let (result, lines) = run(&db, &scripted(|_: &str| Ok(vec![0.6, 0.8])));
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.embeddings_generated, 1);
        assert_eq!(result.rows[0].anchored_action, None);
        assert_eq!(
            lines[0],
            format!(
                "WARN {LOG_TARGET} Probe embedding dimension does not match the store; skipping row \
                 memoryId=m-1 probe=2 store=3"
            )
        );
        assert!(!lines.iter().any(|l| l.contains("Probe failed for row")));

        // size == 0: the row proceeds (and decides an insert — nothing older).
        let (_d2, empty) = provisioned(&[("m-1", "2026-01-01T00:00:00.000Z")], None);
        let (result, lines) = run(&empty, &scripted(|_: &str| Ok(vec![0.6, 0.8])));
        assert_eq!(
            result.rows[0].anchored_action,
            Some(AdmissionActionKind::Insert)
        );
        assert!(!lines.iter().any(|l| l.contains("does not match the store")));
    }

    /// v4's `isOlder` for an unparseable `createdAt` (v4's `NaN`): nothing is
    /// older than it and it is older than nothing.
    #[test]
    fn an_unparseable_created_at_is_never_older() {
        let provider = scripted(|_: &str| Ok(vec![1.0]));
        let inp = input();
        let mut state = ProbeState {
            provider: &provider,
            input: &inp,
            embeddings_generated: 0,
            provider_name: None,
            thresholds: admission_thresholds_for_provider(None),
            time_zone: "UTC".into(),
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
    }
}
