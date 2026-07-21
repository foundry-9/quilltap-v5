//! Tier-1 differential test (chat-orchestration wave 1.3): the pure
//! context-injection formatters from v4's lib/chat/context/memory-injector.ts.
//!
//! Covers formatMemoryMetadataTag, formatCurrentSceneState,
//! formatMemoriesForContext, formatInterCharacterMemoriesForContext,
//! formatFrozenMemoryArchive, formatDynamicMemoryHead, formatSummaryForContext.
//! Every string / count / debug field is compared exactly.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-injector.ts \
//!     > /tmp/oracle-memory-injector.ndjson
//! Run:
//!   QT_ORACLE_MEMORY_INJECTOR=/tmp/oracle-memory-injector.ndjson \
//!     cargo test -p quilltap-harness --test memory_injector_equivalence

use quilltap_core::memory_injector::{
    format_current_scene_state, format_dynamic_memory_head, format_frozen_memory_archive,
    format_inter_character_memories_for_context, format_memories_for_context,
    format_memory_metadata_tag, format_summary_for_context, DebugInterCharacterMemoryInfo,
    DebugMemoryInfo, InjectorMemory, InjectorResult, MetadataTagOpts, RecallAdjustment, SceneState,
    SceneStateCharacter, SceneStateEmissionEntry,
};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Wire shapes
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct WireMem {
    id: String,
    content: Option<String>,
    summary: String,
    importance: f64,
    keywords: Vec<String>,
    #[serde(rename = "aboutCharacterId")]
    about_character_id: Option<String>,
    #[serde(rename = "reinforcedImportance")]
    reinforced_importance: Option<f64>,
    #[serde(rename = "createdAtMs")]
    created_at_ms: f64,
    #[serde(rename = "lastReinforcedAtMs")]
    last_reinforced_at_ms: Option<f64>,
    #[serde(rename = "lastAccessedAtMs")]
    last_accessed_at_ms: Option<f64>,
    #[serde(rename = "reinforcementCount")]
    reinforcement_count: Option<u64>,
    #[serde(rename = "graphDegree")]
    graph_degree: usize,
    // Episodic spine (v4 8bf3cb5f) — absent in older specs → defaults.
    #[serde(rename = "kindEpisodic", default)]
    kind_episodic: bool,
    #[serde(rename = "occurredAtMs", default)]
    occurred_at_ms: Option<f64>,
    #[serde(rename = "narrativeTime", default)]
    narrative_time: Option<String>,
}

impl WireMem {
    fn into_mem(self) -> InjectorMemory {
        InjectorMemory {
            id: self.id,
            content: self.content,
            summary: self.summary,
            importance: self.importance,
            keywords: self.keywords,
            about_character_id: self.about_character_id,
            reinforced_importance: self.reinforced_importance,
            created_at_ms: self.created_at_ms,
            last_reinforced_at_ms: self.last_reinforced_at_ms,
            last_accessed_at_ms: self.last_accessed_at_ms,
            reinforcement_count: self.reinforcement_count,
            graph_degree: self.graph_degree,
            kind_episodic: self.kind_episodic,
            occurred_at_ms: self.occurred_at_ms,
            narrative_time: self.narrative_time,
        }
    }
}

#[derive(Deserialize)]
struct WireRecall {
    multiplier: Option<f64>,
    fired: Vec<String>,
    #[serde(rename = "blendedBefore")]
    blended_before: Option<f64>,
    #[serde(rename = "blendedAfter")]
    blended_after: Option<f64>,
}

#[derive(Deserialize)]
struct WireResult {
    memory: WireMem,
    score: f64,
    #[serde(rename = "effectiveWeight")]
    effective_weight: Option<f64>,
    #[serde(rename = "rawWeight")]
    raw_weight: Option<f64>,
    #[serde(rename = "recallAdjustment")]
    recall_adjustment: Option<WireRecall>,
}

impl WireResult {
    fn into_result(self) -> InjectorResult {
        InjectorResult {
            memory: self.memory.into_mem(),
            score: self.score,
            effective_weight: self.effective_weight,
            raw_weight: self.raw_weight,
            recall_adjustment: self.recall_adjustment.map(|r| RecallAdjustment {
                multiplier: r.multiplier,
                fired: r.fired,
                blended_before: r.blended_before,
                blended_after: r.blended_after,
            }),
        }
    }
}

