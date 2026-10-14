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
/// `effectiveWeight` / `rawWeight` are gone). The row stays the net-JSON
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
