//! The shapes that pass between the recall stages — v4
//! `lib/memory/recall/types.ts` (memory refactor §3.8, `584d28164`):
//!
//! ```text
//! query → gather → hydrate → score → select → assemble → render → deliver
//! ```
//!
//! Every stage after `gather` and `hydrate` is a pure function of the previous
//! stage's output, and each appends one entry to the [`RecallTrace`], which the
//! replay harness reports and `deliver` writes to the turn's debug log.

use serde::Serialize;
use serde_json::{Map, Value};

/// How a raw hit entered the pool (v4 `HitSource`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HitSource {
    Vector,
    Probe,
    /// P4.D375 (v4 4.12.1 `6121f59b9`): a row of the query's period, read by
    /// event time in SQL and scored by the same index restricted to them (or,
    /// on the text strategy, the period's newest rows at their text score).
    Window,
    Anchor,
    Literal,
    Text,
}

/// A candidate before hydration: an id and the score it entered with (v4
/// `RawHit`). For the vector strategy `score` is the cosine against the query
/// (the best across probes); for the text strategy it is the text-match score,
/// and the row is already in hand.
#[derive(Clone, Debug)]
pub struct RawHit {
    pub id: String,
    pub score: f64,
    pub via: HitSource,
    /// Present when the strategy already read the row (anchors, the text
    /// fallback).
    pub memory: Option<Value>,
}

/// A hit with its row, after the hydrate filters (v4 `HydratedHit`).
#[derive(Clone, Debug)]
pub struct HydratedHit {
    pub memory: Value,
    /// The cosine (or text score) as gathered — R1's gate reads this.
    pub raw_score: f64,
    /// `raw_score`, after the tool's literal-phrase boost when one applied.
    pub score: f64,
    pub used_embedding: bool,
}

/// The record of why a candidate ranked where it did (v4 `RecallAdjustment`).
#[derive(Clone, Debug, PartialEq)]
pub struct RecallAdjustment {
    /// Combined, clamped multiplier applied to the blended score.
    pub multiplier: f64,
    /// Short labels for the adjustments that fired (e.g. `narrow✓`, `past↓`).
    /// Debug only.
    pub fired: Vec<String>,
    /// The ranking blend (`0.75 × relevance + 0.25 × salience`) before the
    /// multiplier.
    pub blended_before: f64,
    /// The blend after the multiplier — what the head ranks on.
    pub blended_after: f64,
}

/// A ranked recall candidate: the row, its relevance (`score`), its salience
/// and, when a turn's recall context drove the ranking, the adjustment record
/// (v4 `RecallCandidate`, which replaced `SemanticSearchResult` —
/// the floored weight and `rawWeight` are gone). The row stays the net-JSON
/// `Value` the memory reads return (R-L13), so no caller re-models it.
#[derive(Clone, Debug)]
pub struct RecallCandidate {
    pub memory: Value,
    /// Relevance: the cosine (literal-boosted on the tool path) or the text
    /// score.
    pub score: f64,
    /// True when the score is a cosine; false on the text fallback.
    pub used_embedding: bool,
    /// `reinforcedImportance × decay` at the search clock.
    pub salience: f64,
    pub recall_adjustment: Option<RecallAdjustment>,
}

impl RecallCandidate {
    /// The row's `id` (`""` when absent).
    pub fn id(&self) -> &str {
        memory_id(&self.memory)
    }
}

/// A memory row's `id` (`""` when absent).
pub fn memory_id(memory: &Value) -> &str {
    memory.get("id").and_then(Value::as_str).unwrap_or("")
}

/// The stages, in order (v4 `RecallStage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RecallStage {
    Query,
    Gather,
    Hydrate,
    Score,
    Select,
    Assemble,
    Render,
    Deliver,
}

impl RecallStage {
    /// v4's stage string.
    pub fn as_str(self) -> &'static str {
        match self {
            RecallStage::Query => "query",
            RecallStage::Gather => "gather",
            RecallStage::Hydrate => "hydrate",
            RecallStage::Score => "score",
            RecallStage::Select => "select",
            RecallStage::Assemble => "assemble",
            RecallStage::Render => "render",
            RecallStage::Deliver => "deliver",
        }
    }
}

/// One stage's account of itself: counts and choices, never memory text (v4
/// `RecallTraceEntry`). Serialises `stage, label, detail` in that order, with
/// `label` ABSENT (never null) when none was given — v4's `traceStage` pushes
/// `label ? { stage, label, detail } : { stage, detail }`. The detail map is
/// insertion-ordered (`preserve_order`) and built in v4's key order.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RecallTraceEntry {
    pub stage: RecallStage,
    /// Which search or pool the entry belongs to, when more than one ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub detail: Map<String, Value>,
}

