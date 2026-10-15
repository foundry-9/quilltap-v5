//! Stage 7 — render: the section text a model reads (v4
//! `lib/memory/recall/render.ts`, `584d28164`).
//!
//! v4's why, carried: "A model sees each memory's age, whose life it describes
//! (the subject prefix), its words, a digest's provenance count and its human
//! keywords — nothing else. The id tags, the importance / relevance / weight
//! numbers, the multiplier labels and the raw targeting tags (`scope: narrow`,
//! `past`, …) are debug data: they go to `debugMemories` and the replay trace,
//! never into the text (decision 5; invariant 3 of the refactor spec)."
//!
//! Every renderer returns the lines it wrote with the memory each carries, so
//! `deliver` can tell which rows actually reached the model.
//!
//! Token costs are v4's per-provider estimate (R-T): `estimateTokens(line +
//! '\n', provider)` — the provider's registry rate (GOOGLE 3.8, everyone else
//! 3.5), the default 3.5 when no provider is known. Every sort is V8's
//! (`js_array_sort_by`): the comparators are non-total once a salience is NaN.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::memory_scoring::js_sort::js_array_sort_by;
use crate::memory_scoring::{date_ms, truthy_date_ms};
use crate::memory_time::age_label::format_memory_age;
use crate::recall_tags::{CONTEXT_VALUES, SCOPE_VALUES, TEMPORAL_VALUES};
use crate::token_estimation::{estimate_tokens, DEFAULT_CHARS_PER_TOKEN};

use super::rank::salience_of;
use super::types::{memory_id, num, RecallCandidate, RenderedLine};

/// v4 `estimateTokens(text, provider)`'s rate: the provider's registry value
/// (the plugin manifests — GOOGLE 3.8), or the default 3.5 with no provider.
pub fn chars_per_token(provider: Option<&str>) -> f64 {
    match provider {
        Some(p) if !p.is_empty() => {
            crate::provider_manifest::Registry::built_in().chars_per_token(p)
        }
        _ => DEFAULT_CHARS_PER_TOKEN,
    }
}

/// Debug info for one rendered memory — never shown to a model (v4
/// `DebugMemoryInfo`, the `salience` shape).
#[derive(Clone, Debug, PartialEq)]
pub struct DebugMemoryInfo {
    pub memory_id: Option<String>,
    pub summary: Option<String>,
    pub importance: Option<f64>,
    pub score: f64,
    /// `reinforcedImportance × decay`, no floor — the worth term the blend uses.
    pub salience: f64,
    pub recall_multiplier: Option<f64>,
    pub recall_fired: Option<Vec<String>>,
    pub blended_before: Option<f64>,
    pub blended_after: Option<f64>,
}

impl DebugMemoryInfo {
    /// v4's object in its key order, `undefined` keys dropped.
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        if let Some(id) = &self.memory_id {
            m.insert("memoryId".into(), Value::String(id.clone()));
        }
        if let Some(s) = &self.summary {
            m.insert("summary".into(), Value::String(s.clone()));
        }
        if let Some(i) = self.importance {
            m.insert("importance".into(), num(i));
        }
        m.insert("score".into(), num(self.score));
        m.insert("salience".into(), num(self.salience));
        if let Some(x) = self.recall_multiplier {
            m.insert("recallMultiplier".into(), num(x));
        }
        if let Some(f) = &self.recall_fired {
            m.insert(
                "recallFired".into(),
                Value::Array(f.iter().cloned().map(Value::String).collect()),
            );
        }
        if let Some(x) = self.blended_before {
            m.insert("blendedBefore".into(), num(x));
        }
        if let Some(x) = self.blended_after {
            m.insert("blendedAfter".into(), num(x));
        }
        Value::Object(m)
    }
}

/// Debug info for one rendered inter-character memory (v4
/// `DebugInterCharacterMemoryInfo`).
#[derive(Clone, Debug, PartialEq)]
pub struct DebugInterCharacterMemoryInfo {
    pub memory_id: Option<String>,
    pub about_character_name: String,
    pub summary: Option<String>,
    pub importance: Option<f64>,
}