#[derive(Deserialize)]
struct WireTagOpts {
    importance: Option<f64>,
    relevance: Option<f64>,
    weight: Option<f64>,
    keywords: Option<Vec<String>>,
    adjustments: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct WireDebugMem {
    #[serde(rename = "memoryId", default)]
    memory_id: Option<String>,
    summary: String,
    importance: f64,
    score: f64,
    #[serde(rename = "effectiveWeight")]
    effective_weight: f64,
    #[serde(rename = "rawWeight", default)]
    raw_weight: Option<f64>,
    #[serde(rename = "recallMultiplier", default)]
    recall_multiplier: Option<f64>,
    #[serde(rename = "recallFired", default)]
    recall_fired: Option<Vec<String>>,
    #[serde(rename = "blendedBefore", default)]
    blended_before: Option<f64>,
    #[serde(rename = "blendedAfter", default)]
    blended_after: Option<f64>,
}

#[derive(Deserialize)]
struct WireDebugInter {
    #[serde(rename = "aboutCharacterName")]
    about_character_name: String,
    summary: String,
    importance: f64,
}

#[derive(Deserialize)]
struct WireMemOut {
    content: String,
    #[serde(rename = "tokenCount")]
    token_count: i64,
    #[serde(rename = "memoriesUsed")]
    memories_used: usize,
    #[serde(rename = "debugMemories")]
    debug_memories: Vec<WireDebugMem>,
}

#[derive(Deserialize)]
struct WireInterOut {
    content: String,
    #[serde(rename = "tokenCount")]
    token_count: i64,
    #[serde(rename = "memoriesUsed")]
    memories_used: usize,
    #[serde(rename = "debugMemories")]
    debug_memories: Vec<WireDebugInter>,
}

#[derive(Deserialize)]
struct WireSceneChar {
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "characterName")]
    character_name: String,
    action: String,
    clothing: Option<String>,
}

#[derive(Deserialize)]
struct WireScene {
    location: String,
    characters: Vec<WireSceneChar>,
}

#[derive(Deserialize)]
struct WireEmission {
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "actionHash")]
    action_hash: String,
    #[serde(rename = "clothingHash")]
    clothing_hash: String,
    #[serde(rename = "emittedAt")]
    emitted_at: String,
}

#[derive(Deserialize)]
struct WirePriorEmission(String, WirePriorEntry);

#[derive(Deserialize)]
struct WirePriorEntry {
    #[serde(rename = "actionHash")]
    action_hash: String,
    #[serde(rename = "clothingHash")]
    clothing_hash: String,
    #[serde(rename = "emittedAt")]
    emitted_at: String,
}

#[derive(Deserialize)]
struct WireSceneOut {
    content: String,
    #[serde(rename = "tokenCount")]
    token_count: i64,
    #[serde(rename = "emittedByCharacter")]
    emitted_by_character: Vec<WireEmission>,
}

#[derive(Deserialize)]
struct WireHeadOptions {
    #[serde(rename = "maxTokens")]
    max_tokens: Option<i64>,
    #[serde(rename = "maxEntries")]
    max_entries: Option<usize>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "tag")]
    Tag {
        id: String,
        opts: WireTagOpts,
        out: String,
    },
    #[serde(rename = "scene")]
    Scene {
        id: String,
        scene: Option<WireScene>,
        time: Option<String>,
        #[serde(rename = "nowIso")]
        now_iso: String,
        #[serde(rename = "liveClothing")]
        live_clothing: Vec<(String, String)>,
        #[serde(rename = "priorEmission")]
        prior_emission: Vec<WirePriorEmission>,
        out: WireSceneOut,
    },
    #[serde(rename = "memories")]
    Memories {
        id: String,
        memories: Vec<WireResult>,
        #[serde(rename = "maxTokens")]
        max_tokens: i64,
        #[serde(rename = "nowMs")]
        now_ms: f64,
        out: WireMemOut,
    },
    #[serde(rename = "inter")]
    Inter {
        id: String,
        #[serde(rename = "importanceMemories")]
        importance_memories: Vec<WireMem>,
        names: Vec<(String, String)>,
        #[serde(rename = "maxTokens")]
        max_tokens: i64,
        #[serde(rename = "relevanceResults")]
        relevance_results: Vec<WireResult>,
        #[serde(rename = "nowMs")]
        now_ms: f64,
        out: WireInterOut,
    },
    #[serde(rename = "frozen")]
    Frozen {
        id: String,
        memories: Vec<WireMem>,
        #[serde(rename = "maxTokens")]
        max_tokens: i64,
        out: WireMemOut,
    },
    #[serde(rename = "head")]
    Head {
        id: String,
        memories: Vec<WireResult>,
        options: WireHeadOptions,
        #[serde(rename = "nowMs")]
        now_ms: f64,
        out: WireMemOut,
    },
    #[serde(rename = "summary")]
    Summary {
        id: String,
        summary: String,
        #[serde(rename = "maxTokens")]
        max_tokens: i64,
        out: WireSummaryOut,
    },
}

