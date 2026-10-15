//! Stage 2 — gather: the candidate pool, as raw hits (v4
//! `lib/memory/recall/gather.ts`, `584d28164`).
//!
//! Two strategies yield the same [`RawHit`] shape, so every later stage
//! (floor, window, multipliers, expansion, reservation) applies to both:
//!
//! - **vector** — the index's top-K for the query, unioned with each
//!   retrospective probe's top-K (a memory keeps its best cosine), then the
//!   literal hits: the whole query on the tool path, the turn's entity anchors
//!   on the prompt paths (R4 picks the names that narrow the search). Anchor
//!   hits widen the pool only; they are scored at their raw cosine and face
//!   the floor like any row (R5, bug 202).
//! - **text** — when there is no usable vector (no embedding, or the index
//!   holds no vector of the query's length): phrase and word matches for the
//!   query and each probe, plus the anchors, scored by text match (bug 214).
//!   NO exclude / cold filter applies here — hydrate does it, for both
//!   strategies.
//!
//! Recall reads the index store through C1-3 ([`memory_index::search`] /
//! [`memory_index::shape`]) and never touches the vector tables.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::db::runtime::Db;
use crate::db::{memories_read, DbError};
use crate::embedding_vector::cosine_similarity;
use crate::literal_boost::get_literal_phrase;
use crate::memory_scoring::default_min_cosine_for_provider;
use crate::recall_tuning::{select_specific_anchors, ResolvedRecallTuning};
use crate::services::memory_index::{self, IndexSearchOptions};

use super::query::RecallQuery;
use super::types::{num, trace_stage, HitSource, RawHit, RecallStage, RecallTrace};

const MEMORY_LOG_TARGET: &str = "quilltap::memories";

/// v4 `GatherOptions`.
#[derive(Clone, Debug, Default)]
pub struct GatherOptions {
    /// How many the search will return; the vector top-K is three times this.
    pub limit: usize,
    pub include_cold: bool,
    pub exclude_ids: Option<HashSet<String>>,
    /// The tool path: union and boost verbatim matches of the whole query.
    pub literal_phrase: bool,
    /// R4 knobs, when a turn's tuning chooses specific anchors.
    pub tuning: Option<ResolvedRecallTuning>,
    /// Names of the characters present (R4 skips them as anchors).
    pub present_participant_names: Vec<String>,
    /// An explicit relevance floor; otherwise the embedding provider's default.
    pub min_score: Option<f64>,
    /// Force the text strategy (the replay's "what the fallback would do").
    pub force_text: bool,
}

/// Which strategy produced a pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecallStrategy {
    Vector,
    Text,
}

impl RecallStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            RecallStrategy::Vector => "vector",
            RecallStrategy::Text => "text",
        }
    }
}

/// v4 `Gathered`.
#[derive(Clone, Debug)]
pub struct Gathered {
    pub strategy: RecallStrategy,
    pub hits: Vec<RawHit>,
    /// The relevance floor `score` applies (`> 0` for text).
    pub min_score: f64,
    /// The embedding provider, when the vector strategy ran.
    pub provider: Option<String>,
    /// The phrase the tool path boosts, when it applies.
    pub literal_phrase: Option<String>,
    /// Ids the literal phrases matched; they take the boost when
    /// `literal_phrase` is set.
    pub literal_hit_ids: HashSet<String>,
}

/// One-time-per-key throttle for the dimension-mismatch warning (v4
/// `dimensionMismatchWarned`, process-lifetime). A corpus embedded with a
/// different profile than the one now searching degrades recall to text
/// search; said loudly, once per (character, dimension pair).
static DIMENSION_MISMATCH_WARNED: std::sync::LazyLock<std::sync::Mutex<HashSet<String>>> =
    std::sync::LazyLock::new(Default::default);