/// v4 `RecallTrace`.
pub type RecallTrace = Vec<RecallTraceEntry>;

/// Append a trace entry — the stages' one way of writing to the trace (v4
/// `traceStage`). A `None` or EMPTY label is left off (v4's `label ? … : …` —
/// `''` is falsy).
pub fn trace_stage(
    trace: &mut RecallTrace,
    stage: RecallStage,
    detail: Map<String, Value>,
    label: Option<&str>,
) {
    trace.push(RecallTraceEntry {
        stage,
        label: label.filter(|l| !l.is_empty()).map(str::to_string),
        detail,
    });
}

/// One line a renderer produced: which memory, and the text that carries it
/// (v4 `RenderedLine`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedLine {
    pub memory_id: String,
    /// The whole rendered line.
    pub line: String,
    /// The memory's own words on the line — what `deliver` looks for after
    /// compression.
    pub body: String,
}

/// One inner-circle row in a frozen archive: the memory and where it sits in
/// the circle (v4 `InnerCircleEntry`, `a5efdbb97` + `8cb584eff`).
#[derive(Clone, Debug, PartialEq)]
pub struct InnerCircleEntry {
    pub memory: Value,
    /// The circle member the memory is about.
    pub character_id: String,
    /// The member's tier (0 = the holder's tightest group; −1 = a present
    /// character's lifted keystone).
    pub tier: i64,
    /// The row's rank within its member's draw (0 = the member's first).
    pub rank: usize,
    /// The member's position in circle order (or the present character's, for
    /// a lifted keystone).
    pub order: usize,
    /// An operator-marked standing fact (inner-circle-recall.md Part C).
    pub keystone: bool,
}

/// Circle rows from most to least protected (v4 `innerCircleTrimOrder`):
/// keystones before digests, then the tighter tier, then a member's earlier
/// rows, then circle order. The archive's cap and the renderer's token budget
/// both cut from the END of this order, so the loosest tier's digests give up
/// lines first and keystones last. Answers the INDICES of `entries` in that
/// order — a stable sort, as V8's is.
pub fn inner_circle_trim_order(entries: &[InnerCircleEntry]) -> Vec<usize> {
    inner_circle_trim_order_by(entries, |e| e)
}

/// [`inner_circle_trim_order`] over rows that each carry an entry (the
/// renderer's built lines).
pub fn inner_circle_trim_order_by<T>(
    rows: &[T],
    entry: impl Fn(&T) -> &InnerCircleEntry,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|&i, &j| {
        let (a, b) = (entry(&rows[i]), entry(&rows[j]));
        b.keystone
            .cmp(&a.keystone)
            .then_with(|| a.tier.cmp(&b.tier))
            .then_with(|| a.rank.cmp(&b.rank))
            .then_with(|| a.order.cmp(&b.order))
    });
    order
}

/// Which section a delivered row was in (v4 `RecallPool`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum RecallPool {
    #[serde(rename = "archive")]
    Archive,
    #[serde(rename = "head")]
    Head,
    #[serde(rename = "interCharacter")]
    InterCharacter,
    #[serde(rename = "recap")]
    Recap,
    #[serde(rename = "relevant")]
    Relevant,
}

impl RecallPool {
    /// v4's pool string.
    pub fn as_str(self) -> &'static str {
        match self {
            RecallPool::Archive => "archive",
            RecallPool::Head => "head",
            RecallPool::InterCharacter => "interCharacter",
            RecallPool::Recap => "recap",
            RecallPool::Relevant => "relevant",
        }
    }
}

/// A trace detail number the way v4's `JSON.stringify` writes it: an integral
/// value as an integer (`3`, never `3.0`), NaN / ±∞ as `null`.
pub fn num(f: f64) -> Value {
    crate::db::js_number_to_json(f)
}

#[cfg(test)]
mod hit_source_tests {
    use super::HitSource;

    /// P4.D375: v4 `HitSource = 'vector' | 'probe' | 'window' | 'anchor' |
    /// 'literal' | 'text'` — the six serializations.
    #[test]
    fn six_serializations() {
        let all = [
            (HitSource::Vector, "\"vector\""),
            (HitSource::Probe, "\"probe\""),
            (HitSource::Window, "\"window\""),
            (HitSource::Anchor, "\"anchor\""),
            (HitSource::Literal, "\"literal\""),
            (HitSource::Text, "\"text\""),
        ];
        for (src, want) in all {
            assert_eq!(serde_json::to_string(&src).unwrap(), want);
        }
    }
}
