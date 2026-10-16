//! The extraction parsers — port of v4 `lib/memory/extraction/parse.ts`
//! (`ac6348c5f`, memory refactor Phase D; pin `83334b9a5`) plus the draft
//! type it yields (v4 `MemoryDraft`, `lib/memory/cheap-llm-tasks/types.ts:16`
//! — the pre-refactor `MemoryCandidate` renamed by Phase A).
//!
//! v4's why, carried: one family, every pass. Each parser turns a cheap
//! model's raw answer into [`MemoryDraft`]s — the model's half of a
//! [`crate::memory_candidate::MemoryCandidate`], which the pass runner
//! completes with subject and provenance. A malformed answer yields nothing;
//! a malformed item is skipped alone (pre-refactor, a `null` element in a
//! SELF array threw inside the whole-function `try` and the entire answer
//! became `[]`).
//!
//! Caps are one rule per pass ([`pass_cap`]): the pass's own base, clamped by
//! the call's output budget. The SELF and OTHER prompts promise one slot
//! beyond the cap for a dated or placed EVENT ([`EVENT_EXTRA_SLOT`]), and the
//! parsers honour it — at turn and fold grain alike.
//!
//! Importance and targeting go through the candidate's own normalisers
//! ([`coerce_importance`], [`normalise_targeting`]), so the parser holds no
//! default of its own and a model's `0` stays `0`.

use serde_json::{Map, Value};

use crate::jsnum::{math_max, math_min};
use crate::jsstr::{js_trim, utf16_truncate};
use crate::memory_candidate::{
    coerce_importance, normalise_targeting, MemoryDraftView, MemoryKind, PartialTargetingTags,
    TargetingTags,
};
use crate::memory_tasks::strip_code_fences;
use crate::recall_tags::{ContextTag, ScopeTag, TemporalTag};

/// Hard ceiling on candidates from one per-turn call (per subject, for
/// OTHER), whatever the output budget. The base of the turn passes'
/// [`pass_cap`].
pub const HARD_CANDIDATE_CAP: usize = 2;

/// Extra candidate slot granted only when a dated/placed EVENT is present.
pub const EVENT_EXTRA_SLOT: usize = 1;

/// Output tokens one candidate slot is budgeted.
const TOKENS_PER_CANDIDATE_SLOT: f64 = 8000.0;

/// Most entities a parsed draft keeps.
const DRAFT_ENTITY_CAP: usize = 8;

/// A memory as an extractor's parser yields it (v4 `MemoryDraft`): the
/// model's part of a candidate, before the producer adds who holds it, whom
/// it is about and where it came from (`candidate_from_draft`). `None` is
/// v4's `undefined`.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryDraft {
    pub content: Option<String>,
    pub summary: Option<String>,
    /// Free keywords for text-based search (targeting tags are not here).
    pub keywords: Vec<String>,
    /// Clamped by the parser; 0.5 when absent (the only default).
    pub importance: f64,
    /// The targeting axes the model emitted, normalised against the closed
    /// vocabularies. The row stores them in `keywords`; the draft and the
    /// candidate carry them apart.
    pub targeting: TargetingTags,
    pub kind: MemoryKind,
    /// When the event happened, as the model phrased it.
    pub when: Option<String>,
    /// Proper nouns of the episode. `None` is v4's `undefined` (the SELF /
    /// OTHER item carried no array); the episode parser always sets it.
    pub entities: Option<Vec<String>>,
    /// An explicit in-story time stamp (only the episode prompt asks for
    /// one).
    pub narrative_time: Option<String>,
}

impl MemoryDraft {
    /// The structural view [`crate::memory_candidate::candidate_from_draft`]
    /// takes (v4 passes the draft itself as a `MemoryDraftLike`).
    pub fn to_view(&self) -> MemoryDraftView {
        MemoryDraftView {
            content: self.content.clone(),
            summary: self.summary.clone(),
            keywords: Some(self.keywords.iter().cloned().map(Value::String).collect()),
            importance: Some(crate::db::js_number_to_json(self.importance)),
            targeting: Some(serde_json::json!({
                "temporal": self.targeting.temporal.as_str(),
                "scope": self.targeting.scope.as_str(),
                "context": self.targeting.context.as_str(),
            })),
            kind: Some(self.kind.as_str().to_string()),
            when: self.when.clone(),
            entities: self
                .entities
                .as_ref()
                .map(|e| e.iter().cloned().map(Value::String).collect()),
        }
    }
}

