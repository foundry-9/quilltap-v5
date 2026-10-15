//! Differential (P4.D291): recall delivery — v4 `lib/memory/recall/deliver.ts`
//! (`584d28164`, pinned at `83334b9a5`) against v5's
//! `quilltap_core::services::recall::deliver`.
//!
//! Tier 1 (exact): `survivingLines` (verbatim, case / whitespace, the
//! whole-line rule, the 3-word floor, the 0.6 share, the `\p{L}\p{N}\s'-`
//! strip, the compressed arm — R-U), `traceLines` (byte-exact, the detail maps
//! in v4's key order), `ringFromDebugLog`, `withRecallLines`.
//!
//! Tier 2 (the writes): each `deliver` row plants its memories (every rendered
//! id) and its chat (the row's `commonplaceRecallHistory`) in a REAL
//! provisioned scratch instance, runs `deliver_recall`, and compares with what
//! v4's recorders saw: the delivered pools in order, the lost ids, the debug
//! record (byte-exact), the ids whose `lastAccessedAt` became non-null (= v4's
//! one `updateAccessTimeBulk` call — or none), the chat's stored ring (= v4's
//! `chats.update` patch, byte-exact JSON), and the `[Recall]` lines. The
//! failed-ring-write row drops the chats table first (v4's recorder throws);
//! its WARN is compared by message and `chatId` (the error texts are each
//! side's own).
//!
//! Generate the oracle (stage outside `.claude/`; see the case header):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-recall-deliver-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/recall-deliver.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-recall-deliver.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- recall-deliver
//! Run:
//!   QT_ORACLE_RECALL_DELIVER=/tmp/oracle-recall-deliver.ndjson \
//!     cargo test -p quilltap-harness --test recall_deliver_equivalence -- --nocapture

use quilltap_core::db::runtime::Db;
use quilltap_core::services::recall::assemble::{empty_recall_bundle, RecallSections};
use quilltap_core::services::recall::deliver::{
    deliver_recall, ring_from_debug_log, surviving_lines, trace_lines, with_recall_lines,
    DeliverOptions,
};
use quilltap_core::services::recall::types::{
    RecallPool, RecallStage, RecallTraceEntry, RenderedLine,
};
use serde_json::{json, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const CHAT: &str = "cc000000-0000-4000-8000-000000000001";
const CHARACTER: &str = "c0000000-0000-4000-8000-000000000001";

fn lines(v: &Value) -> Vec<RenderedLine> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|l| RenderedLine {
            memory_id: l["memoryId"].as_str().unwrap().to_string(),
            line: l["line"].as_str().unwrap().to_string(),
            body: l["body"].as_str().unwrap().to_string(),
        })
        .collect()
}

fn pool(name: &str) -> RecallPool {
    match name {
        "archive" => RecallPool::Archive,
        "head" => RecallPool::Head,
        "interCharacter" => RecallPool::InterCharacter,
        "recap" => RecallPool::Recap,
        _ => RecallPool::Relevant,
    }
}

fn stage(name: &str) -> RecallStage {
    match name {
        "query" => RecallStage::Query,
        "gather" => RecallStage::Gather,
        "hydrate" => RecallStage::Hydrate,
        "score" => RecallStage::Score,
        "select" => RecallStage::Select,
        "assemble" => RecallStage::Assemble,
        "render" => RecallStage::Render,
        _ => RecallStage::Deliver,
    }
}