/// Stop words the text strategy ignores when matching word by word (v4
/// `TEXT_SEARCH_STOP_WORDS`).
static TEXT_SEARCH_STOP_WORDS: std::sync::LazyLock<HashSet<&'static str>> =
    std::sync::LazyLock::new(|| {
        [
            "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with",
            "by", "is", "was", "are", "were", "be", "been", "being", "have", "has", "had", "do",
            "does", "did", "will", "would", "could", "should", "may", "might", "can", "shall",
            "that", "this", "these", "those", "it", "its", "my", "your", "his", "her", "our",
            "their", "what", "which", "who", "whom", "how", "when", "where", "why", "not", "no",
            "nor", "if", "then", "than", "so", "as", "about", "from", "into", "up", "out", "off",
            "over", "under", "again", "before", "after", "between", "through",
        ]
        .into_iter()
        .collect()
    });

/// v4 `significantQueryWords` — `text.toLowerCase().split(/\s+/)` keeping
/// words longer than two UTF-16 units that are not stop words.
pub fn significant_query_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(crate::jsstr::is_js_ws)
        .filter(|w| crate::jsstr::utf16_len(w) > 2 && !TEXT_SEARCH_STOP_WORDS.contains(w))
        .map(str::to_string)
        .collect()
}

/// A row's string field, lower-cased (`""` when absent).
fn lower_field(memory: &Value, key: &str) -> String {
    memory
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase()
}

/// v4 `scoreTextMatch` — the text-match score in `[0, 1]` for one memory
/// against one query string.
pub fn score_text_match(memory: &Value, text: &str) -> f64 {
    let query_lower = text.to_lowercase();
    let query_words = significant_query_words(text);
    let mut score = 0.0_f64;
    let content_lower = lower_field(memory, "content");
    let summary_lower = lower_field(memory, "summary");

    if summary_lower.contains(&query_lower) {
        score += 0.4;
    }
    if content_lower.contains(&query_lower) {
        score += 0.3;
    }
    if !query_words.is_empty() {
        let content_word_matches = query_words
            .iter()
            .filter(|w| content_lower.contains(w.as_str()))
            .count();
        let summary_word_matches = query_words
            .iter()
            .filter(|w| summary_lower.contains(w.as_str()))
            .count();
        score += 0.2 * (content_word_matches as f64 / query_words.len() as f64);
        score += 0.1 * (summary_word_matches as f64 / query_words.len() as f64);
    }

    let keywords = super::context::row_keywords(memory);
    let matching_keywords = keywords
        .iter()
        .filter(|kw| {
            let kwl = kw.to_lowercase();
            query_words.iter().any(|qw| kwl.contains(qw.as_str()))
        })
        .count();
    score += 0.1 * (matching_keywords as f64 / (keywords.len().max(1) as f64));
    score.min(1.0)
}

/// v4 `cleanAnchors` — trimmed, at least two UTF-16 units.
fn clean_anchors(anchors: &[String]) -> Vec<String> {
    anchors
        .iter()
        .map(|a| crate::jsstr::js_trim(a).to_string())
        .filter(|p| crate::jsstr::utf16_len(p) >= 2)
        .collect()
}

/// Decode the `{"0":v0,…}` Float32Array-shaped embedding a memory row carries
/// (v4's `memory.embedding`). Ascending integer-key order; `None` when absent.
pub fn memory_embedding(memory: &Value) -> Option<Vec<f32>> {
    let obj = memory.get("embedding").and_then(Value::as_object)?;
    if obj.is_empty() {
        return None;
    }
    let mut pairs: Vec<(usize, f32)> = obj
        .iter()
        .filter_map(|(k, val)| Some((k.parse::<usize>().ok()?, val.as_f64()? as f32)))
        .collect();
    pairs.sort_by_key(|(i, _)| *i);
    Some(pairs.into_iter().map(|(_, f)| f).collect())
}

fn is_cold(memory: &Value) -> bool {
    memory.get("tier").and_then(Value::as_str) == Some("cold")
}

async fn search_by_content(
    db: &Db,
    character_id: &str,
    phrase: &str,
) -> Result<Vec<Value>, DbError> {
    let cid = character_id.to_string();
    let ph = phrase.to_string();
    db.read_main(move |conn| memories_read::search_by_content(conn, &cid, &ph))
}

