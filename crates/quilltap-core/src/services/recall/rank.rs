//! `rankForRecall` — the one recall ranking (v4 `lib/memory/scoring/rank.ts`,
//! NEW in `584d28164`): the blend (`0.75 × relevance + 0.25 × salience`), the
//! bounded recall multipliers read from the targeting tags and the turn's
//! context (R1 gate, R2 cap — the table and the combine live in
//! [`crate::recall_tags`]), and R6's background reservation.
//!
//! It lives with recall (R-L2): it returns [`RecallCandidate`] and reads the
//! recall context. The blend itself is C1-A's
//! [`crate::memory_scoring::compute_ranking_blend`] — there is no second blend.
//!
//! v4's why, carried: "The constants do not move in the refactor (decision
//! 14); the probe fixture under `__tests__/fixtures/recall-probes/` pins them."
//!
//! Every sort here is V8's (`js_array_sort_by`): a NaN salience (an
//! unparseable `createdAt`) makes the comparators non-total, and Rust's own
//! `sort_by` would panic or permute differently.

use serde_json::Value;

use crate::memory_scoring::js_sort::js_array_sort_by;
use crate::memory_scoring::{compute_ranking_blend, salience, SalienceInput};
use crate::recall_tags::{combine_recall_multipliers, RecallRelevance};

use super::context::{row_keywords, tag_view, RecallContextInput};
use super::types::{RecallAdjustment, RecallCandidate};

/// A candidate ready to rank: its row, relevance, the raw cosine R1 gates on,
/// and salience (v4 `RankInput`).
#[derive(Clone, Debug)]
pub struct RankInput {
    pub memory: Value,
    /// Relevance (literal-boosted on the tool path).
    pub score: f64,
    /// The raw cosine (or text score) — the R1 gate's input.
    pub raw_score: f64,
    pub salience: f64,
    pub used_embedding: bool,
}

/// v4 `salience(memory, { nowMs })` over a net-JSON memory row: an absent
/// `importance` reads as NaN (v4's `undefined` arithmetic), an absent or
/// unparseable `createdAt` as NaN, `lastReinforcedAt` only when truthy.
pub fn salience_input_of(memory: &Value) -> SalienceInput {
    SalienceInput::from_iso(
        memory
            .get("importance")
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN),
        memory.get("reinforcedImportance").and_then(Value::as_f64),
        memory
            .get("createdAt")
            .and_then(Value::as_str)
            .unwrap_or(""),
        memory.get("lastReinforcedAt").and_then(Value::as_str),
    )
}

/// `salience(memory, { nowMs }).salience` for a row.
pub fn salience_of(memory: &Value, now_ms: f64) -> f64 {
    salience(&salience_input_of(memory), now_ms).salience
}

fn blended_after(c: &RecallCandidate) -> f64 {
    c.recall_adjustment
        .as_ref()
        .map(|a| a.blended_after)
        .unwrap_or(0.0)
}

/// v4 `byBlendedAfter` — descending by post-adjustment blended score, as the
/// comparator number V8 sorts on (`(b.blendedAfter ?? 0) - (a.blendedAfter ?? 0)`).
pub fn by_blended_after(a: &RecallCandidate, b: &RecallCandidate) -> f64 {
    blended_after(b) - blended_after(a)
}

/// Sort candidates by [`by_blended_after`], as V8 would (stable).
pub fn sort_by_blended_after(items: &mut [RecallCandidate]) {
    js_array_sort_by(items, by_blended_after);
}

/// v4 `rankForRecall` — rank candidates under a turn's recall context. Each
/// keeps its blend, takes the combined multiplier (gated on its own raw cosine
/// against the pool's best), and is sorted by the result; a cross-project
/// narrow row under the `exclude` policy is dropped. `extra_fired` labels
/// every row of this call (`related↗` for expansion, `bg↺` for R6).
pub fn rank_for_recall(
    candidates: &[RankInput],
    ctx: &RecallContextInput,
    best_cosine: f64,
    extra_fired: &[&str],
) -> Vec<RecallCandidate> {
    let view = ctx.view();
    let mut out: Vec<RecallCandidate> = Vec::new();
    for c in candidates {
        let blended_before = compute_ranking_blend(c.score, c.salience);
        let keywords = row_keywords(&c.memory);
        let adj = combine_recall_multipliers(
            &tag_view(&c.memory, &keywords),
            &view,
            Some(RecallRelevance {
                cosine: c.raw_score,
                best_cosine,
            }),
        );
        if adj.exclude {
            continue;
        }
        let mut fired = adj.fired;
        fired.extend(extra_fired.iter().map(|f| f.to_string()));
        out.push(RecallCandidate {
            memory: c.memory.clone(),
            score: c.score,
            used_embedding: c.used_embedding,
            salience: c.salience,
            recall_adjustment: Some(RecallAdjustment {
                multiplier: adj.multiplier,
                fired,
                blended_before,
                blended_after: blended_before * adj.multiplier,
            }),
        });
    }
    sort_by_blended_after(&mut out);
    out
}

/// v4 `rankByBlend` — rank without a recall context: the blend alone (the
/// tool, the API, Carina).
pub fn rank_by_blend(candidates: &[RankInput]) -> Vec<RecallCandidate> {
    let mut out: Vec<RecallCandidate> = candidates
        .iter()
        .map(|c| RecallCandidate {
            memory: c.memory.clone(),
            score: c.score,
            used_embedding: c.used_embedding,
            salience: c.salience,
            recall_adjustment: None,
        })
        .collect();
    js_array_sort_by(&mut out, |a, b| {
        compute_ranking_blend(b.score, b.salience) - compute_ranking_blend(a.score, a.salience)
    });
    out
}

/// v4 `reserveBackgroundSlots` — R6: reserve up to `fraction` of the head for
/// qualifying background rows. The head becomes the top `head_size − k`
/// ranked rows plus the top `k` background rows (`k` = reserve, or fewer when
/// fewer qualify — unused slots go back to the ranked rows), each part ranked
/// as before and the head re-sorted by score. Everything else follows in score
/// order. (`fraction` is schema-bounded to `[0, 0.5]`, so `head_size − k`
/// never goes negative.)
pub fn reserve_background_slots(
    ranked: Vec<RecallCandidate>,
    background: Vec<RecallCandidate>,
    head_size: usize,
    fraction: f64,
) -> Vec<RecallCandidate> {
    let reserve = (head_size as f64 * fraction).floor().max(0.0) as usize;
    let taken = reserve.min(background.len());
    if taken == 0 {
        return ranked;
    }
    // v4 `ranked.slice(0, headSize - taken)` / `.slice(headSize - taken)`.
    let cut = head_size.saturating_sub(taken).min(ranked.len());
    let mut ranked = ranked;
    let mut background = background;
    let ranked_tail = ranked.split_off(cut);
    let background_tail = background.split_off(taken);
    let mut head = ranked;
    head.extend(background);
    sort_by_blended_after(&mut head);
    let mut tail = ranked_tail;
    tail.extend(background_tail);
    sort_by_blended_after(&mut tail);
    head.extend(tail);
    head
}
