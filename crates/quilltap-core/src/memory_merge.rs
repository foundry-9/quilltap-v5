//! Port of the pure half of v4's `lib/memory/memory-merge.ts` (`3f7320138`
//! F9): [`plan_memory_merge`] works out what folding duplicate memories into a
//! survivor changes. The one place two (or more) memories become one — at the
//! pin (`01a83539d`) deduplication is its only caller (`d58548051` retired
//! housekeeping's similarity pass), so a merged-away row keeps what it knew
//! instead of simply vanishing:
//!
//! - novel details from each loser append to the survivor as `[+]` footnotes,
//!   up to [`MAX_REINFORCEMENT_FOOTNOTES`](crate::memory_gate::MAX_REINFORCEMENT_FOOTNOTES)
//! - `reinforcementCount` sums (each loser was itself an observation)
//! - `relatedMemoryIds` union, minus the survivor, the losers, and anything
//!   else being deleted in the same pass
//! - `occurredAt` keeps the earliest non-null value
//! - `reinforcedImportance` is recomputed from the survivor's base importance
//!
//! Order matters: callers apply the merge BEFORE deleting the losers, and
//! delete only the losers whose fold succeeded — a failed fold keeps its
//! losers rather than discarding what the survivor never absorbed. The
//! stateful half (`applyMemoryMerge`) lives beside the two helpers it calls in
//! [`crate::services::memory_gate`].
//!
//! The memories are v4's `Memory` JSON (the [`crate::db::memories_read`] row
//! shape); the plan's `patch` is a JSON object in v4's key order, converted for
//! the writer by [`MemoryMergePlan::to_mem_update`].

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::db::memories::MemUpdate;
use crate::memory_gate::{
    append_capped_footnotes, calculate_reinforced_importance, extract_novel_details,
};

/// v4 `MemoryMergePlan`.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryMergePlan {
    pub survivor_id: String,
    /// Patch to apply to the survivor, in v4's key order; empty when the merge
    /// changes nothing.
    pub patch: Map<String, Value>,
    /// Details actually appended as footnotes (after the cap).
    pub merged_details: Vec<String>,
    /// True when the survivor's content changed (so it must be re-embedded).
    pub content_changed: bool,
}

impl MemoryMergePlan {
    /// The patch as the repository's [`MemUpdate`] (only the five keys
    /// [`plan_memory_merge`] can write).
    pub fn to_mem_update(&self) -> MemUpdate {
        let mut update = MemUpdate::default();
        if let Some(content) = self.patch.get("content").and_then(Value::as_str) {
            update.content = Some(content.to_string());
        }
        if let Some(count) = self.patch.get("reinforcementCount").and_then(Value::as_f64) {
            update.reinforcement_count = Some(count);
        }
        if let Some(ri) = self.patch.get("reinforcedImportance") {
            // A NaN importance (an absent base) serializes as JSON null, as
            // `JSON.stringify` renders it; the column then takes NULL's REAL.
            update.reinforced_importance = Some(ri.as_f64().unwrap_or(f64::NAN));
        }
        if let Some(links) = self.patch.get("relatedMemoryIds") {
            update.related_memory_ids = Some(string_array(Some(links)));
        }
        if let Some(occurred) = self.patch.get("occurredAt").and_then(Value::as_str) {
            update.occurred_at = Some(Some(occurred.to_string()));
        }
        if let Some(at) = self.patch.get("lastReinforcedAt").and_then(Value::as_str) {
            update.last_reinforced_at = Some(Some(at.to_string()));
        }
        update
    }
}

/// JS-truthy string: present, a string, non-empty.
fn truthy_str(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn string_array(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// `memory.reinforcementCount ?? 1` — only an absent / `null` count defaults;
/// a `0` stays `0`.
fn count_or_one(memory: &Value) -> f64 {
    match memory.get("reinforcementCount") {
        None | Some(Value::Null) => 1.0,
        Some(v) => v.as_f64().unwrap_or(f64::NAN),
    }
}

/// v4 `earliestOccurredAt`: the first value with the smallest `Date.parse`
/// (falsy and unparseable values skipped; strict `<`, so the first wins ties).
fn earliest_occurred_at<'a>(values: impl IntoIterator<Item = Option<&'a str>>) -> Option<&'a str> {
    let mut best: Option<&str> = None;
    let mut best_ms = f64::INFINITY;
    for value in values {
        let Some(value) = value.filter(|s| !s.is_empty()) else {
            continue;
        };
        // `Date.parse` — the JS-faithful parser (never the strict ISO one).
        let Some(ms) = crate::episodic::js_date_parse_ms(value) else {
            continue;
        };
        let ms = ms as f64;
        if ms < best_ms {
            best_ms = ms;
            best = Some(value);
        }
    }
    best
}

/// v4 `sameIds`: length-sensitive, order-insensitive membership.
fn same_ids(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let set: HashSet<&str> = a.iter().map(String::as_str).collect();
    b.iter().all(|id| set.contains(id.as_str()))
}