#[derive(Deserialize)]
struct WireSummaryOut {
    content: String,
    #[serde(rename = "tokenCount")]
    token_count: i64,
}

// ---------------------------------------------------------------------------
// Comparison helpers
// ---------------------------------------------------------------------------

/// Tier-1 float tolerance (per CLAUDE.md: "1e-12 for floats, exact for
/// strings"). The debug `effectiveWeight`/`rawWeight` carry the FULL-precision
/// decayed weight, where JS `Math.pow` and Rust `f64::powf` can differ by 1 ULP
/// (a libm implementation difference, not a port bug — the memory-weighting
/// module is itself oracle-matched). The user-visible strings/token-counts round
/// through `toFixed(2)` and are compared exactly.
fn f_eq(a: f64, b: f64, ctx: &str) {
    assert!(
        (a - b).abs() <= 1e-12,
        "{ctx}: {a} vs {b} (diff {})",
        (a - b).abs()
    );
}

fn opt_f_eq(a: Option<f64>, b: Option<f64>, ctx: &str) {
    match (a, b) {
        (Some(x), Some(y)) => f_eq(x, y, ctx),
        (None, None) => {}
        _ => panic!("{ctx}: presence mismatch {a:?} vs {b:?}"),
    }
}

fn assert_debug_mem(got: &[DebugMemoryInfo], want: &[WireDebugMem], id: &str) {
    assert_eq!(got.len(), want.len(), "debugMemories len for '{id}'");
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(g.memory_id, w.memory_id, "debug[{i}].memoryId '{id}'");
        assert_eq!(g.summary, w.summary, "debug[{i}].summary '{id}'");
        f_eq(
            g.importance,
            w.importance,
            &format!("debug[{i}].importance '{id}'"),
        );
        f_eq(g.score, w.score, &format!("debug[{i}].score '{id}'"));
        f_eq(
            g.effective_weight,
            w.effective_weight,
            &format!("debug[{i}].effectiveWeight '{id}'"),
        );
        opt_f_eq(
            g.raw_weight,
            w.raw_weight,
            &format!("debug[{i}].rawWeight '{id}'"),
        );
        opt_f_eq(
            g.recall_multiplier,
            w.recall_multiplier,
            &format!("debug[{i}].recallMultiplier '{id}'"),
        );
        assert_eq!(
            g.recall_fired, w.recall_fired,
            "debug[{i}].recallFired '{id}'"
        );
        opt_f_eq(
            g.blended_before,
            w.blended_before,
            &format!("debug[{i}].blendedBefore '{id}'"),
        );
        opt_f_eq(
            g.blended_after,
            w.blended_after,
            &format!("debug[{i}].blendedAfter '{id}'"),
        );
    }
}