impl DebugInterCharacterMemoryInfo {
    /// v4's object in its key order, `undefined` keys dropped.
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        if let Some(id) = &self.memory_id {
            m.insert("memoryId".into(), Value::String(id.clone()));
        }
        m.insert(
            "aboutCharacterName".into(),
            Value::String(self.about_character_name.clone()),
        );
        if let Some(s) = &self.summary {
            m.insert("summary".into(), Value::String(s.clone()));
        }
        if let Some(i) = self.importance {
            m.insert("importance".into(), num(i));
        }
        Value::Object(m)
    }
}

/// A rendered section: its text, its tokens, and what it carries (v4
/// `RenderedSection<D>`).
#[derive(Clone, Debug, PartialEq)]
pub struct RenderedSection<D> {
    pub content: String,
    pub token_count: i64,
    pub memories_used: usize,
    pub lines: Vec<RenderedLine>,
    pub debug_memories: Vec<D>,
}

impl<D> RenderedSection<D> {
    /// v4 `emptySection()`.
    pub fn empty() -> Self {
        RenderedSection {
            content: String::new(),
            token_count: 0,
            memories_used: 0,
            lines: Vec::new(),
            debug_memories: Vec::new(),
        }
    }
}

/// Whose life a memory line describes, for the self-facing blocks (v4
/// `MemorySubjectContext`).
///
/// v4's why, carried: a character's store holds what they remember about
/// themselves and about everyone else side by side; `aboutCharacterId` alone
/// tells them apart. Every self-facing block arrives under a second-person
/// heading, so a line about someone else, printed bare, reads as
/// autobiography (bug 122).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemorySubjectContext {
    /// The character whose store this is. Their own memories go unprefixed.
    pub self_character_id: String,
    /// `aboutCharacterId` → display name. Ids absent from the map still get a
    /// prefix.
    pub character_names: HashMap<String, String>,
}

/// v4 `formatMemorySubjectPrefix` — `""` for the character's own and
/// untargeted memories, `About <Name>: ` otherwise, falling back to `About
/// another character: ` when the id resolves to no name.
pub fn format_memory_subject_prefix(
    about_character_id: Option<&str>,
    subject: &MemorySubjectContext,
) -> String {
    // `if (!aboutCharacterId)` — absent AND empty-string are both falsy.
    let about = match about_character_id {
        Some(id) if !id.is_empty() => id,
        _ => return String::new(),
    };
    if about == subject.self_character_id {
        return String::new();
    }
    // `characterNames.get(id)?.trim()` — a name that trims to empty is falsy.
    match subject
        .character_names
        .get(about)
        .map(|n| crate::jsstr::js_trim(n))
        .filter(|n| !n.is_empty())
    {
        Some(name) => format!("About {name}: "),
        None => "About another character: ".to_string(),
    }
}

/// v4 `formatDigestProvenance` — ` (from 14 notes)` for a consolidated digest.
pub fn format_digest_provenance(memory: &Value) -> String {
    if memory.get("source").and_then(Value::as_str) != Some("CONSOLIDATED") {
        return String::new();
    }
    let count = memory
        .get("consolidatedFrom")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    if count == 0 {
        return String::new();
    }
    format!(
        " (from {count} {})",
        if count == 1 { "note" } else { "notes" }
    )
}

/// v4 `isTargetingKeyword` — is this keyword one of the targeting tags
/// (`past`, `scope: narrow`, `history`, …)? A GENUINE keyword spelled like a
/// temporal or context value (`history`, `banter`, `present`, …) is dropped
/// too — ported as v4 has it (R-L7, a candidate v4 observation).
pub fn is_targeting_keyword(keyword: &str) -> bool {
    let kw = crate::jsstr::js_trim(keyword).to_lowercase();
    if let Some(rest) = kw.strip_prefix("scope:") {
        return SCOPE_VALUES.contains(&crate::jsstr::js_trim(rest));
    }
    TEMPORAL_VALUES.contains(&kw.as_str()) || CONTEXT_VALUES.contains(&kw.as_str())
}