/// v4 `gather` — gather the candidate pool for `query`. The vector strategy
/// runs when the query has an embedding and the index holds vectors of its
/// length; otherwise the text strategy runs.
pub async fn gather(
    db: &Db,
    character_id: &str,
    query: &RecallQuery,
    opts: &GatherOptions,
    trace: &mut RecallTrace,
    label: Option<&str>,
) -> Result<Gathered, DbError> {
    let embedding = match &query.embedding {
        Some(e) if !opts.force_text => e,
        Some(_) => return gather_text(db, character_id, query, opts, trace, label, "forced").await,
        None => {
            return gather_text(db, character_id, query, opts, trace, label, "no embedding").await
        }
    };
    let query_dims = embedding.embedding.len();

    let index_shape = {
        let cid = character_id.to_string();
        db.read_main(move |conn| Ok(memory_index::shape(conn, &cid, query_dims)))?
    };
    if index_shape.size > 0 && index_shape.matching == 0 {
        // v4 `${dominantDimensions}` — the store is non-empty, so a dominant
        // length always exists here.
        let dominant = index_shape
            .dominant_dimensions
            .map(|d| d.to_string())
            .unwrap_or_else(|| "null".to_string());
        let warn_key = format!("{character_id}:{dominant}->{query_dims}");
        let first = DIMENSION_MISMATCH_WARNED
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(warn_key);
        if first {
            tracing::warn!(
                target: MEMORY_LOG_TARGET,
                characterId = %character_id,
                query = %crate::jsstr::utf16_truncate(&query.text, 100),
                storedDimensions = index_shape.dominant_dimensions.unwrap_or_default(),
                queryDimensions = query_dims,
                provider = %embedding.provider,
                "[Memory] Embedding dimension mismatch — recall is degraded to TEXT search (embeddings ignored). Reindex this character or restore the matching embedding profile."
            );
        }
        return gather_text(
            db,
            character_id,
            query,
            opts,
            trace,
            label,
            "dimension mismatch",
        )
        .await;
    }

    let excluded: HashSet<String> = opts.exclude_ids.clone().unwrap_or_default();
    let include_cold = opts.include_cold;
    let search_limit = opts.limit.saturating_mul(3);

    let index_search = |vector: Vec<f32>| {
        let cid = character_id.to_string();
        let exclude = excluded.clone();
        db.read_main(move |conn| {
            Ok(memory_index::search(
                conn,
                &cid,
                &vector,
                IndexSearchOptions {
                    hot_only: !include_cold,
                    exclude: Some(&exclude),
                    limit: search_limit,
                },
            ))
        })
    };

    let mut hits: Vec<RawHit> = index_search(embedding.embedding.clone())?
        .into_iter()
        .map(|vr| RawHit {
            id: vr.id,
            score: vr.score,
            via: HitSource::Vector,
            memory: None,
        })
        .collect();
    let vector_count = hits.len();

    // Probes: each memory keeps its best cosine; new ids follow in probe order.
    let mut probe_added = 0usize;
    let usable_probes: Vec<&Vec<f32>> = query
        .probes
        .iter()
        .filter_map(|p| p.embedding.as_ref())
        .filter(|e| e.embedding.len() == query_dims)
        .map(|e| &e.embedding)
        .collect();
    if !usable_probes.is_empty() {
        // JS `Map` semantics: insertion order kept; a better score re-sets in
        // place (keeping the existing `via`).
        let mut order: Vec<String> = Vec::new();
        let mut by_id: HashMap<String, RawHit> = HashMap::new();
        for h in hits.drain(..) {
            if !by_id.contains_key(&h.id) {
                order.push(h.id.clone());
            }
            by_id.insert(h.id.clone(), h);
        }
        for probe in &usable_probes {
            for vr in index_search((*probe).clone())? {
                match by_id.get_mut(&vr.id) {
                    None => {
                        probe_added += 1;
                        order.push(vr.id.clone());
                        by_id.insert(
                            vr.id.clone(),
                            RawHit {
                                id: vr.id,
                                score: vr.score,
                                via: HitSource::Probe,
                                memory: None,
                            },
                        );
                    }
                    Some(existing) => {
                        if vr.score > existing.score {
                            existing.score = vr.score;
                        }
                    }
                }
            }
        }
        hits = order
            .into_iter()
            .filter_map(|id| by_id.remove(&id))
            .collect();
    }

    // Literal hits: the whole query (tool path) and the turn's entity anchors.
    let literal_phrase = if opts.literal_phrase {
        get_literal_phrase(Some(&query.text))
    } else {
        None
    };
    let mut anchor_phrases: Vec<String> = Vec::new();
    if literal_phrase.is_some() {
        anchor_phrases.push(crate::jsstr::js_trim(&query.text).to_string());
    }
    let entity_phrases = clean_anchors(&query.anchors);
    // Content hits per phrase, fetched once — excluded ids and (unless asked
    // for) cold rows already dropped.
    let mut content_hits: HashMap<String, Vec<Value>> = HashMap::new();
    async fn hits_for(
        db: &Db,
        character_id: &str,
        cache: &mut HashMap<String, Vec<Value>>,
        excluded: &HashSet<String>,
        include_cold: bool,
        phrase: &str,
    ) -> Result<Vec<Value>, DbError> {
        if let Some(found) = cache.get(phrase) {
            return Ok(found.clone());
        }
        let found: Vec<Value> = search_by_content(db, character_id, phrase)
            .await?
            .into_iter()
            .filter(|m| {
                !excluded.contains(super::types::memory_id(m)) && (include_cold || !is_cold(m))
            })
            .collect();
        cache.insert(phrase.to_string(), found.clone());
        Ok(found)
    }

    let chosen_anchors: Vec<String> = match opts.tuning.as_ref().filter(|t| t.specific_anchors) {
        Some(t) => {
            // R4 — prefer the names that narrow the search over the first three.
            let mut counted: Vec<(String, usize)> = Vec::new();
            for phrase in &entity_phrases {
                let n = hits_for(
                    db,
                    character_id,
                    &mut content_hits,
                    &excluded,
                    include_cold,
                    phrase,
                )
                .await?
                .len();
                counted.push((phrase.clone(), n));
            }
            let chosen = select_specific_anchors(
                &counted,
                &opts.present_participant_names,
                3,
                Some(t.anchor_min_hits),
                Some(t.anchor_order),
            );
            let counted_json = serde_json::to_string(
                &counted
                    .iter()
                    .map(|(phrase, count)| serde_json::json!({"phrase": phrase, "count": count}))
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_default();
            tracing::debug!(
                target: MEMORY_LOG_TARGET,
                characterId = %character_id,
                countedJson = %counted_json,
                chosenJson = %serde_json::to_string(&chosen).unwrap_or_default(),
                "[Memory] Specific entity anchors chosen"
            );
            chosen
        }
        None => entity_phrases.iter().take(3).cloned().collect(),
    };
    anchor_phrases.extend(chosen_anchors.iter().cloned());

    let mut literal_hit_ids: HashSet<String> = HashSet::new();
    let mut anchor_added = 0usize;
    if !anchor_phrases.is_empty() {
        let mut direct: Vec<Value> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for phrase in &anchor_phrases {
            for m in hits_for(
                db,
                character_id,
                &mut content_hits,
                &excluded,
                include_cold,
                phrase,
            )
            .await?
            {
                let id = super::types::memory_id(&m).to_string();
                literal_hit_ids.insert(id.clone());
                if seen.insert(id) {
                    direct.push(m);
                }
            }
        }
        let in_pool: HashSet<String> = hits.iter().map(|h| h.id.clone()).collect();
        for memory in direct {
            let id = super::types::memory_id(&memory).to_string();
            if in_pool.contains(&id) {
                continue;
            }
            if let Some(emb) = memory_embedding(&memory) {
                if emb.len() == query_dims {
                    if let Ok(score) = cosine_similarity(&embedding.embedding, &emb) {
                        hits.push(RawHit {
                            id,
                            score,
                            via: if literal_phrase.is_some() {
                                HitSource::Literal
                            } else {
                                HitSource::Anchor
                            },
                            memory: Some(memory),
                        });
                        anchor_added += 1;
                    }
                }
            }
        }
    }

    let gathered = Gathered {
        strategy: RecallStrategy::Vector,
        min_score: opts
            .min_score
            .unwrap_or_else(|| default_min_cosine_for_provider(Some(&embedding.provider))),
        provider: Some(embedding.provider.clone()),
        literal_phrase: literal_phrase.clone(),
        literal_hit_ids,
        hits,
    };
    let mut detail = Map::new();
    detail.insert("strategy".into(), Value::from("vector"));
    detail.insert("vector".into(), num(vector_count as f64));
    detail.insert("probes".into(), num(usable_probes.len() as f64));
    detail.insert("probeAdded".into(), num(probe_added as f64));
    detail.insert(
        "anchors".into(),
        Value::Array(chosen_anchors.into_iter().map(Value::String).collect()),
    );
    detail.insert("anchorAdded".into(), num(anchor_added as f64));
    detail.insert("literal".into(), Value::Bool(literal_phrase.is_some()));
    detail.insert("pool".into(), num(gathered.hits.len() as f64));
    trace_stage(trace, RecallStage::Gather, detail, label);
    Ok(gathered)
}

/// v4 `gatherText` — the text strategy: phrase hits for the query and each
/// probe, word hits when a phrase finds fewer than `limit`, and the anchors
/// (which widen the pool but still need a non-zero text score). Each row
/// scores the best of its text matches against the query and every probe
/// (bug 214).
async fn gather_text(
    db: &Db,
    character_id: &str,
    query: &RecallQuery,
    opts: &GatherOptions,
    trace: &mut RecallTrace,
    label: Option<&str>,
    reason: &str,
) -> Result<Gathered, DbError> {
    let probes: Vec<String> = std::iter::once(query.text.clone())
        .chain(query.probes.iter().map(|p| p.text.clone()))
        .filter(|p| !crate::jsstr::js_trim(p).is_empty())
        .collect();
    let mut order: Vec<String> = Vec::new();
    let mut pool: HashMap<String, Value> = HashMap::new();
    let mut add_all = |found: Vec<Value>, order: &mut Vec<String>| {
        for m in found {
            let id = super::types::memory_id(&m).to_string();
            if let std::collections::hash_map::Entry::Vacant(slot) = pool.entry(id) {
                order.push(slot.key().clone());
                slot.insert(m);
            }
        }
    };
    for probe in &probes {
        let phrase_hits = search_by_content(db, character_id, probe).await?;
        let phrase_count = phrase_hits.len();
        add_all(phrase_hits, &mut order);
        if phrase_count < opts.limit {
            for word in significant_query_words(probe) {
                let found = search_by_content(db, character_id, &word).await?;
                add_all(found, &mut order);
            }
        }
    }
    let anchors: Vec<String> = clean_anchors(&query.anchors).into_iter().take(3).collect();
    for phrase in &anchors {
        let found = search_by_content(db, character_id, phrase).await?;
        add_all(found, &mut order);
    }

    let hits: Vec<RawHit> = order
        .into_iter()
        .filter_map(|id| pool.remove(&id))
        .map(|memory| {
            // `Math.max(...probes.map(...))` — `-Infinity` for no probes.
            let score = probes
                .iter()
                .map(|probe| score_text_match(&memory, probe))
                .fold(f64::NEG_INFINITY, crate::jsnum::math_max);
            RawHit {
                id: super::types::memory_id(&memory).to_string(),
                score,
                via: HitSource::Text,
                memory: Some(memory),
            }
        })
        .collect();
    let mut detail = Map::new();
    detail.insert("strategy".into(), Value::from("text"));
    detail.insert("reason".into(), Value::from(reason));
    detail.insert("probes".into(), num(probes.len() as f64));
    detail.insert(
        "anchors".into(),
        Value::Array(anchors.into_iter().map(Value::String).collect()),
    );
    detail.insert("pool".into(), num(hits.len() as f64));
    trace_stage(trace, RecallStage::Gather, detail, label);
    Ok(Gathered {
        strategy: RecallStrategy::Text,
        hits,
        min_score: 0.0,
        provider: None,
        literal_phrase: None,
        literal_hit_ids: HashSet::new(),
    })
}
