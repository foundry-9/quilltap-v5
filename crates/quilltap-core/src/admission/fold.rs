//! Port of v4 `lib/memory/admission/fold.ts` (`8af764514`, Phase C of the
//! memory system refactor; bugs 204 / 226 from `c2795733f`) — `fold_into`,
//! THE ONE COMBINE (memory refactor §3.6).
//!
//! v4's why, carried: wherever one memory takes in what others knew — a
//! candidate reinforcing a row at admission, near-duplicate rows folded into a
//! survivor — the plan is worked out here, the same way:
//!
//!  - novel details from each absorbed source append to the survivor as `[+]`
//!    footnotes, judged against the content as it grows, up to the cap;
//!  - `reinforcementCount` grows by the absorbed sources' counts, as a
//!    **delta** (`incrementReinforcement(by)`), never an absolute total (bugs
//!    182, 226);
//!  - entities union (case-insensitive, survivor's first) up to the cap;
//!  - links union, minus the survivor and every absorbed source — a fold only
//!    ever ADDS links (the pre `planMemoryMerge` rewrote the whole array);
//!  - the earliest `occurredAt` wins, a missing `narrativeTime` is filled;
//!  - `lastReinforcedAt` is the latest observation (the write never rewinds
//!    it);
//!  - the survivor is re-embedded whenever the text it is embedded as
//!    (`embeddingTextFor`: content and the anchor line) changed. The footnote
//!    cap stops the prose from growing; it never leaves a changed anchor line
//!    on an old vector (bug 204). `reembed = content changed || any patch
//!    key` — so it fires when `entities` GREW even past the anchor line's
//!    four-entity cap (O13: faithful to v4, pinned by a row; do not "fix").
//!
//! Pure: P4.D290's `apply_fold` writes the plan.
//!
//! JS fidelity: `earliest` is the first value with the smallest `Date.parse`
//! (strict `<`; [`crate::episodic::js_date_parse_ms`]); `latest` is
//! `filter(non-empty string).sort().pop()` — the UTF-16 code-unit
//! lexicographic max, NOT a date compare (`2026-1…` beats `2026-09…`, and an
//! astral character sorts below U+E000–U+FFFF where `str::cmp` would put it
//! above); the delta is `Σ Math.max(1, count ?? 1)` through the NaN-propagating
//! [`crate::jsnum::math_max`], so a fractional count adds its fraction and a
//! NaN count makes the whole delta NaN, as in v4.

use std::collections::HashSet;

use serde::Serialize;
use serde_json::Value;

use super::novel_details::extract_novel_details;
use super::thresholds::ADMISSION_THRESHOLDS;
use crate::jsnum::{math_max, number_from_str};
use crate::jsstr::js_trim;
use crate::memory_candidate::MEMORY_ENTITY_CAP;
use crate::reinforcement_footnotes::{append_capped_footnotes, count_reinforcement_footnotes};

/// Bounds a fold respects (v4 `FoldCaps`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoldCaps {
    pub footnote_cap: usize,
    pub entity_cap: usize,
}

/// The bounds every fold uses (v4 `FOLD_CAPS`): the admission footnote cap and
/// the candidate's entity bound.
pub const FOLD_CAPS: FoldCaps = FoldCaps {
    footnote_cap: ADMISSION_THRESHOLDS.footnote_cap,
    entity_cap: MEMORY_ENTITY_CAP,
};

/// What a fold takes from each absorbed source (v4 `FoldSource`). `None` is
/// v4's `undefined` / `null`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FoldSource {
    /// The absorbed row's id; absent for a candidate that never became a row.
    pub id: Option<String>,
    pub content: String,
    pub occurred_at: Option<String>,
    pub narrative_time: Option<String>,
    pub entities: Option<Vec<String>>,
    pub related_memory_ids: Option<Vec<String>>,
    /// Observations the source stands for (a row's count; a candidate is 1).
    /// A JS number: fractional and NaN reach the delta as v4 sums them.
    pub reinforcement_count: Option<f64>,
    /// When it was last observed; a row never reinforced was observed at
    /// creation (`last_reinforced_at ?? created_at` — `??`, so an EMPTY
    /// `last_reinforced_at` does not fall back).
    pub last_reinforced_at: Option<String>,
    pub created_at: Option<String>,
}

