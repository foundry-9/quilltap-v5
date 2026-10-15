//! Tier-3 differential (P4.D291): recall's first stage — v4's REAL
//! `buildRecallQuery` (`lib/memory/recall/query.ts`, `584d28164`) and, through
//! it, the REAL distiller with bug 212's memory calendar (`c2795733f`), pinned
//! at `83334b9a5`, against v5's `services::recall::query::build_recall_query`
//! over the distiller at `services::memory_recap::distill`.
//!
//! Each oracle row carries its case SPEC (the canned embedding table, the
//! canned distil answer, the memory zone, the calls). This side builds the same
//! canned providers — a recording embedding provider answering from the table
//! (a `fail` entry fails with its own message), a completion provider that
//! records the prompt and answers the canned text (or fails) — and compares,
//! per call, by BYTES where v4 emits JSON:
//! - the query: `text`, the embedding (vector + provider, or null), the probes
//!   (`text`, embedded?), `anchors`, `window`, `retrospective`,
//!   `embeddingError`, and the signals as v4's `MemorySearchExtraction` JSON;
//! - the stage trace (`serde_json::to_string` of the entry list — key order is a
//!   comparand);
//! - per case: every embedding call (text + profile) in order, the distil
//!   prompt v4 handed the executor (null when no distil ran), and the `[Recall]`
//!   log lines (the query-embedding WARN, the probe DEBUG).
//!
//! Generate the oracle (from the v4 checkout, or a worktree pinned at the
//! round's pin; stage the case outside `.claude/`, which v4's jest ignores):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-recall-query-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/recall-query-tier3.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-recall-query.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- recall-query-tier3
//! Run:
//!   QT_ORACLE_RECALL_QUERY=/tmp/oracle-recall-query.ndjson \
//!     cargo test -p quilltap-harness --test recall_query_tier3_equivalence -- --nocapture

use std::sync::Mutex;

use quilltap_core::cheap_llm::CheapLlmSelection;
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse, CompletionUsage,
};
use quilltap_core::model::embedding::{
    EmbeddingCallOptions, EmbeddingError, EmbeddingProvider, EmbeddingResult,
};
use quilltap_core::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskOptions};
use quilltap_core::services::memory_recap::distill::{
    DistillMessage, DistilledSearch, ExtractionClock,
};
use quilltap_core::services::recall::query::{
    build_recall_query, BuildRecallQueryInput, EmbeddingMemo, EmbeddingMemoMap, RecallDistill,
    RecallQuery,
};
use quilltap_core::services::recall::types::RecallTrace;
use serde_json::{json, Map, Value};

/// The canned embedding table, recording every call.
struct TableEmbedding {
    table: Map<String, Value>,
    calls: Mutex<Vec<(String, Option<String>)>>,
}

impl EmbeddingProvider for TableEmbedding {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        _user_id: &str,
        profile_id: Option<&str>,
        _opts: EmbeddingCallOptions,
    ) -> impl std::future::Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        self.calls
            .lock()
            .unwrap()
            .push((text.to_string(), profile_id.map(str::to_string)));
        let result = match self.table.get(text) {
            None => Err(EmbeddingError::new("no canned embedding")),
            Some(spec) => match spec.get("fail").and_then(Value::as_str) {
                Some(message) => Err(EmbeddingError::new(message)),
                None => {
                    let embedding: Vec<f32> = spec["vector"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap() as f32)
                        .collect();
                    Ok(EmbeddingResult {
                        dimensions: embedding.len(),
                        embedding,
                        model: "m".to_string(),
                        provider: spec
                            .get("provider")
                            .and_then(Value::as_str)
                            .unwrap_or("OPENAI")
                            .to_string(),
                    })
                }
            },
        };
        async move { result }
    }
}

/// Answers the canned distil text (or fails) and records the prompt.
struct CannedDistil {
    response: Option<String>,
    prompt: Mutex<Option<Vec<(String, String)>>>,
}