/// v4 `humanKeywords` — a row's keywords as a reader would want them: trimmed,
/// non-empty, the targeting tags removed (a non-string entry reads `''` and is
/// dropped).
pub fn human_keywords(keywords: Option<&Value>) -> Vec<String> {
    keywords
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|k| k.as_str().map(crate::jsstr::js_trim).unwrap_or(""))
                .filter(|k| !k.is_empty() && !is_targeting_keyword(k))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// v4 `formatKeywordsTag` — ` _(keywords: a, b)_`, or `""` when none survive.
pub fn format_keywords_tag(keywords: Option<&Value>) -> String {
    let words = human_keywords(keywords);
    if words.is_empty() {
        String::new()
    } else {
        format!(" _(keywords: {})_", words.join(", "))
    }
}

fn s<'a>(memory: &'a Value, key: &str) -> Option<&'a str> {
    memory.get(key).and_then(Value::as_str)
}

/// v4 `formatMemoryAge(memory, now)` over a row.
pub fn memory_age(memory: &Value, now_ms: f64) -> String {
    format_memory_age(
        date_ms(s(memory, "createdAt").unwrap_or("")),
        truthy_date_ms(s(memory, "lastReinforcedAt")),
        s(memory, "occurredAt"),
        now_ms,
    )
}

/// v4 `ageTag` — `[age]`, or `[age · narrative]` when the row carries a
/// narrative time.
pub fn age_tag(memory: &Value, now_ms: f64) -> String {
    let age = memory_age(memory, now_ms);
    match s(memory, "narrativeTime")
        .map(crate::jsstr::js_trim)
        .filter(|n| !n.is_empty())
    {
        Some(narrative) => format!("[{age} · {narrative}]"),
        None => format!("[{age}]"),
    }
}

/// `memory.summary?.trim() || memory.content?.trim() || ''`.
fn summary_first_body(memory: &Value) -> String {
    let summary = s(memory, "summary")
        .map(crate::jsstr::js_trim)
        .unwrap_or("");
    if !summary.is_empty() {
        return summary.to_string();
    }
    s(memory, "content")
        .map(crate::jsstr::js_trim)
        .unwrap_or("")
        .to_string()
}

/// `memory.content?.trim() || memory.summary` (the summary UNtrimmed; `""` when
/// absent).
fn content_first_body(memory: &Value) -> String {
    let content = s(memory, "content")
        .map(crate::jsstr::js_trim)
        .unwrap_or("");
    if !content.is_empty() {
        return content.to_string();
    }
    s(memory, "summary").unwrap_or("").to_string()
}

fn opt_summary(memory: &Value) -> Option<String> {
    s(memory, "summary").map(str::to_string)
}

fn opt_importance(memory: &Value) -> Option<f64> {
    memory.get("importance").and_then(Value::as_f64)
}

fn opt_id(memory: &Value) -> Option<String> {
    s(memory, "id").map(str::to_string)
}

