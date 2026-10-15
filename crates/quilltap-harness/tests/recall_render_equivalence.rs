//! Tier-1 differential (P4.D291): the recall renderers — v4
//! `lib/memory/recall/render.ts` (`584d28164`, pinned at `83334b9a5`) against
//! v5's `quilltap_core::services::recall::render`.
//!
//! THE MODEL-VISIBLE TEXT, byte for byte: every row carries its input and the
//! whole rendered section — `content`, `tokenCount`, `memoriesUsed`, the
//! `lines` (`memoryId`, `line`, `body`) and the `debugMemories` (the `salience`
//! shape, `undefined` keys dropped) — compared as JSON BYTES
//! (`serde_json::to_string`; key order is a comparand). Covered: `humanKeywords`
//! / `isTargetingKeyword` / `formatKeywordsTag` (incl. the genuine-keyword drop,
//! R-L7, and `scope:  Narrow`), the subject prefix, `renderArchive` across
//! budgets on both token rates (R-T: GOOGLE 3.8 vs the default 3.5 — the
//! budget-edge rows differ between the two), `renderHead` (the blend arm, the
//! salience arm with the 0.05 near-tie and a NaN salience, the mixed arm, the
//! entry cap, a zero cap), `renderRelevant` (no provenance, the content-first
//! body) and `renderInterCharacter` (the interleave, digests first, the
//! per-character break, `Unknown` for an empty name).
//!
//! Generate the oracle (from the v4 checkout, or a worktree pinned at the
//! round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/recall-render.ts \
//!     > /tmp/oracle-recall-render.ndjson
//! Run:
//!   QT_ORACLE_RECALL_RENDER=/tmp/oracle-recall-render.ndjson \
//!     cargo test -p quilltap-harness --test recall_render_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::services::recall::render::{
    format_keywords_tag, format_memory_subject_prefix, human_keywords, is_targeting_keyword,
    render_archive, render_head, render_inter_character, render_relevant,
    DebugInterCharacterMemoryInfo, DebugMemoryInfo, MemorySubjectContext, RenderHeadOptions,
    RenderedSection,
};
use quilltap_core::services::recall::types::{RecallAdjustment, RecallCandidate};
use serde_json::{json, Value};

fn f(v: &Value) -> f64 {
    match v {
        Value::String(s) if s == "NaN" => f64::NAN,
        other => other.as_f64().unwrap(),
    }
}

fn subject(v: &Value) -> MemorySubjectContext {
    MemorySubjectContext {
        self_character_id: v["selfCharacterId"].as_str().unwrap().to_string(),
        character_names: v["names"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, n)| (k.clone(), n.as_str().unwrap().to_string()))
            .collect(),
    }
}

fn candidates(v: &Value) -> Vec<RecallCandidate> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|c| RecallCandidate {
            memory: c["memory"].clone(),
            score: f(&c["score"]),
            used_embedding: c["usedEmbedding"].as_bool().unwrap(),
            salience: f(&c["salience"]),
            recall_adjustment: c.get("recallAdjustment").map(|a| RecallAdjustment {
                multiplier: f(&a["multiplier"]),
                fired: a["fired"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap().to_string())
                    .collect(),
                blended_before: f(&a["blendedBefore"]),
                blended_after: f(&a["blendedAfter"]),
            }),
        })
        .collect()
}

fn provider(v: &Value) -> Option<&str> {
    v.as_str()
}

fn section_json<D>(s: &RenderedSection<D>, debug: impl Fn(&D) -> Value) -> Value {
    json!({
        "content": s.content,
        "tokenCount": s.token_count,
        "memoriesUsed": s.memories_used,
        "lines": s.lines.iter().map(|l| json!({"memoryId": l.memory_id, "line": l.line, "body": l.body})).collect::<Vec<_>>(),
        "debugMemories": s.debug_memories.iter().map(debug).collect::<Vec<_>>(),
    })
}

fn bytes(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}