impl FoldSource {
    /// A source read from a memory row's JSON (v4 passes whole `Memory` rows
    /// as sources from maintenance). Strings are taken as they stand (`null` /
    /// absent → `None`); `reinforcementCount` goes through JS `ToNumber` as
    /// `Math.max(1, x)` would apply it (a number as is, a string through
    /// `Number(s)`, a boolean as 1 / 0, an array / object as NaN). A
    /// non-string entity or link id is dropped: v4 skips a non-string entity
    /// inside the loop (after the cap test, so dropping it first changes
    /// nothing), and the `Memory` schema types links `string[]`.
    pub fn from_value(row: &Value) -> FoldSource {
        let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
        let strings = |key: &str| {
            row.get(key).and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
        };
        let reinforcement_count = match row.get("reinforcementCount") {
            None | Some(Value::Null) => None,
            Some(Value::Number(n)) => n.as_f64(),
            Some(Value::Bool(b)) => Some(if *b { 1.0 } else { 0.0 }),
            Some(Value::String(s)) => Some(number_from_str(s)),
            Some(Value::Array(_)) | Some(Value::Object(_)) => Some(f64::NAN),
        };
        FoldSource {
            id: text("id"),
            content: text("content").unwrap_or_default(),
            occurred_at: text("occurredAt"),
            narrative_time: text("narrativeTime"),
            entities: strings("entities"),
            related_memory_ids: strings("relatedMemoryIds"),
            reinforcement_count,
            last_reinforced_at: text("lastReinforcedAt"),
            created_at: text("createdAt"),
        }
    }
}

/// The survivor a fold reads (v4 `FoldSurvivor = Pick<Memory, 'id' |
/// 'content' | 'occurredAt' | 'narrativeTime' | 'entities' |
/// 'relatedMemoryIds' | 'lastReinforcedAt'>`): v4's `Memory` JSON row (the
/// [`crate::db::memories_read`] shape); only those seven keys are read.
pub type FoldSurvivor = Value;

/// Anchor fields that change (v4 `FoldPlan['patch']`). Never the count, its
/// importance or its time. Serialises in v4's insertion order — `occurredAt`,
/// `narrativeTime`, `entities` — omitting the unchanged keys.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrative_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entities: Option<Vec<String>>,
}

impl FoldPatch {
    /// v4 `Object.keys(patch)`, in insertion order.
    pub fn keys(&self) -> Vec<&'static str> {
        let mut keys = Vec::new();
        if self.occurred_at.is_some() {
            keys.push("occurredAt");
        }
        if self.narrative_time.is_some() {
            keys.push("narrativeTime");
        }
        if self.entities.is_some() {
            keys.push("entities");
        }
        keys
    }

    /// `Object.keys(patch).length === 0`.
    pub fn is_empty(&self) -> bool {
        self.occurred_at.is_none() && self.narrative_time.is_none() && self.entities.is_none()
    }
}

/// What folding changes (v4 `FoldPlan`).
#[derive(Clone, Debug, PartialEq)]
pub struct FoldPlan {
    pub survivor_id: String,
    /// Details to append as `[+]` footnotes (already capped, in order).
    pub footnotes: Vec<String>,
    /// The survivor's content with the footnotes appended.
    pub content: String,
    /// Links to add to the survivor.
    pub add_links: Vec<String>,
    /// Anchor fields that change.
    pub patch: FoldPatch,
    /// Observations to add to the survivor's count — v4's JS number sum
    /// (fractional and NaN reach it; P4.D290 maps it to the repository's
    /// positive-integer `by`).
    pub reinforcement_delta: f64,
    /// Latest observation time among survivor and sources.
    pub reinforced_at: Option<String>,
    /// The survivor already carried the footnote cap (no footnote can be
    /// added).
    pub at_cap: bool,
    /// The embedded text (content or anchor line) changed.
    pub reembed: bool,
}