/// v4 `renderArchive` — `## Memory Anchors`. Byte-stable within a compaction
/// generation: input order verbatim (the archive arrives id-sorted),
/// summaries only, and nothing that moves with the clock or the query.
pub fn render_archive(
    memories: &[Value],
    max_tokens: i64,
    provider: Option<&str>,
    subject: &MemorySubjectContext,
) -> RenderedSection<DebugMemoryInfo> {
    if memories.is_empty() {
        return RenderedSection::empty();
    }
    let cpt = chars_per_token(provider);
    let header = "## Memory Anchors";
    let mut parts: Vec<String> = vec![header.to_string()];
    let mut tokens = estimate_tokens(&format!("{header}\n"), cpt);
    let mut section = RenderedSection::<DebugMemoryInfo>::empty();
    for memory in memories {
        let body = summary_first_body(memory);
        if body.is_empty() {
            continue;
        }
        let prefix = format_memory_subject_prefix(s(memory, "aboutCharacterId"), subject);
        let line = format!(
            "- {prefix}{body}{}{}",
            format_digest_provenance(memory),
            format_keywords_tag(memory.get("keywords"))
        );
        let line_tokens = estimate_tokens(&format!("{line}\n"), cpt);
        if tokens + line_tokens > max_tokens {
            break;
        }
        parts.push(line.clone());
        tokens += line_tokens;
        section.lines.push(RenderedLine {
            memory_id: memory_id(memory).to_string(),
            line,
            body,
        });
        section.debug_memories.push(DebugMemoryInfo {
            memory_id: opt_id(memory),
            summary: opt_summary(memory),
            importance: opt_importance(memory),
            score: 0.0,
            salience: 0.0,
            recall_multiplier: None,
            recall_fired: None,
            blended_before: None,
            blended_after: None,
        });
    }
    if section.lines.is_empty() {
        return RenderedSection::empty();
    }
    section.content = parts.join("\n");
    section.token_count = tokens;
    section.memories_used = section.lines.len();
    section
}

/// v4 `renderHead`'s options.
#[derive(Clone, Copy, Debug)]
pub struct RenderHeadOptions {
    pub max_tokens: i64,
    pub max_entries: usize,
    /// The clock the age labels read (the replay pins the replayed turn's).
    pub now_ms: f64,
}

/// v4's salience-then-relevance comparator: salience when the two differ by
/// more than 0.05, else relevance.
fn salience_then_score(a: &RecallCandidate, b: &RecallCandidate) -> f64 {
    let weight_diff = b.salience - a.salience;
    if weight_diff.abs() > 0.05 {
        return weight_diff;
    }
    b.score - a.score
}

/// v4 `renderHead` — the per-turn head, `Most relevant memories for this
/// turn:`. Ranked by the post-adjustment blend when a recall context drove the
/// search, else by salience with relevance breaking near-ties; capped by
/// entries and tokens. Every line is dated: the age label, and any narrative
/// time.
pub fn render_head(
    candidates: &[RecallCandidate],
    provider: Option<&str>,
    subject: &MemorySubjectContext,
    options: RenderHeadOptions,
) -> RenderedSection<DebugMemoryInfo> {
    if candidates.is_empty() {
        return RenderedSection::empty();
    }
    let mut ranked: Vec<&RecallCandidate> = candidates.iter().collect();
    js_array_sort_by(&mut ranked, |a, b| {
        match (
            a.recall_adjustment.as_ref().map(|x| x.blended_after),
            b.recall_adjustment.as_ref().map(|x| x.blended_after),
        ) {
            (Some(a_adj), Some(b_adj)) => b_adj - a_adj,
            _ => salience_then_score(a, b),
        }
    });
    ranked.truncate(options.max_entries);

    let cpt = chars_per_token(provider);
    let header = "Most relevant memories for this turn:";
    let mut entries: Vec<String> = Vec::new();
    let mut tokens = estimate_tokens(&format!("{header}\n"), cpt);
    let mut section = RenderedSection::<DebugMemoryInfo>::empty();
    for c in ranked {
        let memory = &c.memory;
        let body = summary_first_body(memory);
        if body.is_empty() {
            continue;
        }
        let prefix = format_memory_subject_prefix(s(memory, "aboutCharacterId"), subject);
        let line = format!(
            "{} {prefix}{body}{}{}",
            age_tag(memory, options.now_ms),
            format_digest_provenance(memory),
            format_keywords_tag(memory.get("keywords"))
        );
        let line_tokens = estimate_tokens(&format!("{line}\n"), cpt);
        if tokens + line_tokens > options.max_tokens {
            break;
        }
        entries.push(line.clone());
        tokens += line_tokens;
        section.lines.push(RenderedLine {
            memory_id: memory_id(memory).to_string(),
            line,
            body,
        });
        let adj = c.recall_adjustment.as_ref();
        section.debug_memories.push(DebugMemoryInfo {
            memory_id: opt_id(memory),
            summary: opt_summary(memory),
            importance: opt_importance(memory),
            score: c.score,
            salience: c.salience,
            recall_multiplier: adj.map(|a| a.multiplier),
            recall_fired: adj.map(|a| a.fired.clone()),
            blended_before: adj.map(|a| a.blended_before),
            blended_after: adj.map(|a| a.blended_after),
        });
    }
    if section.lines.is_empty() {
        return RenderedSection::empty();
    }
    section.content = format!("{header}\n{}", entries.join("\n"));
    section.token_count = tokens;
    section.memories_used = section.lines.len();
    section
}