impl CompletionProvider for CannedDistil {
    fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        params: &CompletionParams,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        *self.prompt.lock().unwrap() = Some(
            params
                .messages
                .iter()
                .map(|m| (m.role.as_str().to_string(), m.content.clone()))
                .collect(),
        );
        let response = self.response.clone();
        async move {
            match response {
                Some(content) => Ok(CompletionResponse {
                    content,
                    usage: Some(CompletionUsage {
                        prompt_tokens: 100,
                        completion_tokens: 20,
                        total_tokens: 120,
                    }),
                    finish_reason: Some("stop".to_string()),
                    attachment_results: None,
                    cache_usage: None,
                }),
                None => Err(CompletionError::new("canned failure")),
            }
        }
    }
}

/// v4's `MemorySearchExtraction` JSON handed in as `signals` → v5's
/// `DistilledSearch` (the given-signals arm; fields absent in the JSON stay
/// absent).
fn signals_from(v: &Value) -> Option<DistilledSearch> {
    let obj = v.as_object()?;
    let strings = |k: &str| -> Vec<String> {
        obj.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(DistilledSearch {
        keywords: strings("keywords"),
        temporal: obj
            .get("temporal")
            .and_then(Value::as_str)
            .and_then(quilltap_core::recall_tags::TemporalTag::from_kw),
        context: obj
            .get("context")
            .and_then(Value::as_str)
            .and_then(quilltap_core::recall_tags::ContextTag::from_kw),
        paraphrase: obj
            .get("paraphrase")
            .and_then(Value::as_str)
            .map(str::to_string),
        retrospective: obj.get("retrospective").and_then(Value::as_bool) == Some(true),
        time_range: obj.get("timeRange").filter(|t| t.is_object()).map(|t| {
            quilltap_core::recall_tags::TimeWindow {
                from: t["from"].as_str().unwrap().to_string(),
                to: t["to"].as_str().unwrap().to_string(),
            }
        }),
        entities: strings("entities"),
        parse_failed: false,
    })
}

fn query_json(q: &RecallQuery) -> Value {
    json!({
        "text": q.text,
        "embedding": q.embedding.as_ref().map(|e| json!({
            // `JSON.stringify` of a Float32Array's numbers: an integer-valued
            // component renders bare (`0`, not `0.0`).
            "vector": e
                .embedding
                .iter()
                .map(|f| {
                    let f = *f as f64;
                    if f.is_finite() && f.fract() == 0.0 {
                        Value::from(f as i64)
                    } else {
                        Value::from(f)
                    }
                })
                .collect::<Vec<_>>(),
            "provider": e.provider,
        })),
        "probes": q.probes.iter().map(|p| json!({"text": p.text, "embedded": p.embedding.is_some()})).collect::<Vec<_>>(),
        "signals": q.signals.as_ref().map(DistilledSearch::signals_json),
        "anchors": q.anchors,
        "window": q.window.as_ref().map(|w| json!({"from": w.from, "to": w.to})),
        "retrospective": q.retrospective,
        "embeddingError": q.embedding_error,
    })
}

/// A v4 log row → the rig's rendered line.
fn v4_log_line(l: &Value) -> String {
    let level = l["level"].as_str().unwrap().to_uppercase();
    let mut line = format!(
        "{level} quilltap::recall {}",
        l["message"].as_str().unwrap()
    );
    for (k, v) in l["fields"].as_object().unwrap() {
        let rendered = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        line.push_str(&format!(" {k}={rendered}"));
    }
    line
}

#[test]
fn recall_query_tier3_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_RECALL_QUERY") else {
        eprintln!("SKIP: QT_ORACLE_RECALL_QUERY not set");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let mut cases = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("ndjson row");
        let name = row["name"].as_str().unwrap().to_string();
        let spec = &row["spec"];
        let zone = spec["zone"].as_str().unwrap().to_string();
        let embedding = TableEmbedding {
            table: spec["embeddings"].as_object().cloned().unwrap_or_default(),
            calls: Mutex::new(Vec::new()),
        };
        let completion = CannedDistil {
            response: match spec.get("distill") {
                Some(d) if d["kind"] == "json" => {
                    Some(d["text"].as_str().unwrap_or("").to_string())
                }
                _ => None,
            },
            prompt: Mutex::new(None),
        };
        let executor = CheapLlmTaskExecutor::new();
        let selection = CheapLlmSelection {
            provider: "OPENAI".to_string(),
            model_name: "cheap-model".to_string(),
            base_url: None,
            connection_profile_id: None,
            is_local: false,
            profile_parameters: None,
        };
        let memo: Option<EmbeddingMemo> = (spec["sharedMemo"] == true)
            .then(|| std::sync::Arc::new(Mutex::new(EmbeddingMemoMap::default())));

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (calls, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(async {
                let mut out = Vec::new();
                for call in spec["calls"].as_array().unwrap() {
                    let distill = call.get("distill").map(|d| RecallDistill {
                        completion: &completion,
                        executor: &executor,
                        messages: d["messages"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|m| DistillMessage {
                                role: m["role"].as_str().unwrap().to_string(),
                                content: m["content"].as_str().unwrap().to_string(),
                            })
                            .collect(),
                        character_name: d["characterName"].as_str().unwrap(),
                        selection: &selection,
                        character_id: d.get("characterId").and_then(Value::as_str).unwrap_or(""),
                        clock: d.get("clock").map(|c| ExtractionClock {
                            now_iso: c["nowIso"].as_str().unwrap().to_string(),
                            timeline_mode: c["timelineMode"].as_str().unwrap().to_string(),
                            time_zone: Some(zone.clone()),
                        }),
                        latency: if d["latency"] == "interactive" {
                            CheapLlmTaskOptions::interactive()
                        } else {
                            CheapLlmTaskOptions::default()
                        },
                    });
                    let mut trace: RecallTrace = Vec::new();
                    let q = build_recall_query(
                        &embedding,
                        BuildRecallQueryInput {
                            user_id: "u1",
                            embedding_profile_id: call
                                .get("embeddingProfileId")
                                .and_then(Value::as_str),
                            text: call["text"].as_str().unwrap(),
                            signals: call.get("signals").and_then(signals_from),
                            distill,
                            embedding_memo: memo.clone(),
                            skip_embedding: call["skipEmbedding"] == true,
                            probes: call.get("probes").map(|p| {
                                p.as_array()
                                    .unwrap()
                                    .iter()
                                    .map(|s| s.as_str().unwrap().to_string())
                                    .collect()
                            }),
                        },
                        &mut trace,
                    )
                    .await;
                    out.push(json!({ "query": query_json(&q), "trace": trace }));
                }
                out
            })
        });

        let want_calls = row["calls"].as_array().unwrap();
        assert_eq!(calls.len(), want_calls.len(), "{name}: call count");
        for (i, (got, want)) in calls.iter().zip(want_calls).enumerate() {
            assert_eq!(
                serde_json::to_string(&got["query"]).unwrap(),
                serde_json::to_string(&want["query"]).unwrap(),
                "{name}[{i}]: the query"
            );
            assert_eq!(
                serde_json::to_string(&got["trace"]).unwrap(),
                serde_json::to_string(&want["trace"]).unwrap(),
                "{name}[{i}]: the trace"
            );
        }
        let got_calls: Vec<Value> = embedding
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|(t, p)| json!({"text": t, "profile": p}))
            .collect();
        assert_eq!(
            serde_json::to_string(&got_calls).unwrap(),
            serde_json::to_string(&row["embedCalls"]).unwrap(),
            "{name}: the embedding calls"
        );
        let got_prompt = completion.prompt.lock().unwrap().clone().map(|p| {
            p.into_iter()
                .map(|(role, content)| json!({"role": role, "content": content}))
                .collect::<Vec<_>>()
        });
        assert_eq!(
            serde_json::to_string(&got_prompt).unwrap(),
            serde_json::to_string(&row["distillPrompt"]).unwrap(),
            "{name}: the distil prompt"
        );
        let got_logs: Vec<String> = lines
            .into_iter()
            .filter(|l| l.contains("[Recall]"))
            .collect();
        let want_logs: Vec<String> = row["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(v4_log_line)
            .collect();
        assert_eq!(got_logs, want_logs, "{name}: the [Recall] lines");
        cases += 1;
    }
    eprintln!("recall_query_tier3 cases: {cases}");
    assert_eq!(cases, 15);
}
