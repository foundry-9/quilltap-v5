//! The recall search: gather → hydrate → score → select over one
//! [`RecallQuery`] (v4 `lib/memory/recall/search.ts`). Every path that
//! searches a character's memories — the turn's head, the proactive pass, the
//! search tool, the Memories API, Carina, the voice rehearsals, the greeting,
//! the replay — runs these same stages with different options.
//!
//! Nothing here stamps `lastAccessedAt`: what a search returns is not what a
//! model saw. `deliver` stamps what was delivered.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::db::runtime::Db;
use crate::db::DbError;
use crate::model::embedding::EmbeddingProvider;
use crate::recall_tags::TimeWindow;

use super::context::RecallContextInput;
use super::gather::{gather, GatherOptions, RecallStrategy};
use super::hydrate::{hydrate, hydrate_neighbours, HydrateFilters};
use super::query::{build_recall_query, BuildRecallQueryInput, EmbeddingMemo, RecallQuery};
use super::score::{score_gathered, ScoreOptions};
use super::select::{select_ranked, wants_expansion, SelectOptions};
use super::types::{trace_stage, RecallCandidate, RecallStage, RecallTrace};

const MEMORY_LOG_TARGET: &str = "quilltap::memories";

/// v4 `RecallSearchOptions`.
#[derive(Clone, Debug, Default)]
pub struct RecallSearchOptions {
    /// How many to return (v4 `options.limit || 20` — `None` or `0` → 20).
    pub limit: Option<usize>,
    /// An explicit relevance floor; otherwise the embedding provider's
    /// default. `Some(0.0)` means none.
    pub min_score: Option<f64>,
    pub min_importance: Option<f64>,
    /// `'AUTO' | 'MANUAL' | 'CONSOLIDATED'`.
    pub source: Option<String>,
    /// Only rows about this character.
    pub about_character_id: Option<String>,
    /// Include cold-tier rows (the tool and the Commonplace Book UI).
    pub include_cold: bool,
    /// Ids to leave out of every source (the replay's `asOf`).
    pub exclude_ids: Option<HashSet<String>>,
    /// The tool path: union and boost verbatim matches of the whole query.
    pub literal_phrase: bool,
    /// The turn's recall context (prompt paths). Absent → the blend alone.
    pub recall_context: Option<RecallContextInput>,
    /// The head the caller will take — R6 reads it.
    pub head_size: Option<usize>,
    /// The salience clock, ms. v4's absent → `Date.now()`; v5 callers always
    /// pass their injected clock.
    pub now_ms: f64,
    /// Run the text strategy whatever the query holds (the replay).
    pub force_text: bool,
}

/// v4 `RecallSearchResult`.
#[derive(Clone, Debug)]
pub struct RecallSearchResult {
    pub candidates: Vec<RecallCandidate>,
    /// Every ranked row before the cut to `limit`. The inter-character
    /// relevance half filters this by subject: the same search, read past the
    /// head's cut.
    pub ranked: Vec<RecallCandidate>,
    pub strategy: RecallStrategy,
}

struct Run {
    result: RecallSearchResult,
    empty: bool,
}

async fn run(
    db: &Db,
    character_id: &str,
    query: &RecallQuery,
    options: &RecallSearchOptions,
    force_text: bool,
    trace: &mut RecallTrace,
    label: Option<&str>,
) -> Result<Run, DbError> {
    let limit = options.limit.filter(|l| *l > 0).unwrap_or(20);
    let ctx = options.recall_context.as_ref();
    let filters = HydrateFilters {
        include_cold: options.include_cold,
        exclude_ids: options.exclude_ids.clone(),
        min_importance: options.min_importance,
        source: options.source.clone(),
        about_character_id: options.about_character_id.clone(),
    };
    let gather_options = GatherOptions {
        limit,
        include_cold: options.include_cold,
        exclude_ids: options.exclude_ids.clone(),
        literal_phrase: options.literal_phrase,
        tuning: ctx.map(|c| c.tuning_of().clone()),
        present_participant_names: ctx
            .map(|c| c.present_participant_names.clone())
            .unwrap_or_default(),
        min_score: options.min_score,
        force_text,
    };
    let gathered = gather(db, character_id, query, &gather_options, trace, label).await?;
    if gathered.strategy == RecallStrategy::Vector && gathered.hits.is_empty() {
        return Ok(Run {
            result: RecallSearchResult {
                candidates: Vec::new(),
                ranked: Vec::new(),
                strategy: RecallStrategy::Vector,
            },
            empty: true,
        });
    }
    let hydrated = hydrate(db, &gathered, &filters, trace, label).await?;
    let scored = score_gathered(
        &hydrated,
        &gathered,
        &ScoreOptions {
            limit,
            recall_context: options.recall_context.clone(),
            window: query.window.clone(),
            now_ms: options.now_ms,
        },
        trace,
        label,
    );
    let neighbours = if wants_expansion(&scored) {
        hydrate_neighbours(db, &scored.ranked, limit, character_id, &filters).await?
    } else {
        Vec::new()
    };
    let selected = select_ranked(
        &scored,
        &neighbours,
        &SelectOptions {
            limit,
            head_size: options.head_size,
            query_embedding: query.embedding.as_ref().map(|e| e.embedding.as_slice()),
            now_ms: options.now_ms,
        },
        trace,
        label,
    );
    Ok(Run {
        result: RecallSearchResult {
            candidates: selected.candidates,
            ranked: selected.ranked,
            strategy: gathered.strategy,
        },
        empty: false,
    })
}

