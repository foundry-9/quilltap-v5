//! P4.D291 tier-3 differential: recall's assemble + deliver stages —
//! `services::recall::assemble::assemble_recall` and
//! `services::recall::deliver::deliver_recall` against v4's REAL
//! `assembleRecall` / `deliverRecall` (`lib/memory/recall/`, `584d28164`,
//! pinned at `83334b9a5`), over a fresh copy of the committed
//! `recall-assemble-main.db` per case (R-L6): the frozen archive, the index
//! store, the repositories, the recap's banded tier reads and the chat's ring
//! write are REAL on both sides.
//!
//! Only the model boundaries are canned, identically: every text embeds as
//! `[1, 0]` (`OPENAI`) through a RECORDING provider (the "one embedding per
//! turn" comparand is the ordered list of embedded texts), and the recap's
//! narrative replays the oracle's recorded completion keys.
//!
//! Per case, compared field for field: each assemble's bundle (path, the query
//! summary, the pool / relevance-pool ids, sections, tokens, every rendered
//! line, pools, the retro signature, the debug entries, the counts, the stage
//! trace, the warnings), each delivery (delivered by pool, lost, the debug
//! record), the embedded texts, the recorded completions, the `[Recall]` /
//! `[Memory]` lines (message + fields), the ids stamped accessed, and the
//! chat's ring afterwards.
//!
//! Regenerate + run (Node 24, from the v4 pin; STAGE outside any `.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   STAGE=/tmp/qt-recall-assemble-stage
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $V5W/harness/oracle/cases/recall-assemble-tier3.test.ts $STAGE/harness/oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/recall-assemble.json $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_RA_MAIN=$V5W/crates/quilltap-web/tests/fixtures/recall-assemble-main.db \
//!   QT_ORACLE_OUT=/tmp/oracle-recall-assemble.ndjson PATH=$N:$PATH \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" \
//!     -- "recall-assemble-tier3\.test\.ts$"
//!   cd $V5W
//!   QT_ORACLE_RECALL_ASSEMBLE=/tmp/oracle-recall-assemble.ndjson \
//!     cargo test -p quilltap-harness --test recall_assemble_tier3_equivalence -- --nocapture

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use quilltap_core::cheap_llm::CheapLlmSelection;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::{
    EmbeddingCallOptions, EmbeddingError, EmbeddingProvider, EmbeddingResult,
};
use quilltap_core::recall_tags::ScopePolicy;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::frozen_archive::reset_frozen_archive_cache_for_tests;
use quilltap_core::services::memory_recap::distill::{DistillTimeRange, DistilledSearch};
use quilltap_core::services::recall::assemble::{
    assemble_recall, empty_recall_bundle, PlanConversations, PlanRecallSettings, PlanRecap,
    PreRecall, RecallBundle, RecallChat, RecallPlan,
};
use quilltap_core::services::recall::context::{
    build_turn_recall_context, TurnRecallContextInput, TurnRecallSettings,
};
use quilltap_core::services::recall::deliver::{deliver_recall, DeliverOptions, RecallDelivery};
use quilltap_core::services::recall::query::{build_recall_query, BuildRecallQueryInput};
use quilltap_core::services::recall::search::{search_recall, RecallSearchOptions};
use serde_json::{json, Map, Value};

/// Every text embeds as `[1, 0]` (`OPENAI`); the texts are recorded in order.
#[derive(Default)]
struct RecordingEmbedding(Mutex<Vec<String>>);

impl EmbeddingProvider for RecordingEmbedding {
    async fn generate_embedding_for_user(
        &self,
        text: &str,
        _user_id: &str,
        _profile_id: Option<&str>,
        _opts: EmbeddingCallOptions,
    ) -> Result<EmbeddingResult, EmbeddingError> {
        self.0.lock().unwrap().push(text.to_string());
        Ok(EmbeddingResult {
            embedding: vec![1.0, 0.0],
            model: "canned".to_string(),
            dimensions: 2,
            provider: "OPENAI".to_string(),
        })
    }
}

