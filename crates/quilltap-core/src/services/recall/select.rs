//! Stage 5 — select: related expansion, R6's background reservation, the cut
//! (v4 `lib/memory/recall/select.ts`). Pure.
//!
//! Expansion (item 5) catches the memory that is relevant by association but
//! did not clear the floor on its own: the neighbours `hydrate_neighbours`
//! read are scored against the same query vector, ranked under the same
//! context (gated on their own cosine) and merged. The floor is deliberately
//! not re-applied to them — association is the point.

use serde_json::{Map, Value};

use crate::embedding_vector::cosine_similarity;
use crate::recall_tags::boost_gate_threshold;

use super::gather::{memory_embedding, RecallStrategy};
use super::rank::{
    rank_for_recall, reserve_background_slots, salience_of, sort_by_blended_after, RankInput,
};
use super::score::Scored;
use super::types::{num, trace_stage, RecallCandidate, RecallStage, RecallTrace};

/// v4 `SelectOptions`.
#[derive(Clone, Debug)]
pub struct SelectOptions<'a> {
    pub limit: usize,
    /// The head the caller will take — only R6 reads it.
    pub head_size: Option<usize>,
    /// The query vector expansion scores neighbours against.
    pub query_embedding: Option<&'a [f32]>,
    pub now_ms: f64,
}

/// v4 `wantsExpansion` — expansion runs for a vector search under a context
/// that asks for it.
pub fn wants_expansion(scored: &Scored) -> bool {
    scored.strategy == RecallStrategy::Vector
        && scored.context.as_ref().is_some_and(|c| c.expand_related)
}

/// v4 `Selected` — what `select` keeps, and the whole ranked list it cut it
/// from (expansion and R6 applied).
#[derive(Clone, Debug)]
pub struct Selected {
    pub candidates: Vec<RecallCandidate>,
    pub ranked: Vec<RecallCandidate>,
}

/// v4 `selectRanked`.
pub fn select_ranked(
    scored: &Scored,
    neighbours: &[Value],
    opts: &SelectOptions<'_>,
    trace: &mut RecallTrace,
    label: Option<&str>,
) -> Selected {
    let mut results: Vec<RecallCandidate> = scored.ranked.clone();
    let ctx = scored.context.as_ref();
    let mut expanded = 0usize;
    if let (Some(ctx), Some(query)) = (ctx, opts.query_embedding) {
        if wants_expansion(scored) && !neighbours.is_empty() {
            let scorable: Vec<RankInput> = neighbours
                .iter()
                .filter_map(|m| {
                    let emb = memory_embedding(m)?;
                    if emb.len() != query.len() {
                        return None;
                    }
                    let cosine = cosine_similarity(query, &emb).ok()?;
                    Some(RankInput {
                        memory: m.clone(),
                        score: cosine,
                        raw_score: cosine,
                        used_embedding: true,
                        salience: salience_of(m, opts.now_ms),
                    })
                })
                .collect();
            let survivors = rank_for_recall(&scorable, ctx, scored.best_cosine, &["related↗"]);
            expanded = survivors.len();
            if !survivors.is_empty() {
                results.extend(survivors);
                sort_by_blended_after(&mut results);
            }
        }
    }

    // R6 — when the window filtered hard, keep room in the head for
    // out-of-window background that clears the gate at full strength.
    let mut reserved = 0usize;
    if let Some(ctx) = ctx {
        let tuning = ctx.tuning_of();
        if let (Some(gate), Some(head_size)) = (tuning.boost_gate.as_ref(), opts.head_size) {
            // v4 `opts.headSize` is truthy-tested: 0 disables R6.
            if head_size > 0 && tuning.background_reserve > 0.0 && !scored.out_of_window.is_empty()
            {
                let threshold = boost_gate_threshold(gate, scored.best_cosine);
                let present: std::collections::HashSet<&str> =
                    results.iter().map(|r| r.id()).collect();
                let qualifying: Vec<RankInput> = scored
                    .out_of_window
                    .iter()
                    .filter(|c| {
                        !present.contains(super::types::memory_id(&c.memory))
                            && c.raw_score >= threshold
                    })
                    .cloned()
                    .collect();
                let background = rank_for_recall(&qualifying, ctx, scored.best_cosine, &["bg↺"]);
                reserved = ((head_size as f64 * tuning.background_reserve).floor() as usize)
                    .min(background.len());
                results = reserve_background_slots(
                    results,
                    background,
                    head_size,
                    tuning.background_reserve,
                );
            }
        }
    }

    let cut = results.len().saturating_sub(opts.limit);
    let candidates: Vec<RecallCandidate> = results.iter().take(opts.limit).cloned().collect();
    let mut detail = Map::new();
    detail.insert("expanded".into(), num(expanded as f64));
    detail.insert("reserved".into(), num(reserved as f64));
    detail.insert("cut".into(), num(cut as f64));
    detail.insert("returned".into(), num(candidates.len() as f64));
    trace_stage(trace, RecallStage::Select, detail, label);
    Selected {
        candidates,
        ranked: results,
    }
}

/// v4 `select` — [`select_ranked`]'s cut alone.
pub fn select(
    scored: &Scored,
    neighbours: &[Value],
    opts: &SelectOptions<'_>,
    trace: &mut RecallTrace,
    label: Option<&str>,
) -> Vec<RecallCandidate> {
    select_ranked(scored, neighbours, opts, trace, label).candidates
}