/// The cap of one call (v4 `passCap`): `min(base, ceil(maxTokens / 8000))`,
/// never below 1. The turn passes' base is [`HARD_CANDIDATE_CAP`]; the fold
/// OTHER pass's is `foldCandidatesPerSubject`. (The episode pass does NOT
/// call it — its cap is `FOLD_EPISODE_CAP`, raw.)
pub fn pass_cap(base: f64, max_tokens: Option<f64>) -> usize {
    let budget =
        (max_tokens.unwrap_or(TOKENS_PER_CANDIDATE_SLOT) / TOKENS_PER_CANDIDATE_SLOT).ceil();
    let cap = math_max(1.0, math_min(math_max(1.0, base.floor()), budget));
    // A NaN cap (a NaN `base` or budget — unreachable from v4's validated
    // settings) is `drafts.slice(0, NaN)` in v4: ZERO drafts, not every draft.
    if cap.is_nan() {
        0
    } else if cap.is_finite() {
        cap as usize
    } else {
        usize::MAX
    }
}

/// True when a draft is a dated/placed EVENT that earns the extra slot (v4
/// `isAnchoredEvent`).
fn is_anchored_event(d: &MemoryDraft) -> bool {
    d.kind == MemoryKind::Episodic
        && (d.when.as_deref().is_some_and(|w| !w.is_empty())
            || d.entities.as_ref().is_some_and(|e| !e.is_empty()))
}

/// The cap with the episodic soft-raise (v4 `capWithEventSlot`): `cap`
/// drafts always, one more only when a dated/placed EVENT survives inside the
/// raised window.
pub fn cap_with_event_slot(mut drafts: Vec<MemoryDraft>, cap: usize) -> Vec<MemoryDraft> {
    let raised_len = cap.saturating_add(EVENT_EXTRA_SLOT).min(drafts.len());
    if raised_len > cap && drafts[..raised_len].iter().any(is_anchored_event) {
        drafts.truncate(raised_len);
    } else {
        drafts.truncate(cap);
    }
    drafts
}

/// v4 `parseJsonItems`: `JSON.parse(stripCodeFences(raw))`, an array or the
/// one value wrapped; a parse error is `None`.
fn parse_json_items(raw: &str) -> Option<Vec<Value>> {
    let parsed: Value = serde_json::from_str(&strip_code_fences(raw)).ok()?;
    Some(match parsed {
        Value::Array(items) => items,
        other => vec![other],
    })
}

/// v4 `strings(value, cap)`: the non-blank string elements, trimmed, capped;
/// a non-array is empty.
fn strings(value: Option<&Value>, cap: usize) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .filter(|e| !js_trim(e).is_empty())
            .map(|e| js_trim(e).to_string())
            .take(cap)
            .collect(),
        _ => Vec::new(),
    }
}

/// v4 `optionalString`: a non-blank string, trimmed; else `undefined`.
fn optional_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) if !js_trim(s).is_empty() => Some(js_trim(s).to_string()),
        _ => None,
    }
}

/// JS truthiness of a parsed JSON value.
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// `typeof x === 'string' ? x : (x ? JSON.stringify(x) : undefined)`.
fn coerce_text(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) if is_truthy(other) => Some(crate::pascal::js_value::json_stringify(other)),
        _ => None,
    }
}

/// One raw SELF/OTHER item as a draft, or `None` when it has neither content
/// nor summary (v4 `coerceMemoryDraft`). Truthiness, not blankness: a
/// whitespace-only content keeps the draft alive (the candidate then falls
/// back to the other field, or skips it with the "no usable text" line).
pub fn coerce_memory_draft(item: &Map<String, Value>) -> Option<MemoryDraft> {
    let raw_kind = match item.get("kind") {
        Some(Value::String(s)) => js_trim(s).to_lowercase(),
        _ => String::new(),
    };
    let entities = match item.get("entities") {
        Some(v @ Value::Array(_)) => Some(strings(Some(v), DRAFT_ENTITY_CAP)),
        _ => None,
    };
    let item_value = Value::Object(item.clone());
    let draft = MemoryDraft {
        content: coerce_text(item.get("content")),
        summary: coerce_text(item.get("summary")),
        keywords: match item.get("keywords") {
            Some(Value::Array(ks)) => ks
                .iter()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        },
        importance: coerce_importance(item.get("importance")),
        targeting: normalise_targeting(Some(&item_value), &PartialTargetingTags::default()),
        kind: if raw_kind == "episodic" {
            MemoryKind::Episodic
        } else {
            MemoryKind::Semantic
        },
        when: optional_string(item.get("when")),
        entities,
        narrative_time: None,
    };
    let empty = |s: &Option<String>| s.as_deref().is_none_or(str::is_empty);
    if empty(&draft.content) && empty(&draft.summary) {
        return None;
    }
    Some(draft)
}

/// The SELF parser: a flat array, capped with the event slot (v4
/// `parseSelfDrafts`). A non-object element — `null` included — is skipped.
pub fn parse_self_drafts(raw: &str, cap: usize) -> Vec<MemoryDraft> {
    let Some(items) = parse_json_items(raw) else {
        return Vec::new();
    };
    let drafts = items
        .iter()
        .filter_map(|item| match item {
            Value::Object(o) => coerce_memory_draft(o),
            // JS `typeof [] === 'object'`: an array reaches the coercer and,
            // having no `content` / `summary` key, is dropped there.
            Value::Array(_) => coerce_memory_draft(&Map::new()),
            _ => None,
        })
        .collect();
    cap_with_event_slot(drafts, cap)
}