fn spec() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/recall-assemble.json"),
        )
        .expect("read the spec"),
    )
    .expect("the spec parses")
}

fn fixture_main() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-web/tests/fixtures/recall-assemble-main.db")
}

fn selection() -> CheapLlmSelection {
    CheapLlmSelection {
        provider: "OPENAI".to_string(),
        model_name: "gpt-canned".to_string(),
        base_url: None,
        connection_profile_id: Some("p1".to_string()),
        is_local: false,
        profile_parameters: None,
    }
}

fn canned_completions(rows: &[Value]) -> CannedCompletionProvider {
    let mut c = CannedCompletionProvider::new();
    for row in rows {
        let messages: Vec<CompletionMessage> = row["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| CompletionMessage {
                role: CompletionRole::from_v4_wire(m["role"].as_str().unwrap()).unwrap(),
                content: m["content"].as_str().unwrap().to_string(),
            })
            .collect();
        c = c.with_response(
            row["provider"].as_str().unwrap(),
            row["model"].as_str().unwrap(),
            row["temperature"].as_f64(),
            &messages,
            row["response"].as_str().unwrap().to_string(),
            Some(CompletionUsage {
                prompt_tokens: row["usage"]["promptTokens"].as_i64().unwrap(),
                completion_tokens: row["usage"]["completionTokens"].as_i64().unwrap(),
                total_tokens: row["usage"]["totalTokens"].as_i64().unwrap(),
            }),
        );
    }
    c
}

fn ids<'a>(v: impl IntoIterator<Item = &'a String>) -> Value {
    Value::Array(v.into_iter().cloned().map(Value::String).collect())
}

/// The bundle in the oracle's `dumpBundle` shape (`undefined` keys dropped).
fn dump_bundle(b: &RecallBundle) -> Value {
    // JS `JSON.stringify` of a number: an integer-valued double renders bare.
    let num = |f: f64| -> Value {
        if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 {
            Value::from(f as i64)
        } else {
            Value::from(f)
        }
    };
    let opt = |m: &mut Map<String, Value>, k: &str, v: &Option<String>| {
        if let Some(s) = v {
            m.insert(k.into(), Value::String(s.clone()));
        }
    };
    let mut sections = Map::new();
    opt(&mut sections, "recap", &b.sections.recap);
    opt(&mut sections, "relevant", &b.sections.relevant);
    opt(&mut sections, "interCharacter", &b.sections.inter_character);
    opt(
        &mut sections,
        "relevantConversations",
        &b.sections.relevant_conversations,
    );
    opt(&mut sections, "retrospective", &b.sections.retrospective);
    let mut lines = Map::new();
    for (pool, rendered) in &b.lines {
        lines.insert(
            pool.as_str().to_string(),
            Value::Array(
                rendered
                    .iter()
                    .map(|l| json!({ "memoryId": l.memory_id, "line": l.line, "body": l.body }))
                    .collect(),
            ),
        );
    }
    let query = b.query.as_ref().map_or(Value::Null, |q| {
        json!({
            "text": q.text,
            "retrospective": q.retrospective,
            "anchors": q.anchors,
            "window": q.window.as_ref().map_or(Value::Null, |w| json!({ "from": w.from, "to": w.to })),
            "probes": q.probes.iter().map(|p| json!({ "text": p.text, "embedded": p.embedding.is_some() })).collect::<Vec<_>>(),
            "embedded": q.embedding.is_some(),
        })
    });
    json!({
        "path": b.path.as_str(),
        "query": query,
        "pool": b.pool.iter().map(|c| c.id().to_string()).collect::<Vec<_>>(),
        "relevancePool": b.relevance_pool.iter().map(|c| c.id().to_string()).collect::<Vec<_>>(),
        "sections": Value::Object(sections),
        "tokens": { "recap": num(b.tokens.recap as f64), "relevant": num(b.tokens.relevant as f64), "interCharacter": num(b.tokens.inter_character as f64) },
        "lines": Value::Object(lines),
        "pools": b.pools.to_json(),
        "retroSignature": b.retro_signature,
        "debugMemories": b.debug_memories.iter().map(|d| d.to_json()).collect::<Vec<_>>(),
        "debugInterCharacterMemories": b.debug_inter_character_memories.iter().map(|d| d.to_json()).collect::<Vec<_>>(),
        "memoriesIncluded": b.memories_included,
        "interCharacterIncluded": b.inter_character_included,
        "trace": serde_json::to_value(&b.trace).unwrap(),
        "warnings": b.warnings,
    })
}