/// Work out what folding `losers` into `survivor` changes (v4
/// `planMemoryMerge`). Pure — no writes.
///
/// `exclude_ids` are ids that must not appear in the survivor's links (the rest
/// of the batch being deleted alongside the losers).
pub fn plan_memory_merge(
    survivor: &Value,
    losers: &[Value],
    exclude_ids: &[String],
) -> MemoryMergePlan {
    let survivor_id = survivor
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut excluded: HashSet<String> = exclude_ids.iter().cloned().collect();
    excluded.insert(survivor_id.clone());
    for loser in losers {
        if let Some(id) = loser.get("id").and_then(Value::as_str) {
            excluded.insert(id.to_string());
        }
    }

    // Novel details, judged against the survivor's content as it grows so two
    // losers carrying the same detail append it once.
    let survivor_content = survivor
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut content = survivor_content.to_string();
    let mut merged_details: Vec<String> = Vec::new();
    for loser in losers {
        let loser_content = loser
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let novel = extract_novel_details(loser_content, &content);
        if novel.is_empty() {
            continue;
        }
        let (next, appended) = append_capped_footnotes(&content, &novel);
        content = next;
        merged_details.extend(appended);
    }
    let content_changed = content != survivor_content;

    let survivor_count = count_or_one(survivor);
    let reinforcement_count = losers
        .iter()
        .fold(survivor_count, |sum, loser| sum + count_or_one(loser));

    let existing_links = string_array(survivor.get("relatedMemoryIds"));
    let mut seen: HashSet<String> = HashSet::new();
    let mut related_memory_ids: Vec<String> = Vec::new();
    let loser_links = losers
        .iter()
        .flat_map(|l| string_array(l.get("relatedMemoryIds")));
    for id in existing_links.iter().cloned().chain(loser_links) {
        if !excluded.contains(&id) && seen.insert(id.clone()) {
            related_memory_ids.push(id);
        }
    }

    let occurred_at = earliest_occurred_at(
        std::iter::once(survivor.get("occurredAt").and_then(Value::as_str)).chain(
            losers
                .iter()
                .map(|l| l.get("occurredAt").and_then(Value::as_str)),
        ),
    );

    // `.filter(non-empty string).sort().pop()` — the UTF-16 code-unit
    // lexicographic max, NOT a date compare (`2026-1…` beats `2026-09…`, and
    // an astral character sorts below U+E000–U+FFFF where `str::cmp` — UTF-8 /
    // scalar order — would put it above).
    let last_reinforced_at = std::iter::once(survivor)
        .chain(losers.iter())
        .filter_map(|m| truthy_str(m.get("lastReinforcedAt")))
        .max_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));

    let mut patch = Map::new();
    if content_changed {
        patch.insert("content".into(), Value::String(content));
    }
    if reinforcement_count != survivor_count {
        let importance = survivor
            .get("importance")
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN);
        patch.insert("reinforcementCount".into(), json_num(reinforcement_count));
        patch.insert(
            "reinforcedImportance".into(),
            json_num(calculate_reinforced_importance(
                importance,
                reinforcement_count,
            )),
        );
    }
    if !same_ids(&related_memory_ids, &existing_links) {
        patch.insert(
            "relatedMemoryIds".into(),
            Value::Array(related_memory_ids.into_iter().map(Value::String).collect()),
        );
    }
    if let Some(occurred) = occurred_at {
        if survivor.get("occurredAt").and_then(Value::as_str) != Some(occurred) {
            patch.insert("occurredAt".into(), Value::String(occurred.to_string()));
        }
    }
    if let Some(at) = last_reinforced_at {
        if survivor.get("lastReinforcedAt").and_then(Value::as_str) != Some(at) {
            patch.insert("lastReinforcedAt".into(), Value::String(at.to_string()));
        }
    }

    MemoryMergePlan {
        survivor_id,
        patch,
        merged_details,
        content_changed,
    }
}

/// A JS number as JSON: integral values render without a fraction (`3`, not
/// `3.0`), and a non-finite one is `null` (`JSON.stringify`).
fn json_num(n: f64) -> Value {
    if !n.is_finite() {
        return Value::Null;
    }
    if n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
        return Value::from(n as i64);
    }
    Value::from(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sums_counts_and_unions_links_minus_the_group() {
        let survivor = json!({"id": "s", "content": "Mara sails.", "importance": 0.5,
            "reinforcementCount": 2, "relatedMemoryIds": ["x", "l1"]});
        let losers = vec![
            json!({"id": "l1", "content": "Mara sails to Lisbon.", "reinforcementCount": 3,
                "relatedMemoryIds": ["s", "y"]}),
            json!({"id": "l2", "content": "Mara sails."}),
        ];
        let plan = plan_memory_merge(&survivor, &losers, &[]);
        assert_eq!(plan.merged_details, vec!["Lisbon".to_string()]);
        assert!(plan.content_changed);
        assert_eq!(
            serde_json::to_string(&plan.patch).unwrap(),
            format!(
                "{{\"content\":\"Mara sails.\\n[+] Lisbon\",\"reinforcementCount\":6,\
                 \"reinforcedImportance\":{},\"relatedMemoryIds\":[\"x\",\"y\"]}}",
                serde_json::to_string(&json_num(calculate_reinforced_importance(0.5, 6.0)))
                    .unwrap()
            )
        );
    }

    #[test]
    fn last_reinforced_at_is_the_utf16_string_max() {
        let survivor = json!({"id": "s", "content": "", "lastReinforcedAt": "2026-09-01"});
        let loser = json!({"id": "l", "content": "", "lastReinforcedAt": "2026-1"});
        let plan = plan_memory_merge(&survivor, &[loser], &[]);
        assert_eq!(plan.patch.get("lastReinforcedAt"), Some(&json!("2026-1")));
        // An astral character (surrogate 0xD8xx) sorts BELOW U+FFFD in UTF-16.
        let survivor = json!({"id": "s", "content": "", "lastReinforcedAt": "\u{FFFD}"});
        let loser = json!({"id": "l", "content": "", "lastReinforcedAt": "\u{1F600}"});
        let plan = plan_memory_merge(&survivor, &[loser], &[]);
        assert_eq!(plan.patch.get("lastReinforcedAt"), None);
    }
}