#[test]
fn memory_injector_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MEMORY_INJECTOR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MEMORY_INJECTOR to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("parse error on line: {e}\n{line}"));
        match row {
            Row::Tag { id, opts, out } => {
                let got = format_memory_metadata_tag(&MetadataTagOpts {
                    importance: opts.importance,
                    relevance: opts.relevance,
                    weight: opts.weight,
                    keywords: opts.keywords.as_deref(),
                    adjustments: opts.adjustments.as_deref(),
                });
                assert_eq!(got, out, "tag '{id}'");
            }
            Row::Scene {
                id,
                scene,
                time,
                now_iso,
                live_clothing,
                prior_emission,
                out,
            } => {
                let scene = scene.map(|s| SceneState {
                    location: s.location,
                    characters: s
                        .characters
                        .into_iter()
                        .map(|c| SceneStateCharacter {
                            character_id: c.character_id,
                            character_name: c.character_name,
                            action: c.action,
                            clothing: c.clothing,
                        })
                        .collect(),
                });
                let live: std::collections::HashMap<String, String> =
                    live_clothing.into_iter().collect();
                let prior: std::collections::HashMap<String, SceneStateEmissionEntry> =
                    prior_emission
                        .into_iter()
                        .map(|WirePriorEmission(k, e)| {
                            (
                                k,
                                SceneStateEmissionEntry {
                                    action_hash: e.action_hash,
                                    clothing_hash: e.clothing_hash,
                                    emitted_at: e.emitted_at,
                                },
                            )
                        })
                        .collect();

                let got = format_current_scene_state(
                    scene.as_ref(),
                    time.as_deref(),
                    &now_iso,
                    &|cid| live.get(cid).cloned(),
                    &|cid| prior.get(cid).cloned(),
                );
                assert_eq!(got.content, out.content, "scene '{id}' content");
                assert_eq!(got.token_count, out.token_count, "scene '{id}' tokenCount");
                assert_eq!(
                    got.emitted_by_character.len(),
                    out.emitted_by_character.len(),
                    "scene '{id}' emitted len"
                );
                for (g, w) in got
                    .emitted_by_character
                    .iter()
                    .zip(out.emitted_by_character.iter())
                {
                    assert_eq!(g.0, w.character_id, "scene '{id}' emitted id");
                    assert_eq!(g.1.action_hash, w.action_hash, "scene '{id}' actionHash");
                    assert_eq!(
                        g.1.clothing_hash, w.clothing_hash,
                        "scene '{id}' clothingHash"
                    );
                    assert_eq!(g.1.emitted_at, w.emitted_at, "scene '{id}' emittedAt");
                }
            }
            Row::Memories {
                id,
                memories,
                max_tokens,
                now_ms,
                out,
            } => {
                let results: Vec<InjectorResult> =
                    memories.into_iter().map(|r| r.into_result()).collect();
                let got = format_memories_for_context(&results, max_tokens, now_ms);
                assert_eq!(got.content, out.content, "memories '{id}' content");
                assert_eq!(
                    got.token_count, out.token_count,
                    "memories '{id}' tokenCount"
                );
                assert_eq!(
                    got.memories_used, out.memories_used,
                    "memories '{id}' memoriesUsed"
                );
                assert_debug_mem(&got.debug_memories, &out.debug_memories, &id);
            }
            Row::Inter {
                id,
                importance_memories,
                names,
                max_tokens,
                relevance_results,
                now_ms,
                out,
            } => {
                let imp: Vec<InjectorMemory> = importance_memories
                    .into_iter()
                    .map(|m| m.into_mem())
                    .collect();
                let rel: Vec<InjectorResult> = relevance_results
                    .into_iter()
                    .map(|r| r.into_result())
                    .collect();
                let name_map: std::collections::HashMap<String, String> =
                    names.into_iter().collect();
                let got = format_inter_character_memories_for_context(
                    &imp,
                    &|cid| name_map.get(cid).cloned(),
                    max_tokens,
                    &rel,
                    now_ms,
                );
                assert_eq!(got.content, out.content, "inter '{id}' content");
                assert_eq!(got.token_count, out.token_count, "inter '{id}' tokenCount");
                assert_eq!(
                    got.memories_used, out.memories_used,
                    "inter '{id}' memoriesUsed"
                );
                assert_eq!(
                    got.debug_memories.len(),
                    out.debug_memories.len(),
                    "inter '{id}' debug len"
                );
                for (i, (g, w)) in got
                    .debug_memories
                    .iter()
                    .zip(out.debug_memories.iter())
                    .enumerate()
                {
                    let want = DebugInterCharacterMemoryInfo {
                        about_character_name: w.about_character_name.clone(),
                        summary: w.summary.clone(),
                        importance: w.importance,
                    };
                    assert_eq!(g, &want, "inter '{id}' debug[{i}]");
                }
            }
            Row::Frozen {
                id,
                memories,
                max_tokens,
                out,
            } => {
                let mems: Vec<InjectorMemory> =
                    memories.into_iter().map(|m| m.into_mem()).collect();
                let got = format_frozen_memory_archive(&mems, max_tokens);
                assert_eq!(got.content, out.content, "frozen '{id}' content");
                assert_eq!(got.token_count, out.token_count, "frozen '{id}' tokenCount");
                assert_eq!(
                    got.memories_used, out.memories_used,
                    "frozen '{id}' memoriesUsed"
                );
                assert_debug_mem(&got.debug_memories, &out.debug_memories, &id);
            }
            Row::Head {
                id,
                memories,
                options,
                now_ms,
                out,
            } => {
                let results: Vec<InjectorResult> =
                    memories.into_iter().map(|r| r.into_result()).collect();
                let got = format_dynamic_memory_head(
                    &results,
                    options.max_tokens,
                    options.max_entries,
                    now_ms,
                );
                assert_eq!(got.content, out.content, "head '{id}' content");
                assert_eq!(got.token_count, out.token_count, "head '{id}' tokenCount");
                assert_eq!(
                    got.memories_used, out.memories_used,
                    "head '{id}' memoriesUsed"
                );
                assert_debug_mem(&got.debug_memories, &out.debug_memories, &id);
            }
            Row::Summary {
                id,
                summary,
                max_tokens,
                out,
            } => {
                let (content, token_count) = format_summary_for_context(&summary, max_tokens);
                assert_eq!(content, out.content, "summary '{id}' content");
                assert_eq!(token_count, out.token_count, "summary '{id}' tokenCount");
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: memory-injector matched oracle ({count} rows).");
}
