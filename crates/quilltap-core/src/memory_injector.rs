//! Port of v4's lib/chat/context/memory-injector.ts — the pure context-injection
//! formatters that render memories, scene state, the frozen archive, the dynamic
//! head, and the conversation summary into the Commonplace Book whisper / LLM
//! context blocks.
//!
//! Every string is byte-stable against the v4 output: whitespace, separators
//! (` · ` middle dots), the italicised `_(…)_` parenthetical, the ` days ago`
//! relative-age forms, JS `toFixed(2)` number rendering (via [`crate::jsnum`]),
//! and the token-budget cut boundaries (via [`crate::token_estimation`]).
//!
//! Token counts follow the no-provider path: v4's `estimateTokens(text)` with an
//! absent provider uses `CHARS_PER_TOKEN.default` (3.5) directly, so this module
//! calls [`crate::token_estimation::estimate_tokens`] with
//! [`crate::token_estimation::DEFAULT_CHARS_PER_TOKEN`]. Consumers that route a
//! real provider through the plugin registry are a later transport concern; the
//! pure formatter's arithmetic is provider-parameterised nowhere in v4 beyond the
//! chars-per-token rate, which the differential pins at the default.
//!
//! Sort order matters: v4 uses `Array.prototype.sort` (stable) with float
//! comparators, so ties preserve input order. Rust's `slice::sort_by` is stable,
//! and the comparators map `>0 → Greater`, `<0 → Less`, `==0 → Equal` exactly.
//!
//! The decayed `effectiveWeight` used for ranking + the debug records flows
//! through [`crate::memory_weighting::calculate_effective_weight`], whose
//! `0.5.powf(days/halfLife)` can differ from JS `Math.pow` by 1 ULP (a libm
//! difference in the already-oracle-matched weighting module). That last bit
//! never survives the `toFixed(2)` rendering, so every emitted string / token
//! count is byte-exact; the differential compares the raw debug floats at the
//! tier-1 1e-12 tolerance.

use crate::jsnum::to_fixed;
use crate::memory_weighting::{
    calculate_effective_weight, format_relative_age, MemoryInputs, DEFAULT_WEIGHTING_CONFIG,
};
use crate::token_estimation::{
    estimate_tokens as est_tokens_rate, truncate_to_token_limit, DEFAULT_CHARS_PER_TOKEN,
};
use sha2::{Digest, Sha256};

/// v4 `estimateTokens(text)` with no provider → default chars-per-token.
fn estimate_tokens(text: &str) -> i64 {
    est_tokens_rate(text, DEFAULT_CHARS_PER_TOKEN)
}

/// JS `String.prototype.trim()` — strips Unicode whitespace (the JS `\s` set +
/// the BOM/line/paragraph separators) from both ends. For the corpus we only
/// need the common ASCII + a few Unicode spaces; [`str::trim`] on the Rust side
/// strips `char::is_whitespace`, which matches JS `trim` on the whitespace this
/// module actually sees (ASCII spaces/tabs/newlines and NBSP-class spaces).
fn js_trim(s: &str) -> &str {
    s.trim()
}

const SCENE_HASH_LENGTH: usize = 16;

/// SHA-256 hex, first 16 chars — v4 `sceneHash`.
fn scene_hash(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let hex = hex::encode(digest);
    hex[..SCENE_HASH_LENGTH].to_string()
}

/// Phase 3b: hard cap on the dynamic-head rank instruction.
pub const DYNAMIC_HEAD_TOKEN_BUDGET: i64 = 200;
/// Phase 3b: how many rank-instruction entries to attempt.
pub const DYNAMIC_HEAD_DEFAULT_SIZE: usize = 5;

// ---------------------------------------------------------------------------
// formatMemoryMetadataTag
// ---------------------------------------------------------------------------

/// Options for [`format_memory_metadata_tag`]. Each numeric field is `Option`;
/// v4 renders a number only when it is a finite JS `number`. `NaN`/`Infinity`
/// are dropped (v4's `Number.isFinite`), so callers pass `None` for those.
#[derive(Default)]
pub struct MetadataTagOpts<'a> {
    pub importance: Option<f64>,
    pub relevance: Option<f64>,
    pub weight: Option<f64>,
    pub keywords: Option<&'a [String]>,
    pub adjustments: Option<&'a [String]>,
}