/// v4 `earliest`: the first value with the smallest `Date.parse` (falsy and
/// unparseable values skipped; strict `<`, so the first wins ties).
fn earliest<'a>(values: impl IntoIterator<Item = Option<&'a str>>) -> Option<&'a str> {
    let mut best: Option<&str> = None;
    let mut best_ms = f64::INFINITY;
    for value in values {
        let Some(value) = value.filter(|s| !s.is_empty()) else {
            continue;
        };
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

/// v4 `latest`: `values.filter(non-empty string).sort().pop() ?? null` — the
/// UTF-16 code-unit lexicographic max.
fn latest<'a>(values: impl IntoIterator<Item = Option<&'a str>>) -> Option<&'a str> {
    values
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .max_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()))
}

fn string_array(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// JS truthiness of an optional JSON field.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// Work out what folding `absorbed` into `survivor` changes (v4 `foldInto`).
/// Pure — no writes.
///
/// The survivor's `entities` / `relatedMemoryIds` are read as string arrays
/// (the `Memory` schema's `string[]`; v4 would throw on a non-string survivor
/// entity's `toLowerCase`, so no defined behaviour exists to follow there).
pub fn fold_into(survivor: &FoldSurvivor, absorbed: &[FoldSource], caps: &FoldCaps) -> FoldPlan {
    let survivor_id = survivor
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let survivor_content = survivor
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let at_cap = count_reinforcement_footnotes(survivor_content) >= caps.footnote_cap;

    // Footnotes, judged against the content as it grows so two sources
    // carrying the same detail append it once.
    let mut content = survivor_content.to_string();
    let mut footnotes: Vec<String> = Vec::new();
    for source in absorbed {
        let novel = extract_novel_details(&source.content, &content);
        if novel.is_empty() {
            continue;
        }
        let (next, appended) = append_capped_footnotes(&content, &novel, caps.footnote_cap);
        content = next;
        footnotes.extend(appended);
    }

    // Links: the sources' links, minus the group and what the survivor has.
    let mut group: HashSet<&str> = HashSet::new();
    group.insert(survivor_id.as_str());
    for source in absorbed {
        if let Some(id) = source.id.as_deref().filter(|id| !id.is_empty()) {
            group.insert(id);
        }
    }
    let existing_links: HashSet<String> = string_array(survivor.get("relatedMemoryIds"))
        .into_iter()
        .collect();
    let mut add_links: Vec<String> = Vec::new();
    for id in absorbed
        .iter()
        .flat_map(|s| s.related_memory_ids.iter().flatten())
    {
        if group.contains(id.as_str()) || existing_links.contains(id) || add_links.contains(id) {
            continue;
        }
        add_links.push(id.clone());
    }

    let mut patch = FoldPatch::default();
    let survivor_occurred = survivor.get("occurredAt").and_then(Value::as_str);
    let occurred_at = earliest(
        std::iter::once(survivor_occurred).chain(absorbed.iter().map(|s| s.occurred_at.as_deref())),
    );
    if let Some(occurred) = occurred_at {
        if Some(occurred) != survivor_occurred {
            patch.occurred_at = Some(occurred.to_string());
        }
    }
    if !truthy(survivor.get("narrativeTime")) {
        let narrative = absorbed
            .iter()
            .filter_map(|s| s.narrative_time.as_deref())
            .find(|v| !v.is_empty() && !js_trim(v).is_empty());
        if let Some(narrative) = narrative {
            patch.narrative_time = Some(narrative.to_string());
        }
    }
    let survivor_entities = string_array(survivor.get("entities"));
    let mut entities = survivor_entities.clone();
    let mut seen: HashSet<String> = entities.iter().map(|e| e.to_lowercase()).collect();
    for entity in absorbed.iter().flat_map(|s| s.entities.iter().flatten()) {
        if entities.len() >= caps.entity_cap {
            break;
        }
        if js_trim(entity).is_empty() {
            continue;
        }
        let lower = entity.to_lowercase();
        if seen.contains(&lower) {
            continue;
        }
        seen.insert(lower);
        entities.push(entity.clone());
    }
    if entities.len() > survivor_entities.len() {
        patch.entities = Some(entities);
    }

    let reinforcement_delta = absorbed.iter().fold(0.0_f64, |sum, s| {
        sum + math_max(1.0, s.reinforcement_count.unwrap_or(1.0))
    });
    let reinforced_at = latest(
        std::iter::once(survivor.get("lastReinforcedAt").and_then(Value::as_str)).chain(
            absorbed
                .iter()
                .map(|s| s.last_reinforced_at.as_deref().or(s.created_at.as_deref())),
        ),
    )
    .map(str::to_string);

    let reembed = content != survivor_content || !patch.is_empty();
    FoldPlan {
        survivor_id,
        footnotes,
        content,
        add_links,
        patch,
        reinforcement_delta,
        reinforced_at,
        at_cap,
        reembed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identical_source_changes_only_the_count() {
        let survivor = json!({ "id": "keep", "content": "Same words.", "relatedMemoryIds": [], "entities": [] });
        let plan = fold_into(
            &survivor,
            &[FoldSource {
                content: "Same words.".into(),
                ..Default::default()
            }],
            &FOLD_CAPS,
        );
        assert!(plan.footnotes.is_empty());
        assert!(plan.patch.is_empty());
        assert_eq!(serde_json::to_string(&plan.patch).unwrap(), "{}");
        assert!(!plan.reembed);
        assert_eq!(plan.reinforcement_delta, 1.0);
    }

    #[test]
    fn patch_keys_follow_v4_order() {
        let p = FoldPatch {
            occurred_at: Some("a".into()),
            narrative_time: None,
            entities: Some(vec!["E".into()]),
        };
        assert_eq!(p.keys(), vec!["occurredAt", "entities"]);
        assert_eq!(
            serde_json::to_string(&p).unwrap(),
            r#"{"occurredAt":"a","entities":["E"]}"#
        );
    }
}

#[cfg(test)]
mod jest_twins {
    //! Case-for-case port of the `foldInto` half of v4
    //! `__tests__/unit/lib/memory/admission-fold.test.ts` (pin `83334b9a5`).
    //! The `applyFold` half (the repository writes) is P4.D290's. A source v4
    //! passes as a whole `Memory` row goes through [`FoldSource::from_value`];
    //! one it passes as a literal (`{ content, … }`) is built field by field.

    use super::*;
    use crate::reinforcement_footnotes::MAX_REINFORCEMENT_FOOTNOTES;
    use serde_json::json;

    const CAPS: FoldCaps = FoldCaps {
        footnote_cap: MAX_REINFORCEMENT_FOOTNOTES,
        entity_cap: 12,
    };

    /// v4's `footnotes(n)`.
    fn footnotes(n: usize) -> String {
        (0..n)
            .map(|i| format!("[+] Detail{i}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// v4's `makeMemory(overrides)`.
    fn make_memory(overrides: Value) -> Value {
        let mut m = json!({
            "id": "keep",
            "characterId": "char-1",
            "content": "Friday keeps the ledger.",
            "summary": "Friday keeps the ledger.",
            "keywords": [],
            "tags": [],
            "importance": 0.6,
            "source": "AUTO",
            "reinforcementCount": 1,
            "lastReinforcedAt": null,
            "relatedMemoryIds": [],
            "reinforcedImportance": 0.6,
            "entities": [],
            "occurredAt": null,
            "narrativeTime": null,
            "tier": "hot",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z",
        });
        if let (Value::Object(base), Value::Object(over)) = (&mut m, overrides) {
            base.extend(over);
        }
        m
    }

    /// A literal source `{ content }`.
    fn src(content: &str) -> FoldSource {
        FoldSource {
            content: content.into(),
            ..Default::default()
        }
    }

    fn strs(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    /// "folds details, counts, links and the earliest occurredAt into the survivor"
    #[test]
    fn folds_details_counts_links_and_the_earliest_occurred_at_into_the_survivor() {
        let survivor = make_memory(json!({
            "reinforcementCount": 2,
            "relatedMemoryIds": ["n1", "loser"],
            "occurredAt": "2026-03-10T00:00:00.000Z",
        }));
        let loser = make_memory(json!({
            "id": "loser",
            "content": "Friday keeps the ledger for Marguerite.",
            "reinforcementCount": 3,
            "relatedMemoryIds": ["keep", "n1", "n2"],
            "occurredAt": "2026-03-08T00:00:00.000Z",
        }));

        let plan = fold_into(&survivor, &[FoldSource::from_value(&loser)], &CAPS);

        assert_eq!(plan.footnotes, strs(&["Marguerite"]));
        assert_eq!(plan.content, "Friday keeps the ledger.\n[+] Marguerite");
        // Bug 226: counts ride as a delta, never an absolute total.
        assert_eq!(plan.reinforcement_delta, 3.0);
        let keys = plan.patch.keys();
        assert!(!keys.contains(&"reinforcementCount"));
        assert!(!keys.contains(&"reinforcedImportance"));
        assert!(!keys.contains(&"content"));
        // Union minus the group and what the survivor already names.
        assert_eq!(plan.add_links, strs(&["n2"]));
        assert_eq!(
            plan.patch.occurred_at.as_deref(),
            Some("2026-03-08T00:00:00.000Z")
        );
        assert!(plan.reembed);
    }

    /// "judges details against the growing content, so two sources add one footnote"
    #[test]
    fn judges_details_against_the_growing_content_so_two_sources_add_one_footnote() {
        let plan = fold_into(
            &make_memory(json!({})),
            &[
                src("Seen with Marguerite."),
                src("Again with Marguerite and Valparaiso."),
            ],
            &CAPS,
        );
        assert_eq!(plan.footnotes, strs(&["Marguerite", "Valparaiso"]));
        assert_eq!(plan.reinforcement_delta, 2.0);
    }

    /// "appends up to the footnote cap and no further"
    #[test]
    fn appends_up_to_the_footnote_cap_and_no_further() {
        let survivor = make_memory(json!({
            "content": format!("Body\n{}", footnotes(MAX_REINFORCEMENT_FOOTNOTES - 2)),
        }));
        let plan = fold_into(&survivor, &[src("Body with Alpha, Beta and Gamma.")], &CAPS);
        assert_eq!(plan.footnotes.len(), 2);
        assert_eq!(
            count_reinforcement_footnotes(&plan.content),
            MAX_REINFORCEMENT_FOOTNOTES
        );
        assert!(!plan.at_cap);
        assert!(plan.reembed);
    }

    /// "past the cap: counts and unions entities, appends nothing, and
    /// re-embeds the changed anchor line (review #5)"
    #[test]
    fn past_the_cap_counts_and_unions_entities_appends_nothing_and_re_embeds_the_changed_anchor_line_review_5(
    ) {
        let survivor = make_memory(json!({
            "content": format!("Friday keeps the ledger.\n{}", footnotes(MAX_REINFORCEMENT_FOOTNOTES)),
            "entities": ["Friday"],
        }));
        let plan = fold_into(
            &survivor,
            &[FoldSource {
                entities: Some(strs(&["Marguerite"])),
                ..src("Friday keeps the ledger with Marguerite in Valparaiso.")
            }],
            &CAPS,
        );
        assert!(plan.at_cap);
        assert_eq!(plan.footnotes, Vec::<String>::new());
        assert_eq!(plan.patch.entities, Some(strs(&["Friday", "Marguerite"])));
        assert_eq!(plan.reinforcement_delta, 1.0);
        // The entities are on the anchor line, so the stored vector must follow (bug 204).
        assert!(plan.reembed);
    }

    /// "past the cap with nothing new to the anchor line: no re-embed"
    #[test]
    fn past_the_cap_with_nothing_new_to_the_anchor_line_no_re_embed() {
        let survivor = make_memory(json!({
            "content": format!("Body\n{}", footnotes(MAX_REINFORCEMENT_FOOTNOTES)),
        }));
        let plan = fold_into(&survivor, &[src("Body with Marguerite.")], &CAPS);
        assert!(plan.at_cap);
        assert!(!plan.reembed);
    }

    /// "unions entities case-insensitively, survivor first, up to the cap"
    #[test]
    fn unions_entities_case_insensitively_survivor_first_up_to_the_cap() {
        let survivor = make_memory(json!({ "entities": ["Friday", "Lighthouse"] }));
        let mut entities = strs(&["friday"]);
        entities.extend((0..20).map(|i| format!("Place{i}")));
        let plan = fold_into(
            &survivor,
            &[FoldSource {
                entities: Some(entities),
                ..src("x")
            }],
            &CAPS,
        );
        let got = plan.patch.entities.expect("entities grew");
        assert_eq!(got[..3].to_vec(), strs(&["Friday", "Lighthouse", "Place0"]));
        assert_eq!(got.len(), 12);
    }

    /// "fills a missing narrativeTime and never overwrites one"
    #[test]
    fn fills_a_missing_narrative_time_and_never_overwrites_one() {
        let with_time = |t: &str| FoldSource {
            narrative_time: Some(t.into()),
            ..src("x")
        };
        assert_eq!(
            fold_into(
                &make_memory(json!({})),
                &[with_time("the third bell")],
                &CAPS
            )
            .patch
            .narrative_time
            .as_deref(),
            Some("the third bell")
        );
        let patch = fold_into(
            &make_memory(json!({ "narrativeTime": "dawn" })),
            &[with_time("dusk")],
            &CAPS,
        )
        .patch;
        assert!(!patch.keys().contains(&"narrativeTime"));
        assert_eq!(patch.narrative_time, None);
    }

    /// "takes the latest observation time and counts an unreinforced row from its creation"
    #[test]
    fn takes_the_latest_observation_time_and_counts_an_unreinforced_row_from_its_creation() {
        let survivor = make_memory(json!({ "lastReinforcedAt": "2026-02-01T00:00:00.000Z" }));
        let source = make_memory(json!({ "id": "l", "createdAt": "2026-05-01T00:00:00.000Z" }));
        let plan = fold_into(&survivor, &[FoldSource::from_value(&source)], &CAPS);
        assert_eq!(
            plan.reinforced_at.as_deref(),
            Some("2026-05-01T00:00:00.000Z")
        );
    }

    /// "changes nothing but the count for an identical source"
    #[test]
    fn changes_nothing_but_the_count_for_an_identical_source() {
        let plan = fold_into(
            &make_memory(json!({ "content": "Same words." })),
            &[src("Same words.")],
            &CAPS,
        );
        assert_eq!(plan.footnotes, Vec::<String>::new());
        assert!(plan.patch.is_empty());
        assert_eq!(serde_json::to_string(&plan.patch).unwrap(), "{}");
        assert_eq!(plan.add_links, Vec::<String>::new());
        assert!(!plan.reembed);
        assert_eq!(plan.reinforcement_delta, 1.0);
    }
}
