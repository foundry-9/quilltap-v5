//! Stage 4 — score: relevance · salience · multipliers (v4
//! `lib/memory/recall/score.ts`). Pure.
//!
//! Applies the relevance floor (on the BOOSTED score, before the blend, so a
//! low-relevance row cannot ride its importance in), measures salience on the
//! search clock, stages the event-time window and ranks through
//! [`rank_for_recall`].
//!
//! The window is two-stage on the prompt paths (a recall context is present):
//! hard when at least `limit` hits fall inside it, otherwise the bounded
//! ×`occurredWithinWindow` boost on the whole pool — never fewer results than
//! an unwindowed search. Without a recall context (the tool) it is a plain
//! filter.

use serde_json::{Map, Value};

use crate::embedding_scale::is_lexical_embedding_provider;
use crate::memory_scoring::date_ms;
use crate::recall_tags::TimeWindow;
use crate::recall_tuning::ResolvedRecallTuning;

use super::context::RecallContextInput;
use super::gather::{Gathered, RecallStrategy};
use super::rank::{rank_by_blend, rank_for_recall, salience_of, RankInput};
use super::types::{num, trace_stage, HydratedHit, RecallCandidate, RecallStage, RecallTrace};

/// v4 `ScoreOptions`.
#[derive(Clone, Debug)]
pub struct ScoreOptions {
    pub limit: usize,
    /// The turn's recall context. Absent → the blend alone, window as a filter.
    pub recall_context: Option<RecallContextInput>,
    pub window: Option<TimeWindow>,
    /// The salience clock (the replay pins it to the replayed turn).
    pub now_ms: f64,
}

/// v4 `Scored['windowMode']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMode {
    None,
    Filter,
    Hard,
    Soft,
}

impl WindowMode {
    pub fn as_str(self) -> &'static str {
        match self {
            WindowMode::None => "none",
            WindowMode::Filter => "filter",
            WindowMode::Hard => "hard",
            WindowMode::Soft => "soft",
        }
    }
}

/// v4 `Scored`.
#[derive(Clone, Debug)]
pub struct Scored {
    pub strategy: RecallStrategy,
    pub ranked: Vec<RecallCandidate>,
    /// The recall context the ranking ran under (the soft window rides on it).
    pub context: Option<RecallContextInput>,
    /// The best raw score in the floored pool — R1's reference point.
    pub best_cosine: f64,
    /// Rows a hard window set aside (R6 may reserve head room for some).
    pub out_of_window: Vec<RankInput>,
    pub window_mode: WindowMode,
}

/// v4 `parseWindow` — both ends `Date.parse`-finite and `from <= to`.
fn parse_window(window: Option<&TimeWindow>) -> Option<(f64, f64)> {
    let w = window?;
    let from = date_ms(&w.from);
    let to = date_ms(&w.to);
    (from.is_finite() && to.is_finite() && from <= to).then_some((from, to))
}

/// A row's event time: `Date.parse(memory.occurredAt ?? memory.createdAt)`
/// (`??` — an absent or null `occurredAt` falls through; `""` does not).
fn event_time_ms(memory: &Value) -> f64 {
    let occurred = memory.get("occurredAt").filter(|v| !v.is_null());
    let iso = match occurred {
        Some(v) => v.as_str(),
        None => memory.get("createdAt").and_then(Value::as_str),
    };
    iso.map(date_ms).unwrap_or(f64::NAN)
}

/// The context with R1's gate switched off (v4 `{ ...context, tuning: {
/// ...recallTuningOf(context), boostGate: null } }`).
fn without_gate(context: &RecallContextInput) -> RecallContextInput {
    RecallContextInput {
        tuning: Some(ResolvedRecallTuning {
            boost_gate: None,
            ..context.tuning_of().clone()
        }),
        ..context.clone()
    }
}