/// Build the trailing metadata tag appended to a delivered memory line.
///
/// Returns `""` when no fields are provided so callers can append
/// unconditionally. Format:
/// ` _(importance 0.98 · relevance 0.87 · weight 0.92 · keywords: a, b)_`
/// — leading space, italicised parenthetical, middle-dot separators, trailing
/// keywords list when non-empty. Numbers are rounded to 2 decimals (JS
/// `toFixed(2)`).
pub fn format_memory_metadata_tag(opts: &MetadataTagOpts) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = opts.importance {
        if v.is_finite() {
            parts.push(format!("importance {}", to_fixed(v, 2)));
        }
    }
    if let Some(v) = opts.relevance {
        if v.is_finite() {
            parts.push(format!("relevance {}", to_fixed(v, 2)));
        }
    }
    if let Some(v) = opts.weight {
        if v.is_finite() {
            parts.push(format!("weight {}", to_fixed(v, 2)));
        }
    }
    if let Some(adj) = opts.adjustments {
        if !adj.is_empty() {
            let trimmed: Vec<&str> = adj
                .iter()
                .map(|a| js_trim(a))
                .filter(|a| !a.is_empty())
                .collect();
            if !trimmed.is_empty() {
                parts.push(format!("recall: {}", trimmed.join(" ")));
            }
        }
    }
    if let Some(kw) = opts.keywords {
        if !kw.is_empty() {
            let trimmed: Vec<&str> = kw
                .iter()
                .map(|k| js_trim(k))
                .filter(|k| !k.is_empty())
                .collect();
            if !trimmed.is_empty() {
                parts.push(format!("keywords: {}", trimmed.join(", ")));
            }
        }
    }
    if parts.is_empty() {
        return String::new();
    }
    format!(" _({})_", parts.join(" · "))
}

// ---------------------------------------------------------------------------
// Input shapes (the Memory / SemanticSearchResult subset the formatters read).
// ---------------------------------------------------------------------------

/// The subset of a `Memory` these formatters consume, plus the epoch-ms times
/// the weighting/age functions need (the harness converts ISO → ms the way
/// `new Date(x)` does).
#[derive(Clone, Default)]
pub struct InjectorMemory {
    pub id: String,
    pub content: Option<String>,
    pub summary: String,
    pub importance: f64,
    pub keywords: Vec<String>,
    pub about_character_id: Option<String>,
    // Weighting inputs (mirror MemoryInputs).
    pub reinforced_importance: Option<f64>,
    pub created_at_ms: f64,
    pub last_reinforced_at_ms: Option<f64>,
    pub last_accessed_at_ms: Option<f64>,
    pub reinforcement_count: Option<u64>,
    pub graph_degree: usize,
    /// Episodic spine (v4 8bf3cb5f): `memory.kind === 'episodic'`.
    pub kind_episodic: bool,
    /// Pre-parsed event time (build via [`crate::episodic::event_time_ms`]) —
    /// the age labels read this clock when present.
    pub occurred_at_ms: Option<f64>,
}

impl InjectorMemory {
    fn weighting(&self) -> MemoryInputs {
        MemoryInputs {
            importance: self.importance,
            reinforced_importance: self.reinforced_importance,
            created_at_ms: self.created_at_ms,
            last_reinforced_at_ms: self.last_reinforced_at_ms,
            last_accessed_at_ms: self.last_accessed_at_ms,
            reinforcement_count: self.reinforcement_count,
            graph_degree: self.graph_degree,
            kind_episodic: self.kind_episodic,
            occurred_at_ms: self.occurred_at_ms,
        }
    }

    fn effective_weight(&self, now_ms: f64) -> f64 {
        calculate_effective_weight(&self.weighting(), &DEFAULT_WEIGHTING_CONFIG, now_ms)
            .effective_weight
    }

    /// v4 `memory.content?.trim() || memory.summary` — full content, falling
    /// back to the summary when content is absent or trims to empty.
    fn body(&self) -> String {
        match &self.content {
            Some(c) if !js_trim(c).is_empty() => js_trim(c).to_string(),
            _ => self.summary.clone(),
        }
    }

