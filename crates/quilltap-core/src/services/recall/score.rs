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
//! filter, and a tool window at most [`TOOL_WINDOW_FLOOR_MAX_DAYS`] wide is
//! its own relevance filter: an in-window row is eligible however loosely it
//! matches (the caller named the period). The prompt paths keep the floor
//! (R5, bug 202). (v4 4.12.1 `6121f59b9`, P4.D375.)

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

// === P4.D375 ===
/// An explicit tool window at most this wide is its own relevance filter (v4
/// `TOOL_WINDOW_FLOOR_MAX_DAYS`).
pub const TOOL_WINDOW_FLOOR_MAX_DAYS: f64 = 14.0;

const DAY_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

/// v4 `isNarrowToolWindow` — true when an explicit tool window (ISO bounds) is
/// narrow enough to stand in for the relevance floor. The conversation search
/// reads it too, so the two sources agree on what "narrow" means.
pub fn is_narrow_tool_window(window: Option<&TimeWindow>) -> bool {
    parse_window(window).is_some_and(|(from, to)| to - from <= TOOL_WINDOW_FLOOR_MAX_DAYS * DAY_MS)
}
// === end P4.D375 ===

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
    // P4.D375: the window parsed once; `in_window_at` serves the floor's relaxed
    // arm and the window staging alike.
    let window = parse_window(opts.window.as_ref());
    let in_window_at = |memory: &Value| match window {
        Some((from, to)) => {
            let t = event_time_ms(memory);
            t.is_finite() && t >= from && t <= to
        }
        None => false,
    };
    let relax_floor = opts.recall_context.is_none()
        && window.is_some_and(|(from, to)| to - from <= TOOL_WINDOW_FLOOR_MAX_DAYS * DAY_MS);
    let floored: Vec<&HydratedHit> = hydrated
        .iter()
        .filter(|h| {
            if relax_floor && in_window_at(&h.memory) {
                return true;
            }
            match gathered_strategy {
                RecallStrategy::Text => h.score > 0.0,
                RecallStrategy::Vector => h.score >= gathered_min_score,
            }
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
    if window.is_some() {
        let in_window = |c: &RankInput| in_window_at(&c.memory);
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
    // P4.D375: only when relaxed.
    if relax_floor {
        detail.insert("floorRelaxed".into(), Value::Bool(true));
    }
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

#[cfg(test)]
mod narrow_window_tests {
    //! P4.D375 item 7 (v4 `window-first-recall.test.ts`'s `isNarrowToolWindow`
    //! row, plus the edges): the differential is
    //! `window_first_recall_equivalence`.
    use super::*;

    fn w(from: &str, to: &str) -> TimeWindow {
        TimeWindow {
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn fourteen_days_exactly_is_narrow_and_one_ms_more_is_not() {
        assert!(is_narrow_tool_window(Some(&w(
            "2026-11-01T00:00:00.000Z",
            "2026-11-15T00:00:00.000Z"
        ))));
        assert!(!is_narrow_tool_window(Some(&w(
            "2026-11-01T00:00:00.000Z",
            "2026-11-15T00:00:00.001Z"
        ))));
    }

    #[test]
    fn absent_unparseable_or_inverted_is_not_narrow() {
        assert!(!is_narrow_tool_window(None));
        assert!(!is_narrow_tool_window(Some(&w(
            "soon",
            "2026-11-15T00:00:00.000Z"
        ))));
        assert!(!is_narrow_tool_window(Some(&w(
            "2026-11-15T00:00:00.000Z",
            "2026-11-01T00:00:00.000Z"
        ))));
    }

    #[test]
    fn the_floor_relaxes_on_the_tool_path_only() {
        let hit = |id: &str, at: &str| HydratedHit {
            memory: serde_json::json!({ "id": id, "occurredAt": at, "createdAt": at, "reinforcedImportance": 0.5 }),
            raw_score: 0.2,
            score: 0.2,
            used_embedding: true,
        };
        let hydrated = vec![
            hit("in", "2026-11-03T22:00:00.000Z"),
            hit("out", "2026-10-01T22:00:00.000Z"),
        ];
        let day = w("2026-11-03T06:00:00.000Z", "2026-11-04T05:59:59.999Z");
        let run = |ctx: Option<RecallContextInput>, window: TimeWindow| {
            let mut trace = Vec::new();
            let scored = score(
                &hydrated,
                RecallStrategy::Vector,
                0.3,
                Some("OPENAI"),
                &ScoreOptions {
                    limit: 10,
                    recall_context: ctx,
                    window: Some(window),
                    now_ms: 1_793_800_000_000.0,
                },
                &mut trace,
                None,
            );
            let relaxed = trace[0].detail.get("floorRelaxed").cloned();
            (scored.ranked.len(), relaxed)
        };
        assert_eq!(run(None, day.clone()), (1, Some(Value::Bool(true))));
        assert_eq!(
            run(
                None,
                w("2026-10-15T00:00:00.000Z", "2026-11-14T00:00:00.000Z")
            ),
            (0, None)
        );
        assert_eq!(run(Some(RecallContextInput::default()), day), (0, None));
    }
}