/// v4 `renderRelevant` — `## Relevant Memories`: full content, salience first,
/// for a one-off call that recalls against a question (Carina). No
/// provenance. v4 dates the lines on the wall clock (`new Date()`); the
/// caller's `now_ms` stands in.
pub fn render_relevant(
    candidates: &[RecallCandidate],
    max_tokens: i64,
    provider: Option<&str>,
    subject: &MemorySubjectContext,
    now_ms: f64,
) -> RenderedSection<DebugMemoryInfo> {
    if candidates.is_empty() {
        return RenderedSection::empty();
    }
    let cpt = chars_per_token(provider);
    let header = "## Relevant Memories";
    let mut parts: Vec<String> = vec![header.to_string()];
    let mut tokens = estimate_tokens(&format!("{header}\n"), cpt);
    let mut section = RenderedSection::<DebugMemoryInfo>::empty();
    let mut sorted: Vec<&RecallCandidate> = candidates.iter().collect();
    js_array_sort_by(&mut sorted, |a, b| salience_then_score(a, b));
    for c in sorted {
        let memory = &c.memory;
        let body = content_first_body(memory);
        let prefix = format_memory_subject_prefix(s(memory, "aboutCharacterId"), subject);
        let line = format!(
            "- {} {prefix}{body}{}",
            age_tag(memory, now_ms),
            format_keywords_tag(memory.get("keywords"))
        );
        let line_tokens = estimate_tokens(&format!("{line}\n"), cpt);
        if tokens + line_tokens > max_tokens {
            break;
        }
        parts.push(line.clone());
        tokens += line_tokens;
        section.lines.push(RenderedLine {
            memory_id: memory_id(memory).to_string(),
            line,
            body,
        });
        section.debug_memories.push(DebugMemoryInfo {
            memory_id: opt_id(memory),
            summary: opt_summary(memory),
            importance: opt_importance(memory),
            score: c.score,
            salience: c.salience,
            recall_multiplier: None,
            recall_fired: None,
            blended_before: None,
            blended_after: None,
        });
    }
    if section.lines.is_empty() {
        return RenderedSection::empty();
    }
    section.content = parts.join("\n");
    section.token_count = tokens;
    section.memories_used = section.lines.len();
    section
}