    /// v4 `memory.summary?.trim() || memory.content?.trim() || ''` — summary
    /// first (used by the frozen archive + dynamic head).
    fn summary_or_content(&self) -> String {
        let s = js_trim(&self.summary);
        if !s.is_empty() {
            return s.to_string();
        }
        match &self.content {
            Some(c) if !js_trim(c).is_empty() => js_trim(c).to_string(),
            _ => String::new(),
        }
    }
}

/// The `recallAdjustment` record carried on a semantic search result.
#[derive(Clone, Default)]
pub struct RecallAdjustment {
    pub multiplier: Option<f64>,
    pub fired: Vec<String>,
    pub blended_before: Option<f64>,
    pub blended_after: Option<f64>,
}

/// The subset of a `SemanticSearchResult` the formatters read.
#[derive(Clone)]
pub struct InjectorResult {
    pub memory: InjectorMemory,
    pub score: f64,
    /// v4's `effectiveWeight?` — when `Some`, used verbatim instead of
    /// recomputing via `calculateEffectiveWeight`.
    pub effective_weight: Option<f64>,
    pub raw_weight: Option<f64>,
    pub recall_adjustment: Option<RecallAdjustment>,
}

// ---------------------------------------------------------------------------
// Debug info structs (returned alongside the content).
// ---------------------------------------------------------------------------

/// Debug info for included memories.
#[derive(Clone, Debug, PartialEq)]
pub struct DebugMemoryInfo {
    /// v4 `memoryId?` — omitted (None) except in the dynamic head.
    pub memory_id: Option<String>,
    pub summary: String,
    pub importance: f64,
    pub score: f64,
    pub effective_weight: f64,
    pub raw_weight: Option<f64>,
    pub recall_multiplier: Option<f64>,
    pub recall_fired: Option<Vec<String>>,
    pub blended_before: Option<f64>,
    pub blended_after: Option<f64>,
}

/// Debug info for inter-character memories.
#[derive(Clone, Debug, PartialEq)]
pub struct DebugInterCharacterMemoryInfo {
    pub about_character_name: String,
    pub summary: String,
    pub importance: f64,
}

/// Result of formatting memories for context.
pub struct FormattedMemoriesResult {
    pub content: String,
    pub token_count: i64,
    pub memories_used: usize,
    pub debug_memories: Vec<DebugMemoryInfo>,
}

/// Result of formatting inter-character memories for context.
pub struct FormattedInterCharacterMemoriesResult {
    pub content: String,
    pub token_count: i64,
    pub memories_used: usize,
    pub debug_memories: Vec<DebugInterCharacterMemoryInfo>,
}