#[test]
fn recall_render_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_RECALL_RENDER") else {
        eprintln!("SKIP: QT_ORACLE_RECALL_RENDER not set");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for (n, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let row: Value = serde_json::from_str(line).expect("ndjson row");
        let kind = row["kind"].as_str().unwrap().to_string();
        let input = &row["input"];
        match kind.as_str() {
            "keywords" => {
                let got = human_keywords(Some(input));
                assert_eq!(
                    bytes(&json!(got)),
                    bytes(&row["human"]),
                    "row {n}: humanKeywords"
                );
                assert_eq!(
                    format_keywords_tag(Some(input)),
                    row["tag"].as_str().unwrap(),
                    "row {n}: formatKeywordsTag"
                );
                for pair in row["targeting"].as_array().unwrap() {
                    let kw = pair[0].as_str().unwrap();
                    assert_eq!(
                        is_targeting_keyword(kw),
                        pair[1].as_bool().unwrap(),
                        "row {n}: isTargetingKeyword({kw:?})"
                    );
                }
            }
            "keywordsNull" => {
                assert!(human_keywords(None).is_empty());
                assert_eq!(format_keywords_tag(None), "");
            }
            "prefix" => {
                let named = MemorySubjectContext {
                    self_character_id: "00000000-0000-4000-8000-0000000000c1".to_string(),
                    character_names: HashMap::from([
                        (
                            "00000000-0000-4000-8000-00000000a001".to_string(),
                            "Amy".to_string(),
                        ),
                        (
                            "00000000-0000-4000-8000-00000000b002".to_string(),
                            "  Bo  ".to_string(),
                        ),
                        (
                            "00000000-0000-4000-8000-00000000e00e".to_string(),
                            "   ".to_string(),
                        ),
                    ]),
                };
                assert_eq!(
                    format_memory_subject_prefix(row["about"].as_str(), &named),
                    row["prefix"].as_str().unwrap(),
                    "row {n}: prefix"
                );
            }
            "archive" => {
                let got = render_archive(
                    input["memories"].as_array().unwrap(),
                    input["maxTokens"].as_i64().unwrap(),
                    provider(&input["provider"]),
                    &subject(&input["subject"]),
                );
                assert_eq!(
                    bytes(&section_json(&got, DebugMemoryInfo::to_json)),
                    bytes(&row["output"]),
                    "row {n}: renderArchive"
                );
            }
            "head" => {
                let got = render_head(
                    &candidates(&input["candidates"]),
                    provider(&input["provider"]),
                    &subject(&input["subject"]),
                    RenderHeadOptions {
                        max_tokens: input["maxTokens"].as_i64().unwrap(),
                        max_entries: input["maxEntries"].as_u64().unwrap() as usize,
                        now_ms: f(&input["nowMs"]),
                    },
                );
                assert_eq!(
                    bytes(&section_json(&got, DebugMemoryInfo::to_json)),
                    bytes(&row["output"]),
                    "row {n}: renderHead"
                );
            }
            "relevant" => {
                let got = render_relevant(
                    &candidates(&input["candidates"]),
                    input["maxTokens"].as_i64().unwrap(),
                    provider(&input["provider"]),
                    &subject(&input["subject"]),
                    f(&input["nowMs"]),
                );
                assert_eq!(
                    bytes(&section_json(&got, DebugMemoryInfo::to_json)),
                    bytes(&row["output"]),
                    "row {n}: renderRelevant"
                );
            }
            "inter" => {
                let names: HashMap<String, String> = input["names"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                    .collect();
                let got = render_inter_character(
                    input["importance"].as_array().unwrap(),
                    &names,
                    input["maxTokens"].as_i64().unwrap(),
                    provider(&input["provider"]),
                    &candidates(&input["relevance"]),
                    f(&input["nowMs"]),
                );
                assert_eq!(
                    bytes(&section_json(&got, DebugInterCharacterMemoryInfo::to_json)),
                    bytes(&row["output"]),
                    "row {n} ({}): renderInterCharacter",
                    row["id"]
                );
            }
            "meta" => continue,
            other => panic!("unknown kind {other}"),
        }
        *counts.entry(kind).or_default() += 1;
    }
    eprintln!("recall_render rows: {counts:?}");
    assert_eq!(counts.get("archive"), Some(&72));
    assert_eq!(counts.get("head"), Some(&30));
    assert_eq!(counts.get("relevant"), Some(&10));
    assert_eq!(counts.get("inter"), Some(&48));
}
