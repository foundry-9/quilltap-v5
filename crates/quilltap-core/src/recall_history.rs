//! Port of lib/memory/recall-history.ts — the Commonplace Book recall
//! anti-repetition ring buffer.
//!
//! Persisted per-chat in `chats.commonplaceRecallHistory` as a JSON object
//! `{ turns: string[][] }` — one inner array of whispered memory IDs per recent
//! turn, most recent last, capped to [`RECALL_HISTORY_TURNS`]. (Wrapped in an
//! object rather than a bare array so it stays a plain JSON record column, not an
//! auto-detected SQLite "array column".)
//!
//! This is the producer side of the "recently whispered" set that
//! `recall_tags::recently_whispered_multiplier` consumes: the recall path unions
//! these IDs and applies a bounded penalty so the same memory doesn't get
//! whispered turn after turn and read as a stuck record.
//!
//! Pure + I/O-free. The TS parameter is typed `unknown` (an arbitrary, possibly
//! malformed JSON column), so the port takes `&serde_json::Value` and reproduces
//! the exact coercion: only a plain object with a `turns` array qualifies; each
//! turn must itself be an array; within a turn only non-empty strings survive.
//! Every malformed-input branch is preserved verbatim — that coercion is the
//! whole reason this module exists.

use std::collections::HashSet;

use serde_json::Value;

/// How many recent whisper-turns to remember for anti-repetition.
pub const RECALL_HISTORY_TURNS: usize = 3;

/// Persisted shape of the recall-history column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallHistory {
    /// One inner array of whispered memory IDs per recent turn, most recent last.
    pub turns: Vec<Vec<String>>,
    /// Retro mini-recap signatures (episodic overhaul). Round 2 (P4.d13) only
    /// PRESERVES an existing list through [`append_recall_turn`] exactly as
    /// v4's `appendRecallTurn` does — the spam-guard machinery that writes and
    /// consumes them (`appendRetroSignature` / `RETRO_SIGNATURE_TURNS`) is the
    /// round-3 port.
    pub retro_signatures: Vec<String>,
}

impl RecallHistory {
    /// The persisted JSON shape (v4 `RecallHistory`): always `{ "turns": … }`,
    /// with `retroSignatures` present only when non-empty — key order matches
    /// v4's object literals.
    pub fn to_value(&self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "turns".to_string(),
            Value::Array(
                self.turns
                    .iter()
                    .map(|t| Value::Array(t.iter().cloned().map(Value::String).collect()))
                    .collect(),
            ),
        );
        if !self.retro_signatures.is_empty() {
            obj.insert(
                "retroSignatures".to_string(),
                Value::Array(
                    self.retro_signatures
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        Value::Object(obj)
    }
}

/// Coerce the persisted retro-signature list, dropping anything malformed (v4
/// `parseRetroSignatures`: only a plain object carrying `retroSignatures`
/// qualifies; only non-empty strings survive).
pub fn parse_retro_signatures(raw: &Value) -> Vec<String> {
    let sigs = match raw.as_object().and_then(|o| o.get("retroSignatures")) {
        Some(s) => s,
        None => return Vec::new(),
    };
    let sigs = match sigs.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    sigs.iter()
        .filter_map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Coerce the raw JSON column into a clean `Vec<Vec<String>>`, dropping anything
/// malformed.
///
/// Mirrors the TS `raw && typeof raw === 'object' && 'turns' in raw` guard: only
/// a plain JSON object carrying a `turns` key qualifies. A bare array does NOT
/// (`'turns' in [...]` is false in JS, and a `Value::Array` is not a
/// `Value::Object` here), null/number/string fall through, and a missing or
/// non-array `turns` yields an empty buffer. Within each turn, only strings of
/// length > 0 survive; a non-array turn element is dropped ENTIRELY (it does not
/// degrade to an empty turn), while an array turn that filters down to empty is
/// PRESERVED as `[]`.
pub fn parse_recall_history(raw: &Value) -> Vec<Vec<String>> {
    let turns = match raw.as_object().and_then(|o| o.get("turns")) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let turns = match turns.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    turns
        .iter()
        .filter_map(|turn| turn.as_array()) // non-array turns dropped entirely
        .map(|turn| {
            turn.iter()
                .filter_map(|id| id.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
        })
        .collect()
}

/// Union of all memory IDs across the retained recent turns.
///
/// The TS returns a `ReadonlySet<string>` (insertion-ordered, but consumers only
/// ever `.has()` it), so the natural Rust shape is a `HashSet<String>` — exactly
/// what `recall_tags::recently_whispered_multiplier` takes.
pub fn recently_whispered_id_set(raw: &Value) -> HashSet<String> {
    let mut set = HashSet::new();
    for turn in parse_recall_history(raw) {
        for id in turn {
            set.insert(id);
        }
    }
    set
}

/// Append this turn's whispered IDs and trim to the last [`RECALL_HISTORY_TURNS`].
///
/// Subtleties preserved from v4:
///   * Empty turns are not recorded — the buffer tracks recent *whispers*, not
///     silence, so a gap turn doesn't prematurely age a still-recent memory out
///     of the penalty. With empty `new_ids` the parsed buffer is returned
///     UNCHANGED — notably NOT trimmed, so an over-cap column survives a silent
///     turn intact (mirrors TS `return { turns }`).
///   * `new_ids` is only de-duplicated (first-occurrence order), NOT
///     empty-string-filtered — the `[...new Set(newIds)]` step trusts its caller
///     to pass real IDs, so an empty string in `new_ids` is pushed as-is.
pub fn append_recall_turn(raw: &Value, new_ids: &[String]) -> RecallHistory {
    let mut turns = parse_recall_history(raw);
    // v4 (8bf3cb5f): an existing retroSignatures list rides through the append
    // untouched (the write must not destroy a v4-written spam-guard list).
    let retro_signatures = parse_retro_signatures(raw);
    if new_ids.is_empty() {
        return RecallHistory {
            turns,
            retro_signatures,
        };
    }
    // Dedup new_ids preserving first-occurrence order (mirrors `new Set(newIds)`);
    // deliberately no empty-string filter here.
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for id in new_ids {
        if seen.insert(id.as_str()) {
            deduped.push(id.clone());
        }
    }
    turns.push(deduped);
    // slice(-RECALL_HISTORY_TURNS): keep the last N turns.
    if turns.len() > RECALL_HISTORY_TURNS {
        turns.drain(0..turns.len() - RECALL_HISTORY_TURNS);
    }
    RecallHistory {
        turns,
        retro_signatures,
    }
}