fn empty_memories_result() -> FormattedMemoriesResult {
    FormattedMemoriesResult {
        content: String::new(),
        token_count: 0,
        memories_used: 0,
        debug_memories: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// formatCurrentSceneState
// ---------------------------------------------------------------------------

/// One scene-state character (the subset `formatCurrentSceneState` reads).
#[derive(Clone)]
pub struct SceneStateCharacter {
    pub character_id: String,
    pub character_name: String,
    pub action: String,
    /// v4's `clothing` is `string | null`; `None` = null.
    pub clothing: Option<String>,
}

/// The scene state (the subset the formatter reads).
#[derive(Clone)]
pub struct SceneState {
    pub location: String,
    pub characters: Vec<SceneStateCharacter>,
}

/// Per-target emission record for a single character's slice of the scene-state
/// whisper (v4 `SceneStateEmissionEntry`).
#[derive(Clone, Debug, PartialEq)]
pub struct SceneStateEmissionEntry {
    pub action_hash: String,
    pub clothing_hash: String,
    pub emitted_at: String,
}

/// Result of [`format_current_scene_state`].
pub struct SceneStateResult {
    pub content: String,
    pub token_count: i64,
    /// Preserves emission order (v4 returns a `Map`; iteration order = insertion
    /// order). A `Vec` of `(characterId, entry)` keeps that order for the
    /// differential.
    pub emitted_by_character: Vec<(String, SceneStateEmissionEntry)>,
}

/// Render the latest scene-state snapshot as the `## Current State` section.
/// Returns empty content when `scene_state` is `None`.
///
/// `time` is the chat's announced timestamp; `None`/blank drops the Time line.
/// `now_iso` is v4's `new Date().toISOString()` — injected here for determinism
/// (v4 computes it inline). `live_clothing_by_character_id` overrides each
/// character's clothing; `prior_emission_by_character` is the per-target cache
/// that collapses an unchanged section to `### Name — _unchanged_`.
pub fn format_current_scene_state(
    scene_state: Option<&SceneState>,
    time: Option<&str>,
    now_iso: &str,
    live_clothing_by_character_id: &dyn Fn(&str) -> Option<String>,
    prior_emission_by_character: &dyn Fn(&str) -> Option<SceneStateEmissionEntry>,
) -> SceneStateResult {
    let scene_state = match scene_state {
        Some(s) => s,
        None => {
            return SceneStateResult {
                content: String::new(),
                token_count: 0,
                emitted_by_character: Vec::new(),
            };
        }
    };

    let names: Vec<&str> = scene_state
        .characters
        .iter()
        .map(|c| c.character_name.as_str())
        .filter(|n| !n.is_empty())
        .collect();

    let mut lines: Vec<String> = vec!["## Current State".to_string(), String::new()];
    lines.push(format!(
        "- **Location**: {}",
        if scene_state.location.is_empty() {
            "Unknown"
        } else {
            &scene_state.location
        }
    ));
    lines.push(format!("- **Characters Present**: {}", names.join(", ")));
    if let Some(t) = time {
        if !js_trim(t).is_empty() {
            lines.push(format!("- **Time**: {}", js_trim(t)));
        }
    }
    lines.push("- **Active Now**: true".to_string());

    let mut emitted_by_character: Vec<(String, SceneStateEmissionEntry)> = Vec::new();

    for c in &scene_state.characters {
        if c.character_name.is_empty() {
            continue;
        }

        let action_text = {
            let t = js_trim(&c.action);
            if t.is_empty() {
                "_unspecified_".to_string()
            } else {
                t.to_string()
            }
        };
        // liveClothing || (c.clothing ?? '').trim() || '_unspecified_'
        let live_clothing = live_clothing_by_character_id(&c.character_id)
            .map(|s| js_trim(&s).to_string())
            .filter(|s| !s.is_empty());
        let clothing_text = live_clothing.unwrap_or_else(|| {
            let cached = c.clothing.as_deref().unwrap_or("");
            let t = js_trim(cached);
            if t.is_empty() {
                "_unspecified_".to_string()
            } else {
                t.to_string()
            }
        });
        let action_hash = scene_hash(&action_text);
        let clothing_hash = scene_hash(&clothing_text);

        let prior = if !c.character_id.is_empty() {
            prior_emission_by_character(&c.character_id)
        } else {
            None
        };
        let unchanged = match &prior {
            Some(p) => p.action_hash == action_hash && p.clothing_hash == clothing_hash,
            None => false,
        };

        lines.push(String::new());
        if unchanged {
            lines.push(format!("### {} — _unchanged_", c.character_name));
        } else {
            lines.push(format!("### {}", c.character_name));
            lines.push(String::new());
            lines.push("#### Action".to_string());
            lines.push(String::new());
            lines.push(action_text.clone());
            lines.push(String::new());
            lines.push("#### Clothing".to_string());
            lines.push(String::new());
            lines.push(clothing_text.clone());
        }

        if !c.character_id.is_empty() {
            let emitted_at = if unchanged {
                match &prior {
                    Some(p) => p.emitted_at.clone(),
                    None => now_iso.to_string(),
                }
            } else {
                now_iso.to_string()
            };
            emitted_by_character.push((
                c.character_id.clone(),
                SceneStateEmissionEntry {
                    action_hash,
                    clothing_hash,
                    emitted_at,
                },
            ));
        }
    }

    let content = lines.join("\n");
    let token_count = estimate_tokens(&format!("{content}\n"));
    SceneStateResult {
        content,
        token_count,
        emitted_by_character,
    }
}

// ---------------------------------------------------------------------------
// formatMemoriesForContext
// ---------------------------------------------------------------------------

/// Map a JS float-comparator difference to an `Ordering`, matching
/// `Array.prototype.sort` (`>0 → Greater`, `<0 → Less`, `==0 → Equal`; a stable
/// sort preserves ties).
fn cmp_num(diff: f64) -> std::cmp::Ordering {
    diff.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
}

/// Format memories for injection into context.
pub fn format_memories_for_context(
    memories: &[InjectorResult],
    max_tokens: i64,
    now_ms: f64,
) -> FormattedMemoriesResult {
    if memories.is_empty() {
        return empty_memories_result();
    }

    let mut memory_parts: Vec<String> = vec!["## Relevant Memories".to_string()];
    let mut current_tokens = estimate_tokens("## Relevant Memories\n");
    let mut memories_used = 0usize;
    let mut debug_memories: Vec<DebugMemoryInfo> = Vec::new();

    // weight = effectiveWeight ?? calculateEffectiveWeight(memory).effectiveWeight
    let mut with_weight: Vec<(&InjectorResult, f64)> = memories
        .iter()
        .map(|r| {
            let weight = r
                .effective_weight
                .unwrap_or_else(|| r.memory.effective_weight(now_ms));
            (r, weight)
        })
        .collect();

    // sort by weight (primary), score (tiebreaker within 0.05).
    with_weight.sort_by(|(ra, wa), (rb, wb)| {
        let weight_diff = wb - wa;
        if weight_diff.abs() > 0.05 {
            return cmp_num(weight_diff);
        }
        cmp_num(rb.score - ra.score)
    });

    for (r, weight) in &with_weight {
        let age = format_relative_age(&r.memory.weighting(), now_ms);
        let body = r.memory.body();
        let meta = format_memory_metadata_tag(&MetadataTagOpts {
            importance: Some(r.memory.importance),
            relevance: Some(r.score),
            weight: Some(*weight),
            keywords: Some(&r.memory.keywords),
            adjustments: None,
        });
        let memory_line = format!("- [{age}] {body}{meta}");
        let line_tokens = estimate_tokens(&format!("{memory_line}\n"));

        if current_tokens + line_tokens > max_tokens {
            break;
        }

        memory_parts.push(memory_line);
        current_tokens += line_tokens;
        memories_used += 1;
        debug_memories.push(DebugMemoryInfo {
            memory_id: None,
            summary: r.memory.summary.clone(),
            importance: r.memory.importance,
            score: r.score,
            effective_weight: *weight,
            raw_weight: None,
            recall_multiplier: None,
            recall_fired: None,
            blended_before: None,
            blended_after: None,
        });
    }

    if memories_used == 0 {
        return empty_memories_result();
    }

    FormattedMemoriesResult {
        content: memory_parts.join("\n"),
        token_count: current_tokens,
        memories_used,
        debug_memories,
    }
}

// ---------------------------------------------------------------------------
// formatInterCharacterMemoriesForContext
// ---------------------------------------------------------------------------

/// Format inter-character memories for injection into context. `character_names`
/// resolves `aboutCharacterId → name`. `importance_memories` is the direct-DB
/// half (no per-turn score); `relevance_results` is the semantic half.
pub fn format_inter_character_memories_for_context(
    importance_memories: &[InjectorMemory],
    character_names: &dyn Fn(&str) -> Option<String>,
    max_tokens: i64,
    relevance_results: &[InjectorResult],
    now_ms: f64,
) -> FormattedInterCharacterMemoriesResult {
    if importance_memories.is_empty() && relevance_results.is_empty() {
        return FormattedInterCharacterMemoriesResult {
            content: String::new(),
            token_count: 0,
            memories_used: 0,
            debug_memories: Vec::new(),
        };
    }

    let mut memory_parts: Vec<String> = vec!["## Memories About Other Characters".to_string()];
    let mut current_tokens = estimate_tokens("## Memories About Other Characters\n");
    let mut memories_used = 0usize;
    let mut debug_memories: Vec<DebugInterCharacterMemoryInfo> = Vec::new();

    // Group both sources by about-character, preserving first-seen key order
    // (JS Map iteration order = insertion order).
    let mut relevance_keys: Vec<String> = Vec::new();
    let mut relevance_by_character: std::collections::HashMap<String, Vec<&InjectorResult>> =
        std::collections::HashMap::new();
    for result in relevance_results {
        let about_id = match &result.memory.about_character_id {
            Some(id) => id.clone(),
            None => continue,
        };
        relevance_by_character
            .entry(about_id.clone())
            .or_insert_with(|| {
                relevance_keys.push(about_id.clone());
                Vec::new()
            })
            .push(result);
    }
    let mut importance_keys: Vec<String> = Vec::new();
    let mut importance_by_character: std::collections::HashMap<String, Vec<&InjectorMemory>> =
        std::collections::HashMap::new();
    for memory in importance_memories {
        let about_id = match &memory.about_character_id {
            Some(id) => id.clone(),
            None => continue,
        };
        importance_by_character
            .entry(about_id.clone())
            .or_insert_with(|| {
                importance_keys.push(about_id.clone());
                Vec::new()
            })
            .push(memory);
    }

    // new Set([...relevanceKeys, ...importanceKeys]) — insertion-ordered, deduped.
    let mut character_ids: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for id in relevance_keys.iter().chain(importance_keys.iter()) {
        if seen.insert(id.clone()) {
            character_ids.push(id.clone());
        }
    }

    struct Entry<'a> {
        memory: &'a InjectorMemory,
        weight: f64,
        relevance: Option<f64>,
    }

    'outer: for character_id in &character_ids {
        let character_name = character_names(character_id).unwrap_or_else(|| "Unknown".to_string());

        // Relevance half — sorted by cosine score desc; carries its relevance.
        // weight = r.effectiveWeight ?? calculateEffectiveWeight(r.memory).
        let mut relevance_entries: Vec<Entry> = relevance_by_character
            .get(character_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|r| Entry {
                memory: &r.memory,
                weight: r
                    .effective_weight
                    .unwrap_or_else(|| r.memory.effective_weight(now_ms)),
                relevance: Some(r.score),
            })
            .collect();
        relevance_entries.sort_by(|a, b| {
            let bv = b.relevance.unwrap_or(0.0);
            let av = a.relevance.unwrap_or(0.0);
            cmp_num(bv - av)
        });

        // Importance half — sorted by effective weight desc; excluding ids in
        // the relevance half (dedup keeps the relevance copy).
        let relevance_ids: std::collections::HashSet<&str> = relevance_entries
            .iter()
            .map(|e| e.memory.id.as_str())
            .collect();
        let mut importance_entries: Vec<Entry> = importance_by_character
            .get(character_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter(|m| !relevance_ids.contains(m.id.as_str()))
            .map(|m| Entry {
                memory: m,
                weight: m.effective_weight(now_ms),
                relevance: None,
            })
            .collect();
        importance_entries.sort_by(|a, b| cmp_num(b.weight - a.weight));

        // Interleave relevance-first.
        let mut ordered: Vec<Entry> = Vec::new();
        let mut ri = 0usize;
        let mut ii = 0usize;
        while ri < relevance_entries.len() || ii < importance_entries.len() {
            if ri < relevance_entries.len() {
                ordered.push(Entry {
                    memory: relevance_entries[ri].memory,
                    weight: relevance_entries[ri].weight,
                    relevance: relevance_entries[ri].relevance,
                });
                ri += 1;
            }
            if ii < importance_entries.len() {
                ordered.push(Entry {
                    memory: importance_entries[ii].memory,
                    weight: importance_entries[ii].weight,
                    relevance: importance_entries[ii].relevance,
                });
                ii += 1;
            }
        }

        for entry in &ordered {
            let memory = entry.memory;
            let weight = entry.weight;
            let age = format_relative_age(&memory.weighting(), now_ms);
            let body = memory.body();
            let meta = format_memory_metadata_tag(&MetadataTagOpts {
                importance: Some(memory.importance),
                relevance: entry.relevance,
                weight: Some(weight),
                keywords: Some(&memory.keywords),
                adjustments: None,
            });
            let memory_line = format!("- About {character_name}: [{age}] {body}{meta}");
            let line_tokens = estimate_tokens(&format!("{memory_line}\n"));

            if current_tokens + line_tokens > max_tokens {
                break 'outer;
            }

            memory_parts.push(memory_line);
            current_tokens += line_tokens;
            memories_used += 1;
            debug_memories.push(DebugInterCharacterMemoryInfo {
                about_character_name: character_name.clone(),
                summary: memory.summary.clone(),
                importance: memory.importance,
            });
        }
    }

    if memories_used == 0 {
        return FormattedInterCharacterMemoriesResult {
            content: String::new(),
            token_count: 0,
            memories_used: 0,
            debug_memories: Vec::new(),
        };
    }

    FormattedInterCharacterMemoriesResult {
        content: memory_parts.join("\n"),
        token_count: current_tokens,
        memories_used,
        debug_memories,
    }
}

// ---------------------------------------------------------------------------
// formatFrozenMemoryArchive
// ---------------------------------------------------------------------------

/// Phase 3a: Format the frozen-archive memory pool for context. The input order
/// is verbatim (already sorted by the caller); no per-turn re-ranking. Uses
/// `summary` (falling back to content).
pub fn format_frozen_memory_archive(
    memories: &[InjectorMemory],
    max_tokens: i64,
) -> FormattedMemoriesResult {
    if memories.is_empty() {
        return empty_memories_result();
    }

    let header = "## Memory Anchors";
    let mut memory_parts: Vec<String> = vec![header.to_string()];
    let mut current_tokens = estimate_tokens(&format!("{header}\n"));
    let mut memories_used = 0usize;
    let mut debug_memories: Vec<DebugMemoryInfo> = Vec::new();

    for memory in memories {
        let summary = memory.summary_or_content();
        if summary.is_empty() {
            continue;
        }

        // Only static per-memory fields belong here (no weight/relevance).
        let meta = format_memory_metadata_tag(&MetadataTagOpts {
            importance: Some(memory.importance),
            keywords: Some(&memory.keywords),
            ..Default::default()
        });
        let memory_line = format!("- {summary}{meta}");
        let line_tokens = estimate_tokens(&format!("{memory_line}\n"));
        if current_tokens + line_tokens > max_tokens {
            break;
        }

        memory_parts.push(memory_line);
        current_tokens += line_tokens;
        memories_used += 1;
        debug_memories.push(DebugMemoryInfo {
            memory_id: None,
            summary: memory.summary.clone(),
            importance: memory.importance,
            score: 0.0,
            effective_weight: 0.0,
            raw_weight: None,
            recall_multiplier: None,
            recall_fired: None,
            blended_before: None,
            blended_after: None,
        });
    }

    if memories_used == 0 {
        return empty_memories_result();
    }

    FormattedMemoriesResult {
        content: memory_parts.join("\n"),
        token_count: current_tokens,
        memories_used,
        debug_memories,
    }
}

// ---------------------------------------------------------------------------
// formatDynamicMemoryHead
// ---------------------------------------------------------------------------

/// Phase 3b: Format a compact "dynamic head" rank instruction. Hard-capped at
/// `max_tokens` (default [`DYNAMIC_HEAD_TOKEN_BUDGET`]), `max_entries` default
/// [`DYNAMIC_HEAD_DEFAULT_SIZE`]. Uses `summary` + a 4-char id prefix.
pub fn format_dynamic_memory_head(
    memories: &[InjectorResult],
    max_tokens: Option<i64>,
    max_entries: Option<usize>,
    now_ms: f64,
) -> FormattedMemoriesResult {
    let max_tokens = max_tokens.unwrap_or(DYNAMIC_HEAD_TOKEN_BUDGET);
    let max_entries = max_entries.unwrap_or(DYNAMIC_HEAD_DEFAULT_SIZE);

    if memories.is_empty() {
        return empty_memories_result();
    }

    // weight = effectiveWeight ?? calc; then sort.
    let mut ranked: Vec<(&InjectorResult, f64)> = memories
        .iter()
        .map(|r| {
            let weight = r
                .effective_weight
                .unwrap_or_else(|| r.memory.effective_weight(now_ms));
            (r, weight)
        })
        .collect();

    ranked.sort_by(|(ra, wa), (rb, wb)| {
        let a_adj = ra
            .recall_adjustment
            .as_ref()
            .and_then(|adj| adj.blended_after);
        let b_adj = rb
            .recall_adjustment
            .as_ref()
            .and_then(|adj| adj.blended_after);
        if let (Some(a), Some(b)) = (a_adj, b_adj) {
            return cmp_num(b - a);
        }
        let weight_diff = wb - wa;
        if weight_diff.abs() > 0.05 {
            return cmp_num(weight_diff);
        }
        cmp_num(rb.score - ra.score)
    });
    ranked.truncate(max_entries);

    let header = "Most relevant memories for this turn:";
    let mut entries: Vec<String> = Vec::new();
    let mut debug_memories: Vec<DebugMemoryInfo> = Vec::new();
    let mut current_tokens = estimate_tokens(&format!("{header}\n"));
    let mut memories_used = 0usize;

    for (r, weight) in &ranked {
        let summary = r.memory.summary_or_content();
        if summary.is_empty() {
            continue;
        }
        // `[m_${memory.id.slice(0, 4)}]` — JS slice on UTF-16 code units.
        let id_tag = format!("[m_{}]", js_slice_0_4(&r.memory.id));
        let fired = r.recall_adjustment.as_ref().map(|adj| adj.fired.clone());
        let meta = format_memory_metadata_tag(&MetadataTagOpts {
            importance: Some(r.memory.importance),
            relevance: Some(r.score),
            weight: Some(*weight),
            keywords: Some(&r.memory.keywords),
            adjustments: fired.as_deref(),
        });
        let entry = format!("{id_tag} {summary}{meta}");
        let candidate_tokens = estimate_tokens(&format!("{entry}\n"));
        if current_tokens + candidate_tokens > max_tokens {
            break;
        }
        entries.push(entry);
        current_tokens += candidate_tokens;
        memories_used += 1;
        debug_memories.push(DebugMemoryInfo {
            memory_id: Some(r.memory.id.clone()),
            summary: r.memory.summary.clone(),
            importance: r.memory.importance,
            score: r.score,
            effective_weight: *weight,
            raw_weight: r.raw_weight,
            recall_multiplier: r.recall_adjustment.as_ref().and_then(|adj| adj.multiplier),
            recall_fired: r.recall_adjustment.as_ref().map(|adj| adj.fired.clone()),
            blended_before: r
                .recall_adjustment
                .as_ref()
                .and_then(|adj| adj.blended_before),
            blended_after: r
                .recall_adjustment
                .as_ref()
                .and_then(|adj| adj.blended_after),
        });
    }

    if memories_used == 0 {
        return empty_memories_result();
    }

    FormattedMemoriesResult {
        content: format!("{header}\n{}", entries.join("\n")),
        token_count: current_tokens,
        memories_used,
        debug_memories,
    }
}

/// JS `str.slice(0, 4)` on UTF-16 code units. Ids are hyphenated hex (BMP), but
/// keep the code-unit semantics faithful for the general case.
fn js_slice_0_4(s: &str) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    let end = 4.min(units.len());
    String::from_utf16_lossy(&units[..end])
}

// ---------------------------------------------------------------------------
// formatSummaryForContext
// ---------------------------------------------------------------------------

/// Format conversation summary for context. Returns `(content, token_count)`.
pub fn format_summary_for_context(summary: &str, max_tokens: i64) -> (String, i64) {
    if summary.is_empty() || js_trim(summary).is_empty() {
        return (String::new(), 0);
    }

    let header = "## Previous Conversation Summary";
    let full_content = format!("{header}\n{summary}");
    let full_tokens = estimate_tokens(&full_content);

    if full_tokens <= max_tokens {
        return (full_content, full_tokens);
    }

    let header_tokens = estimate_tokens(&format!("{header}\n"));
    let available_for_summary = max_tokens - header_tokens;
    let truncated_summary = truncate_to_token_limit(
        summary,
        available_for_summary,
        DEFAULT_CHARS_PER_TOKEN,
        "...",
    );

    let content = format!("{header}\n{truncated_summary}");
    let token_count = estimate_tokens(&content);
    (content, token_count)
}