/// The multi-subject OTHER parser (v4 `parseOtherDraftsBySubject`): routes
/// each item to its subject by 1-based `subjectIndex` (`subjectIds[i - 1]`);
/// items with a missing or invalid index are dropped. Each subject's bucket
/// collects up to `cap + 1` and is then capped with the event slot. Every
/// subject appears in the result (subject order — v4's `Map` insertion order),
/// empty when it yielded nothing.
pub fn parse_other_drafts_by_subject(
    raw: &str,
    subject_ids: &[String],
    cap: usize,
) -> Vec<(String, Vec<MemoryDraft>)> {
    // v4's `Map`: a repeated subject id keeps ONE entry (first position).
    let mut result: Vec<(String, Vec<MemoryDraft>)> = Vec::new();
    for id in subject_ids {
        if !result.iter().any(|(k, _)| k == id) {
            result.push((id.clone(), Vec::new()));
        }
    }
    if subject_ids.is_empty() {
        return result;
    }
    let Some(items) = parse_json_items(raw) else {
        return result;
    };
    let collect_cap = cap.saturating_add(EVENT_EXTRA_SLOT);
    for raw_item in &items {
        let empty = Map::new();
        let item = match raw_item {
            Value::Object(o) => o,
            // An array is `typeof 'object'`; it carries no `subjectIndex`.
            Value::Array(_) => &empty,
            _ => continue,
        };
        // `typeof item.subjectIndex === 'number' ? … : NaN`, then
        // `Number.isInteger(idx)` — a JSON `2.0` IS an integer to JS.
        let idx = match item.get("subjectIndex") {
            Some(Value::Number(n)) => match n.as_f64() {
                Some(f) if f.is_finite() && f.fract() == 0.0 => f,
                _ => continue,
            },
            _ => continue,
        };
        if idx < 1.0 || idx > subject_ids.len() as f64 {
            continue;
        }
        let subject_id = &subject_ids[idx as usize - 1];
        let Some((_, bucket)) = result.iter_mut().find(|(k, _)| k == subject_id) else {
            continue;
        };
        if bucket.len() >= collect_cap {
            continue;
        }
        if let Some(draft) = coerce_memory_draft(item) {
            bucket.push(draft);
        }
    }
    for (_, bucket) in result.iter_mut() {
        *bucket = cap_with_event_slot(std::mem::take(bucket), cap);
    }
    result
}

/// The targeting every episode carries: a past, narrow piece of history (v4
/// `EPISODE_TARGETING`).
pub const EPISODE_TARGETING: TargetingTags = TargetingTags {
    temporal: TemporalTag::Past,
    scope: ScopeTag::Narrow,
    context: ContextTag::History,
};

/// The episode parser (v4 `parseEpisodeDrafts`): `{ narrative, summary,
/// when, narrativeTime, entities, importance }` items as episodic drafts.
/// The narrative is the content; the summary defaults to its first 60
/// code units; importance is clamped to [0.2, 1] (0.6 when absent or not a
/// finite number); the entities double as the free keywords (lower-cased).
/// No event slot — the episode prompt promises none.
pub fn parse_episode_drafts(raw: &str, cap: usize) -> Vec<MemoryDraft> {
    let Some(items) = parse_json_items(raw) else {
        return Vec::new();
    };
    let mut drafts: Vec<MemoryDraft> = Vec::new();
    for raw_item in &items {
        if drafts.len() >= cap {
            break;
        }
        let empty = Map::new();
        let item = match raw_item {
            Value::Object(o) => o,
            Value::Array(_) => &empty,
            _ => continue,
        };
        let narrative = match item.get("narrative") {
            Some(Value::String(s)) => js_trim(s).to_string(),
            _ => String::new(),
        };
        if narrative.is_empty() {
            continue;
        }
        let entities = strings(item.get("entities"), DRAFT_ENTITY_CAP);
        let importance = match item
            .get("importance")
            .and_then(|v| v.as_f64().filter(|_| v.is_number()))
        {
            Some(n) if n.is_finite() => math_min(1.0, math_max(0.2, n)),
            _ => 0.6,
        };
        drafts.push(MemoryDraft {
            summary: Some(
                optional_string(item.get("summary"))
                    .unwrap_or_else(|| utf16_truncate(&narrative, 60)),
            ),
            content: Some(narrative),
            keywords: entities.iter().map(|e| e.to_lowercase()).collect(),
            importance,
            targeting: EPISODE_TARGETING,
            kind: MemoryKind::Episodic,
            when: optional_string(item.get("when")),
            narrative_time: optional_string(item.get("narrativeTime")),
            entities: Some(entities),
        });
    }
    drafts
}