fn trace(v: &Value) -> Vec<RecallTraceEntry> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|e| RecallTraceEntry {
                    stage: stage(e["stage"].as_str().unwrap()),
                    label: e.get("label").and_then(Value::as_str).map(str::to_string),
                    detail: e["detail"].as_object().unwrap().clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn strings(v: &Value) -> Option<Vec<String>> {
    v.as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
}

/// A v4 log row → the rig's rendered line (an object field → `<key>Json`).
fn v4_log_line(l: &Value) -> String {
    let level = l["level"].as_str().unwrap().to_uppercase();
    let mut line = format!(
        "{level} quilltap::recall {}",
        l["message"].as_str().unwrap()
    );
    for (k, v) in l["fields"].as_object().unwrap() {
        match v {
            Value::String(s) => line.push_str(&format!(" {k}={s}")),
            Value::Object(_) | Value::Array(_) => {
                line.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
            }
            other => line.push_str(&format!(" {k}={other}")),
        }
    }
    line
}

async fn scratch(memory_ids: &[String], history: &Value) -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    quilltap_core::services::provisioning::provision_fresh_instance(dir.path(), PEPPER).unwrap();
    let db = Db::open_main(dir.path().join("quilltap.db"), PEPPER).unwrap();
    let ids = memory_ids.to_vec();
    let history = if history.is_null() {
        None
    } else {
        Some(serde_json::to_string(history).unwrap())
    };
    db.write(move |w| {
        let c = w.main().connection();
        c.execute(
            "INSERT INTO chats (id, userId, title, commonplaceRecallHistory, createdAt, updatedAt) \
             VALUES (?1, 'u1', 't', ?2, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            rusqlite::params![CHAT, history],
        )?;
        for id in &ids {
            c.execute(
                "INSERT INTO memories (id, characterId, content, summary, importance, source, \
                 createdAt, updatedAt) VALUES (?1, ?2, 'c', 's', 0.5, 'AUTO', \
                 '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
                rusqlite::params![id, CHARACTER],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    (dir, db)
}

#[tokio::test(flavor = "current_thread")]
async fn recall_deliver_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_RECALL_DELIVER") else {
        eprintln!("SKIP: QT_ORACLE_RECALL_DELIVER not set");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("ndjson row");
        let kind = row["kind"].as_str().unwrap().to_string();
        match kind.as_str() {
            "surviving" => {
                let input = &row["input"];
                let ls = lines(&input["lines"]);
                let kept: Vec<&str> = surviving_lines(
                    &ls,
                    input["text"].as_str().unwrap(),
                    input["compressed"] == true,
                )
                .into_iter()
                .map(|l| l.memory_id.as_str())
                .collect();
                assert_eq!(json!(kept), row["kept"], "surviving/{}", row["id"]);
            }
            "traceLines" => {
                assert_eq!(
                    json!(trace_lines(&trace(&row["input"]))),
                    row["output"],
                    "traceLines"
                );
            }
            "ring" => {
                let input = strings(&row["input"]);
                assert_eq!(
                    json!(ring_from_debug_log(input.as_deref())),
                    row["output"],
                    "ringFromDebugLog({})",
                    row["input"]
                );
            }
            "withRecall" => {
                let existing = strings(&row["existing"]);
                let add = strings(&row["add"]).unwrap();
                assert_eq!(
                    json!(with_recall_lines(existing.as_deref(), &add)),
                    row["output"],
                    "withRecallLines"
                );
            }
            "deliver" => {
                let id = row["id"].as_str().unwrap();
                let input = &row["input"];
                let mut bundle = empty_recall_bundle();
                let mut all_ids: Vec<String> = Vec::new();
                for entry in input["lines"].as_array().unwrap() {
                    let ls = lines(&entry[1]);
                    all_ids.extend(ls.iter().map(|l| l.memory_id.clone()));
                    bundle.lines.push((pool(entry[0].as_str().unwrap()), ls));
                }
                let sec = |k: &str| {
                    input["sections"]
                        .get(k)
                        .and_then(Value::as_str)
                        .map(str::to_string)
                };
                bundle.sections = RecallSections {
                    recap: sec("recap"),
                    relevant: sec("relevant"),
                    inter_character: sec("interCharacter"),
                    relevant_conversations: sec("relevantConversations"),
                    retrospective: sec("retrospective"),
                };
                bundle.retro_signature = input["retroSignature"].as_str().map(str::to_string);
                bundle.trace = trace(&input["trace"]);
                let history = input.get("history").cloned().unwrap_or(Value::Null);
                let (_dir, db) = scratch(&all_ids, &history).await;
                if input["failRing"] == true {
                    db.write(|w| {
                        w.main().connection().execute_batch("DROP TABLE chats")?;
                        Ok(())
                    })
                    .await
                    .unwrap();
                }
                let delivered_text = input["deliveredText"].as_str().unwrap().to_string();
                let persist_ring = input["persistRing"] != false;
                let (delivery, logs) =
                    quilltap_core::test_support::global_capture::capture_async(deliver_recall(
                        &db,
                        &bundle,
                        DeliverOptions {
                            character_id: CHARACTER,
                            chat_id: CHAT,
                            commonplace_recall_history: &history,
                            delivered_text: &delivered_text,
                            persist_ring,
                        },
                    ))
                    .await;
                let got_delivered: Vec<Value> = delivery
                    .delivered
                    .iter()
                    .map(|(p, ids)| json!([p.as_str(), ids]))
                    .collect();
                assert_eq!(
                    json!(got_delivered),
                    row["delivered"],
                    "deliver/{id}: delivered"
                );
                assert_eq!(json!(delivery.lost), row["lost"], "deliver/{id}: lost");
                assert_eq!(
                    serde_json::to_string(&delivery.debug_log).unwrap(),
                    serde_json::to_string(&row["debugLog"]).unwrap(),
                    "deliver/{id}: the record"
                );
                // The stamps: v4's one bulk call (or none) = the rows now stamped.
                let want_stamped: Vec<String> = row["stampCalls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|c| strings(&c["ids"]).unwrap())
                    .collect();
                let mut got_stamped: Vec<String> = db
                    .read_main(|c| {
                        let mut s =
                            c.prepare("SELECT id FROM memories WHERE lastAccessedAt IS NOT NULL")?;
                        let ids = s
                            .query_map([], |r| r.get::<_, String>(0))?
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(ids)
                    })
                    .unwrap();
                let mut want_sorted = want_stamped.clone();
                want_sorted.sort();
                got_stamped.sort();
                assert_eq!(got_stamped, want_sorted, "deliver/{id}: stamped rows");
                // The ring: v4's patch = the stored cell.
                if input["failRing"] != true {
                    let stored: Option<String> = db
                        .read_main(|c| {
                            Ok(c.query_row(
                                "SELECT commonplaceRecallHistory FROM chats WHERE id = ?1",
                                [CHAT],
                                |r| r.get::<_, Option<String>>(0),
                            )?)
                        })
                        .unwrap();
                    match row["ringWrites"].as_array().unwrap().first() {
                        Some(w) => assert_eq!(
                            stored.as_deref(),
                            Some(
                                serde_json::to_string(&w["patch"]["commonplaceRecallHistory"])
                                    .unwrap()
                                    .as_str()
                            ),
                            "deliver/{id}: the ring"
                        ),
                        None => assert_eq!(
                            stored,
                            if history.is_null() {
                                None
                            } else {
                                Some(serde_json::to_string(&history).unwrap())
                            },
                            "deliver/{id}: no ring write"
                        ),
                    }
                }
                let got_logs: Vec<String> = logs
                    .into_iter()
                    .filter(|l| l.contains("[Recall]"))
                    .collect();
                let want_logs: Vec<String> = row["logs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(v4_log_line)
                    .collect();
                if input["failRing"] == true {
                    let shape = |ls: &[String]| -> Vec<String> {
                        ls.iter()
                            .map(|l| match l.find(" error=") {
                                Some(i) if l.contains("anti-repetition ring") => l[..i].to_string(),
                                _ => l.clone(),
                            })
                            .collect()
                    };
                    assert_eq!(
                        shape(&got_logs),
                        shape(&want_logs),
                        "deliver/{id}: the lines"
                    );
                } else {
                    assert_eq!(got_logs, want_logs, "deliver/{id}: the lines");
                }
            }
            other => panic!("unknown kind {other}"),
        }
        *counts.entry(kind).or_default() += 1;
    }
    eprintln!("recall_deliver rows: {counts:?}");
    assert_eq!(counts.get("surviving"), Some(&12));
    assert_eq!(counts.get("deliver"), Some(&11));
    assert_eq!(counts.get("ring"), Some(&8));
    assert_eq!(counts.get("withRecall"), Some(&3));
}