/// v4 `score`.
pub fn score(
    hydrated: &[HydratedHit],
    gathered_strategy: RecallStrategy,
    gathered_min_score: f64,
    gathered_provider: Option<&str>,
    opts: &ScoreOptions,
    trace: &mut RecallTrace,
    label: Option<&str>,
) -> Scored {
    let floored: Vec<&HydratedHit> = hydrated
        .iter()
        .filter(|h| match gathered_strategy {
            RecallStrategy::Text => h.score > 0.0,
            RecallStrategy::Vector => h.score >= gathered_min_score,
        })
        .collect();
    let pool: Vec<RankInput> = floored
        .iter()
        .map(|h| RankInput {
            memory: h.memory.clone(),
            score: h.score,
            raw_score: h.raw_score,
            used_embedding: h.used_embedding,
            salience: salience_of(&h.memory, opts.now_ms),
        })
        .collect();
    let best_cosine = pool
        .iter()
        .fold(0.0_f64, |best, c| crate::jsnum::math_max(best, c.raw_score));

    // R1's constants are on the neural cosine scale: off for TF-IDF vectors
    // (when the context brings no tuning of its own) and for text scores.
    let mut context = opts.recall_context.clone();
    if let Some(ctx) = &context {
        if gathered_strategy == RecallStrategy::Text
            || (ctx.tuning.is_none() && is_lexical_embedding_provider(gathered_provider))
        {
            context = Some(without_gate(ctx));
        }
    }

    let mut candidates: Vec<RankInput> = pool.clone();
    let mut out_of_window: Vec<RankInput> = Vec::new();
    let mut window_mode = WindowMode::None;
    let mut rank_context = context.clone();
    if let Some((from, to)) = parse_window(opts.window.as_ref()) {
        let in_window = |c: &RankInput| {
            let t = event_time_ms(&c.memory);
            t.is_finite() && t >= from && t <= to
        };
        let hits: Vec<RankInput> = pool.iter().filter(|c| in_window(c)).cloned().collect();
        match &context {
            None => {
                candidates = hits;
                window_mode = WindowMode::Filter;
            }
            Some(_) if hits.len() >= opts.limit => {
                if gathered_strategy == RecallStrategy::Vector {
                    out_of_window = pool.iter().filter(|c| !in_window(c)).cloned().collect();
                }
                candidates = hits;
                window_mode = WindowMode::Hard;
            }
            Some(ctx) => {
                rank_context = Some(RecallContextInput {
                    occurred_within: opts.window.clone(),
                    ..ctx.clone()
                });
                window_mode = WindowMode::Soft;
            }
        }
    }

    let ranked = match &rank_context {
        Some(ctx) => rank_for_recall(&candidates, ctx, best_cosine, &[]),
        None => rank_by_blend(&candidates),
    };
    let mut detail = Map::new();
    detail.insert(
        "floor".into(),
        match gathered_strategy {
            RecallStrategy::Text => Value::from(">0"),
            RecallStrategy::Vector => num(gathered_min_score),
        },
    );
    detail.insert(
        "floored".into(),
        num((hydrated.len() - floored.len()) as f64),
    );
    detail.insert(
        "bestCosine".into(),
        num(crate::jsnum::math_round(best_cosine * 1000.0) / 1000.0),
    );
    detail.insert("window".into(), Value::from(window_mode.as_str()));
    detail.insert(
        "excluded".into(),
        num(candidates.len() as f64 - ranked.len() as f64),
    );
    detail.insert("ranked".into(), num(ranked.len() as f64));
    trace_stage(trace, RecallStage::Score, detail, label);
    Scored {
        strategy: gathered_strategy,
        ranked,
        context: rank_context,
        best_cosine,
        out_of_window,
        window_mode,
    }
}

/// [`score`] over a [`Gathered`] pool.
pub fn score_gathered(
    hydrated: &[HydratedHit],
    gathered: &Gathered,
    opts: &ScoreOptions,
    trace: &mut RecallTrace,
    label: Option<&str>,
) -> Scored {
    score(
        hydrated,
        gathered.strategy,
        gathered.min_score,
        gathered.provider.as_deref(),
        opts,
        trace,
        label,
    )
}