/// An insertion-ordered group-by on `aboutCharacterId` (v4's `Map` of arrays).
fn group_by_about<'a, T>(
    items: impl Iterator<Item = &'a T>,
    about: impl Fn(&T) -> Option<&str>,
) -> (Vec<String>, HashMap<String, Vec<&'a T>>)
where
    T: 'a,
{
    let mut order: Vec<String> = Vec::new();
    let mut by: HashMap<String, Vec<&'a T>> = HashMap::new();
    for item in items {
        let Some(id) = about(item).filter(|id| !id.is_empty()) else {
            continue;
        };
        if !by.contains_key(id) {
            order.push(id.to_string());
        }
        by.entry(id.to_string()).or_default().push(item);
    }
    (order, by)
}

/// v4 `renderInterCharacter` — `## Memories About Other Characters`: per
/// other character, the relevance half (rows about them from the turn's pool,
/// by relevance) interleaved with the importance half (digests first, then
/// salience), deduped by id and trimmed to `max_tokens`. A line over budget
/// ends THAT character's lines only; the next character still gets a chance.
pub fn render_inter_character(
    importance_memories: &[Value],
    character_names: &HashMap<String, String>,
    max_tokens: i64,
    provider: Option<&str>,
    relevance: &[RecallCandidate],
    now_ms: f64,
) -> RenderedSection<DebugInterCharacterMemoryInfo> {
    if importance_memories.is_empty() && relevance.is_empty() {
        return RenderedSection::empty();
    }
    let cpt = chars_per_token(provider);
    let header = "## Memories About Other Characters";
    let mut parts: Vec<String> = vec![header.to_string()];
    let mut tokens = estimate_tokens(&format!("{header}\n"), cpt);
    let mut section = RenderedSection::<DebugInterCharacterMemoryInfo>::empty();

    let (rel_order, relevance_by) =
        group_by_about(relevance.iter(), |r| s(&r.memory, "aboutCharacterId"));
    let (imp_order, importance_by) =
        group_by_about(importance_memories.iter(), |m| s(m, "aboutCharacterId"));
    let mut character_ids: Vec<String> = rel_order;
    for id in imp_order {
        if !character_ids.contains(&id) {
            character_ids.push(id);
        }
    }

    for character_id in &character_ids {
        // `characterNames.get(id) || 'Unknown'` — an empty name is falsy.
        let name = character_names
            .get(character_id)
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());
        let mut rel: Vec<&RecallCandidate> =
            relevance_by.get(character_id).cloned().unwrap_or_default();
        js_array_sort_by(&mut rel, |a, b| b.score - a.score);
        let rel: Vec<&Value> = rel.into_iter().map(|r| &r.memory).collect();
        let rel_ids: std::collections::HashSet<&str> = rel.iter().map(|m| memory_id(m)).collect();
        let mut imp: Vec<(&Value, f64)> = importance_by
            .get(character_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|m| !rel_ids.contains(memory_id(m)))
            .map(|m| (m, salience_of(m, now_ms)))
            .collect();
        js_array_sort_by(&mut imp, |a, b| {
            let da = f64::from(u8::from(s(a.0, "source") == Some("CONSOLIDATED")));
            let db = f64::from(u8::from(s(b.0, "source") == Some("CONSOLIDATED")));
            // `db - da || b.w - a.w` — a zero difference is falsy.
            let d = db - da;
            if d != 0.0 {
                d
            } else {
                b.1 - a.1
            }
        });
        let imp: Vec<&Value> = imp.into_iter().map(|(m, _)| m).collect();
        let mut ordered: Vec<&Value> = Vec::new();
        for i in 0..rel.len().max(imp.len()) {
            if let Some(m) = rel.get(i) {
                ordered.push(m);
            }
            if let Some(m) = imp.get(i) {
                ordered.push(m);
            }
        }
        for memory in ordered {
            let body = content_first_body(memory);
            let line = format!(
                "- About {name}: [{}] {body}{}{}",
                memory_age(memory, now_ms),
                format_digest_provenance(memory),
                format_keywords_tag(memory.get("keywords"))
            );
            let line_tokens = estimate_tokens(&format!("{line}\n"), cpt);
            if tokens + line_tokens > max_tokens {
                break;
            }
            parts.push(line.clone());
            tokens += line_tokens;
            section.lines.push(RenderedLine {
                memory_id: memory_id(memory).to_string(),
                line,
                body,
            });
            section.debug_memories.push(DebugInterCharacterMemoryInfo {
                memory_id: opt_id(memory),
                about_character_name: name.clone(),
                summary: opt_summary(memory),
                importance: opt_importance(memory),
            });
        }
    }
    if section.lines.is_empty() {
        return RenderedSection::empty();
    }
    section.content = parts.join("\n");
    section.token_count = tokens;
    section.memories_used = section.lines.len();
    section
}