/// v4 `String(error)` for a DB failure — the `Error` prefix v4's thrown
/// errors carry.
fn error_string(e: &DbError) -> String {
    format!("Error: {e}")
}

/// v4 `searchRecall` — run the search stages for a built query. Never fails
/// for a vector failure: the text strategy answers (WARN `[Memory] Semantic
/// search failed, falling back to text search`). A failure of the TEXT run
/// itself propagates, as v4's throw does.
pub async fn search_recall(
    db: &Db,
    character_id: &str,
    query: &RecallQuery,
    options: &RecallSearchOptions,
    trace: &mut RecallTrace,
    label: Option<&str>,
) -> Result<RecallSearchResult, DbError> {
    if !options.force_text && query.embedding.is_some() {
        match run(db, character_id, query, options, false, trace, label).await {
            Ok(vector) if !vector.empty => return Ok(vector.result),
            Ok(_) => {
                // An index with nothing to offer (no vectors yet): the text
                // strategy may.
                let mut detail = Map::new();
                detail.insert("strategy".into(), Value::from("vector"));
                detail.insert("pool".into(), Value::from(0));
                detail.insert("fallback".into(), Value::from("text"));
                trace_stage(trace, RecallStage::Gather, detail, label);
            }
            Err(error) => {
                let error = error_string(&error);
                tracing::warn!(
                    target: MEMORY_LOG_TARGET,
                    characterId = %character_id,
                    query = %crate::jsstr::utf16_truncate(&query.text, 100),
                    error = %error,
                    "[Memory] Semantic search failed, falling back to text search"
                );
                let mut detail = Map::new();
                detail.insert("strategy".into(), Value::from("vector"));
                detail.insert("failed".into(), Value::String(error));
                detail.insert("fallback".into(), Value::from("text"));
                trace_stage(trace, RecallStage::Gather, detail, label);
            }
        }
    }
    if crate::jsstr::js_trim(&query.text).is_empty() {
        return Ok(RecallSearchResult {
            candidates: Vec::new(),
            ranked: Vec::new(),
            strategy: RecallStrategy::Text,
        });
    }
    let text = run(db, character_id, query, options, true, trace, label).await?;
    Ok(RecallSearchResult {
        strategy: RecallStrategy::Text,
        ..text.result
    })
}

/// v4 `SearchMemoriesOptions` — the search's options, plus the query's own.
#[derive(Clone, Debug, Default)]
pub struct SearchMemoriesOptions {
    pub search: RecallSearchOptions,
    pub user_id: String,
    pub embedding_profile_id: Option<String>,
    /// Event-time window (a plain filter without a recall context).
    pub window: Option<TimeWindow>,
    /// Entity anchors unioned into the pool.
    pub anchors: Vec<String>,
    /// Extra embedding probes (≤ 2): each one's top-K joins the pool, a row
    /// keeping its best cosine.
    pub probes: Option<Vec<String>>,
    pub embedding_memo: Option<EmbeddingMemo>,
}

/// v4 `searchMemories` — embed `text` once and search: the one-shot form for
/// a path with a ready-made question and no turn signals (the tool, the
/// Memories API, Carina, the voice rehearsals, the greeting).
pub async fn search_memories<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    character_id: &str,
    text: &str,
    options: &SearchMemoriesOptions,
) -> Result<Vec<RecallCandidate>, DbError> {
    let mut trace: RecallTrace = Vec::new();
    let query = build_recall_query(
        embedding,
        BuildRecallQueryInput {
            embedding_memo: options.embedding_memo.clone(),
            probes: options.probes.clone(),
            ..BuildRecallQueryInput::plain(
                &options.user_id,
                options.embedding_profile_id.as_deref(),
                text,
            )
        },
        &mut trace,
    )
    .await;
    let with_extras = RecallQuery {
        window: options.window.clone(),
        anchors: options.anchors.clone(),
        ..query
    };
    let result = search_recall(
        db,
        character_id,
        &with_extras,
        &options.search,
        &mut trace,
        None,
    )
    .await?;
    Ok(result.candidates)
}