fn dump_delivery(d: &RecallDelivery) -> Value {
    let mut delivered = Map::new();
    for (pool, got) in &d.delivered {
        delivered.insert(pool.as_str().to_string(), ids(got));
    }
    json!({ "delivered": Value::Object(delivered), "lost": d.lost, "debugLog": d.debug_log })
}

/// The oracle's `everything(bundle)`.
fn everything(b: &RecallBundle) -> String {
    [
        &b.sections.recap,
        &b.sections.relevant,
        &b.sections.inter_character,
    ]
    .into_iter()
    .filter_map(|s| s.as_deref().filter(|s| !s.is_empty()))
    .collect::<Vec<_>>()
    .join("\n\n")
}

#[test]
fn recall_assemble_matches_oracle() {
    let Ok(oracle) = std::env::var("QT_ORACLE_RECALL_ASSEMBLE") else {
        eprintln!("SKIP: set QT_ORACLE_RECALL_ASSEMBLE (see header).");
        return;
    };
    let spec = spec();
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let now_ms = spec["$nowMs"].as_f64().unwrap();
    let user = spec["userId"].as_str().unwrap().to_string();
    let lyra = spec["lyra"].as_str().unwrap().to_string();
    let iris = spec["iris"].as_str().unwrap().to_string();
    let chat_id = spec["chat"]["id"].as_str().unwrap().to_string();
    let about_iris = spec["memories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["summary"] == "iris secret")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sel = selection();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut cases = 0usize;
    for line in std::fs::read_to_string(&oracle)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let want: Value = serde_json::from_str(line).unwrap();
        let name = want["name"].as_str().unwrap().to_string();
        reset_frozen_archive_cache_for_tests();
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.db");
        std::fs::copy(fixture_main(), &main).unwrap();
        let db = Db::open(
            DbPaths {
                main,
                mount_index: Some(dir.path().join("mount.db")),
                llm_logs: None,
            },
            &pepper,
        )
        .unwrap_or_else(|e| panic!("{name}: open: {e}"));
        let embedding = RecordingEmbedding::default();
        let completion = canned_completions(want["canned"].as_array().unwrap());
        let executor = CheapLlmTaskExecutor::new();
        let none_standing = || HashSet::new();
        let history = Value::Null;

        let ((steps, _), logs) = rt.block_on(quilltap_core::test_support::global_capture::capture_async(async {
            let mut bundles: Vec<RecallBundle> = Vec::new();
            let mut steps: Vec<Value> = Vec::new();
            let plan = |recap: bool, per_turn: bool, pre: Option<PreRecall>| RecallPlan {
                user_id: &user,
                embedding_profile_id: None,
                provider: Some("OPENAI"),
                character_id: &lyra,
                character_name: "Lyra",
                character_mount_point_id: None,
                chat: RecallChat {
                    id: &chat_id,
                    project_id: None,
                    compaction_generation: 0,
                    commonplace_recall_history: &history,
                    timeline_mode: None,
                },
                memory_budget: 2000,
                min_importance: 0.3,
                skip_memories: false,
                present: vec![(lyra.clone(), "Lyra".into()), (iris.clone(), "Iris".into())],
                inter_character_subjects: vec![(iris.clone(), "Iris".into())],
                current_state_tokens: 0,
                recall_settings: PlanRecallSettings {
                    scope_policy: ScopePolicy::DownWeight,
                    expand_related: false,
                    per_turn_conversation_summaries: per_turn,
                },
                recent_window: "What did Iris tell you at the lighthouse?".to_string(),
                distill: None,
                pre_recall: pre,
                recap: recap.then(|| PlanRecap {
                    selection: &sel,
                    uncensored_fallback: None,
                    max_context: None,
                    fallback_query: "the scenario".to_string(),
                }),
                conversations: Some(PlanConversations {
                    max_context: Some(32000),
                    standing_ids: &none_standing,
                }),
                on_status_change: None,
                replay: None,
                now_ms,
            };
            // The oracle's literal signals, as the distiller's parsed shape.
            let signals = |paraphrase: &str,
                           keywords: &[&str],
                           entities: &[&str],
                           range: Option<(String, String)>| DistilledSearch {
                keywords: keywords.iter().map(|k| k.to_string()).collect(),
                temporal: None,
                context: None,
                paraphrase: Some(paraphrase.to_string()),
                retrospective: true,
                time_range: range.map(|(from, to)| DistillTimeRange { from, to }),
                entities: entities.iter().map(|e| e.to_string()).collect(),
                parse_failed: false,
            };
            // Each case's steps, as the oracle's CASES table spells them.
            let pre_query = |text: &str, sig: Option<DistilledSearch>| {
                let embedding = &embedding;
                let user = &user;
                let text = text.to_string();
                async move {
                    let mut trace = Vec::new();
                    let q = build_recall_query(
                        embedding,
                        BuildRecallQueryInput {
                            signals: sig,
                            ..BuildRecallQueryInput::plain(user, None, &text)
                        },
                        &mut trace,
                    )
                    .await;
                    (q, trace)
                }
            };
            let day = 86_400_000.0;
            let iso = |ms: f64| quilltap_core::clock::iso_from_unix_ms(ms as i64);
            match name.as_str() {
                "ordinary_turn" | "deliver_compression_cut" | "deliver_ring_head_only"
                | "deliver_nothing" => {
                    bundles.push(assemble_recall(&db, &embedding, &completion, &executor, plan(false, true, None)).await);
                }
                "recap_rows_kept_in_head" => {
                    bundles.push(assemble_recall(&db, &embedding, &completion, &executor, plan(true, true, None)).await);
                }
                "deliver_recap_turn" => {
                    bundles.push(assemble_recall(&db, &embedding, &completion, &executor, plan(true, false, None)).await);
                }
                "retrospective_pre_searched" => {
                    let sig = signals(
                        "They remember the lighthouse.",
                        &["lighthouse"],
                        &["Lighthouse Point"],
                        Some((iso(now_ms - 9.0 * day), iso(now_ms - day))),
                    );
                    let (q, _) = pre_query("w", Some(sig)).await;
                    let ctx = build_turn_recall_context(TurnRecallContextInput {
                        chat_id: &chat_id,
                        chat_project_id: None,
                        commonplace_recall_history: &Value::Null,
                        recall_settings: TurnRecallSettings {
                            scope_policy: ScopePolicy::DownWeight,
                            expand_related: false,
                        },
                        turn_context: None,
                        turn_temporal: None,
                        turn_retrospective: true,
                        present_about_character_ids: Vec::new(),
                        present_participant_names: Vec::new(),
                        now_ms,
                        tuning: None,
                    });
                    let mut trace = Vec::new();
                    let out = search_recall(&db, &lyra, &q, &RecallSearchOptions {
                        limit: Some(30),
                        min_importance: Some(0.3),
                        recall_context: Some(ctx),
                        now_ms,
                        ..Default::default()
                    }, &mut trace, None).await.unwrap();
                    let pre = PreRecall { query: q, candidates: out.candidates, ranked: None, trace };
                    bundles.push(assemble_recall(&db, &embedding, &completion, &executor, plan(false, false, Some(pre))).await);
                }
                "mini_recap_names_no_one" => {
                    let sig = signals("They remember the bridge.", &["bridge"], &[], None);
                    let (q, _) = pre_query("w", Some(sig)).await;
                    let out = search_recall(&db, &lyra, &q, &RecallSearchOptions {
                        limit: Some(30),
                        now_ms,
                        ..Default::default()
                    }, &mut Vec::new(), None).await.unwrap();
                    let pre = PreRecall { query: q, candidates: out.candidates, ranked: None, trace: Vec::new() };
                    bundles.push(assemble_recall(&db, &embedding, &completion, &executor, plan(false, false, Some(pre))).await);
                }
                "fallback_when_pre_found_nothing" => {
                    let (q, _) = pre_query("nothing matched this", None).await;
                    let pre = PreRecall { query: q, candidates: Vec::new(), ranked: None, trace: Vec::new() };
                    bundles.push(assemble_recall(&db, &embedding, &completion, &executor, plan(false, true, Some(pre))).await);
                }
                "relevance_past_the_cut" => {
                    let (q, _) = pre_query("w", None).await;
                    let out = search_recall(&db, &lyra, &q, &RecallSearchOptions {
                        limit: Some(30),
                        min_importance: Some(0.3),
                        now_ms,
                        ..Default::default()
                    }, &mut Vec::new(), None).await.unwrap();
                    let head: Vec<_> = out.candidates.into_iter().filter(|c| c.id() != about_iris).take(3).collect();
                    let pre = PreRecall { query: q, candidates: head, ranked: Some(out.ranked), trace: Vec::new() };
                    bundles.push(assemble_recall(&db, &embedding, &completion, &executor, plan(false, true, Some(pre))).await);
                }
                "archive_still_on_retro_turn" => {
                    bundles.push(assemble_recall(&db, &embedding, &completion, &executor, plan(false, true, None)).await);
                    let sig = signals("Remember the bridge?", &["x"], &[], None);
                    let (q, _) = pre_query("w", Some(sig)).await;
                    let pre = PreRecall { query: q, candidates: Vec::new(), ranked: None, trace: Vec::new() };
                    bundles.push(assemble_recall(&db, &embedding, &completion, &executor, plan(false, true, Some(pre))).await);
                }
                "deliver_trimmed_mini_recap" => {}
                other => panic!("no Rust script for oracle case {other}"),
            }
            for b in &bundles {
                steps.push(json!({ "op": "assemble", "bundle": dump_bundle(b) }));
            }
            // The deliveries.
            let deliver_text = |b: &RecallBundle, how: &str| -> String {
                match how {
                    "cutFirstHead" => {
                        let head = b.lines.iter().find(|(p, _)| p.as_str() == "head").unwrap();
                        everything(b).replacen(&head.1[0].line, "", 1)
                    }
                    "nothing" => String::new(),
                    _ => everything(b),
                }
            };
            let how = match name.as_str() {
                "deliver_compression_cut" => Some("cutFirstHead"),
                "deliver_ring_head_only" | "deliver_recap_turn" => Some("everything"),
                "deliver_nothing" => Some("nothing"),
                _ => None,
            };
            if let Some(how) = how {
                let text = deliver_text(&bundles[0], how);
                let d = deliver_recall(&db, &bundles[0], DeliverOptions {
                    character_id: &lyra,
                    chat_id: &chat_id,
                    commonplace_recall_history: &Value::Null,
                    delivered_text: &text,
                    persist_ring: true,
                }).await;
                steps.push(json!({ "op": "deliver", "deliveredText": text, "delivery": dump_delivery(&d) }));
            }
            if name == "deliver_trimmed_mini_recap" {
                let mut b = empty_recall_bundle();
                b.sections.retrospective =
                    Some("### Relevant Past Conversations\n\n#### The Lighthouse (`conv-1`)\n".to_string());
                b.retro_signature = Some("sig".to_string());
                let text = format!(
                    "before\n\n{}\n\nafter",
                    b.sections.retrospective.as_deref().unwrap().trim()
                );
                let d = deliver_recall(&db, &b, DeliverOptions {
                    character_id: &lyra,
                    chat_id: &chat_id,
                    commonplace_recall_history: &Value::Null,
                    delivered_text: &text,
                    persist_ring: true,
                }).await;
                steps.push(json!({ "op": "deliver", "deliveredText": text, "delivery": dump_delivery(&d) }));
            }
            (steps, ())
        }));

        // Steps, field by field.
        let want_steps = want["steps"].as_array().unwrap();
        assert_eq!(steps.len(), want_steps.len(), "{name}: step count");
        for (i, (got, w)) in steps.iter().zip(want_steps).enumerate() {
            let (g, w) = (got.as_object().unwrap(), w.as_object().unwrap());
            for (k, wv) in w {
                if k == "bundle" || k == "delivery" {
                    for (kk, wvv) in wv.as_object().unwrap() {
                        let gv = &g[k][kk];
                        assert_eq!(
                            serde_json::to_string(gv).unwrap(),
                            serde_json::to_string(wvv).unwrap(),
                            "{name}: step {i} {k}.{kk}"
                        );
                    }
                } else {
                    assert_eq!(&g[k], wv, "{name}: step {i} {k}");
                }
            }
        }
        assert_eq!(
            Value::from(embedding.0.lock().unwrap().clone()),
            want["embedTexts"],
            "{name}: the embedded texts"
        );

        // The pipeline's own lines (message + fields, in v4's order).
        let got_logs: Vec<&String> = logs
            .iter()
            .filter(|l| l.contains(" [Recall] ") || l.contains(" [Memory] "))
            .collect();
        let want_logs = want["logs"].as_array().unwrap();
        assert_eq!(
            got_logs.len(),
            want_logs.len(),
            "{name}: the [Recall]/[Memory] lines\nv5: {got_logs:#?}\nv4: {want_logs:#?}"
        );
        for (g, w) in got_logs.iter().zip(want_logs) {
            let msg = w["message"].as_str().unwrap();
            assert!(
                g.starts_with(&format!("{} ", w["level"].as_str().unwrap().to_uppercase()))
                    && g.contains(&format!(" {msg}")),
                "{name}: v4's {msg} vs v5's {g}"
            );
            // Each field: a scalar as ` k=v`; an object / array as its JSON under
            // the house `…Json` key (a tracing field holds no structure).
            for (k, v) in w["fields"].as_object().unwrap() {
                let want_field = match v {
                    Value::String(s) => format!(" {k}={s}"),
                    Value::Object(_) | Value::Array(_) => {
                        format!(" {k}Json={}", serde_json::to_string(v).unwrap())
                    }
                    other => format!(" {k}={other}"),
                };
                assert!(
                    g.contains(&want_field),
                    "{name}: {msg} field `{want_field}` missing from {g}"
                );
            }
        }

        // Stamps + the ring.
        let stamped: Vec<String> = db
            .read_main(|c| {
                let mut s = c.prepare(
                    "SELECT \"id\" FROM \"memories\" WHERE \"lastAccessedAt\" IS NOT NULL ORDER BY \"id\"",
                )?;
                let rows = s.query_map([], |r| r.get::<_, String>(0))?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .unwrap();
        assert_eq!(Value::from(stamped), want["stamped"], "{name}: stamped ids");
        let cid = chat_id.clone();
        let ring: Option<String> = db
            .read_main(move |c| {
                Ok(c.query_row(
                    "SELECT \"commonplaceRecallHistory\" FROM \"chats\" WHERE \"id\" = ?",
                    [cid],
                    |r| r.get::<_, Option<String>>(0),
                )?)
            })
            .unwrap();
        let ring = ring
            .map(|h| serde_json::from_str::<Value>(&h).unwrap())
            .unwrap_or(Value::Null);
        assert_eq!(ring, want["ring"], "{name}: the chat's ring");
        cases += 1;
    }
    assert_eq!(cases, 12, "the oracle lost cases");
    eprintln!("OK: recall-assemble matched the oracle ({cases} cases).");
}
