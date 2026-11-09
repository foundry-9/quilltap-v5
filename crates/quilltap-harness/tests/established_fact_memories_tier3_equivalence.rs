//! Tier-3 differential (no LLM; tier 2 on the writes): the established-fact
//! memory job handler (P4.D372, v4 `db6190b44`, pin `624d0d25c`) — v4's REAL
//! `handleEstablishedFactMemories` against
//! `quilltap_core::services::established_fact_memories::handle_established_fact_memories`.
//!
//! Both sides run the corpus's eight cases IN ORDER over one copy of the
//! carina-memory-extraction fixture pair, after the same plants (participant
//! lists by raw UPDATE, fact rows through `add_message`, an archive before its
//! case). Per case, every `[EstablishedFactMemories]` line is compared (level,
//! message, fields; the minted `memoryId` tokenized). At the end the `memories`
//! table is compared row by row (ids remapped to a shared token map, wall-clock
//! columns masked; `anchorTimeMs`-derived columns come from the fact rows'
//! pinned `createdAt`, so nothing `Date.now()` fed reaches the dump).
//!
//! The handler is driven in-process with `key_scope: None`, as the oracle
//! calls v4's handler outside a child job scope. Rows: the cast (active,
//! absent and silent seats admitted; a removed and a user-controlled seat
//! filtered; a missing character warned), a repeat post (admission absorbs on
//! every seat), a missing chat, a missing message, a non-fact row, a
//! whitespace-only fact (silent), a project chat (`projectId` carried) and an
//! archived seat. A characterless seat is unreachable (v4's chat schema
//! refuses it) — recorded in the corpus.
//!
//! Generate (see the case header):
//!   QT_FIXTURE_OUT=/tmp/qt-efm-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-efm-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-carina-memory-extraction-fixture.ts
//!   … npx jest … -- established-fact-memories-tier3   (QT_ORACLE_OUT=/tmp/oracle-ef-memories.ndjson)
//! Run:
//!   QT_ORACLE_EF_MEMORIES=/tmp/oracle-ef-memories.ndjson \
//!   QT_FIXTURE_EFM_MAIN=/tmp/qt-efm-main.db QT_FIXTURE_EFM_MOUNT=/tmp/qt-efm-mount.db \
//!     cargo test -p quilltap-harness --test established_fact_memories_tier3_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::background_jobs::BackgroundJob;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::established_fact_memories::{
    handle_established_fact_memories, EstablishedFactMemoriesDeps,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseW {
    name: String,
    chat_id: String,
    message_id: String,
    #[serde(default)]
    archive_before: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FactW {
    chat_id: String,
    event: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    now_ms: f64,
    chat_participants: serde_json::Map<String, Value>,
    facts: Vec<FactW>,
    cases: Vec<CaseW>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/established-fact-memories-tier3.json")
}

fn token(map: &mut HashMap<String, String>, raw: &str) -> String {
    let next = format!("ID_{}", map.len());
    map.entry(raw.to_string()).or_insert(next).clone()
}

/// A captured v5 line → v4's `{level, message, fields}` (fields as strings,
/// numbers as their JSON text — the rig renders every field as text).
fn parse_line(line: &str) -> Value {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default().to_lowercase();
    let _target = parts.next();
    let rest = parts.next().unwrap_or_default();
    let tag_end = rest.find(" jobId=").unwrap_or(rest.len());
    let message = rest[..tag_end].to_string();
    let mut fields = serde_json::Map::new();
    for kv in rest[tag_end..].split_whitespace() {
        if let Some((k, v)) = kv.split_once('=') {
            fields.insert(k.to_string(), Value::String(v.to_string()));
        }
    }
    json!({"level": level, "message": message, "fields": fields})
}

fn v4_line(l: &Value, ids: &mut HashMap<String, String>) -> Value {
    let mut fields = serde_json::Map::new();
    for (k, v) in l["fields"].as_object().unwrap() {
        let s = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let s = if k == "memoryId" { token(ids, &s) } else { s };
        fields.insert(k.clone(), Value::String(s));
    }
    json!({"level": l["level"], "message": l["message"], "fields": fields})
}

fn normalize_memories(dump: &mut Value, ids: &mut HashMap<String, String>) {
    for row in dump["rows"].as_array_mut().unwrap() {
        let o = row.as_object_mut().unwrap();
        if let Some(Value::String(id)) = o.get("id").cloned() {
            o.insert("id".into(), Value::String(token(ids, &id)));
        }
        if let Some(Value::String(raw)) = o.get("relatedMemoryIds").cloned() {
            if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&raw) {
                let mapped: Vec<Value> = items
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map_or(v.clone(), |s| Value::String(token(ids, s)))
                    })
                    .collect();
                o.insert(
                    "relatedMemoryIds".into(),
                    Value::String(Value::Array(mapped).to_string()),
                );
            }
        }
        for c in [
            "createdAt",
            "updatedAt",
            "lastReinforcedAt",
            "lastAccessedAt",
        ] {
            if o.get(c).is_some_and(|v| !v.is_null()) {
                o.insert(c.into(), Value::String("<ts>".into()));
            }
        }
    }
}

#[test]
fn established_fact_memories_tier3_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_EF_MEMORIES") else {
        eprintln!("SKIP: set QT_ORACLE_EF_MEMORIES (see header).");
        return;
    };
    let (Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_FIXTURE_EFM_MAIN"),
        std::env::var("QT_FIXTURE_EFM_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_FIXTURE_EFM_MAIN / QT_FIXTURE_EFM_MOUNT (see header).");
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let ran: Vec<&Value> = oracle.iter().filter(|r| r["kind"] == "ran").collect();
    assert_eq!(ran.len(), spec.cases.len(), "regenerate the oracle");
    assert!(
        std::fs::read_to_string(&oracle_path)
            .unwrap()
            .contains("Established fact admitted to the cast"),
        "non-vacuity: v4's closing INFO"
    );

    let mut embedding = CannedEmbeddingProvider::new();
    for r in oracle.iter().filter(|r| r["kind"] == "cannedEmbedding") {
        let v: Vec<f32> = serde_json::from_value(r["vector"].clone()).unwrap();
        embedding = embedding.with_vector(r["text"].as_str().unwrap().to_string(), v);
    }

    let dir = tempfile::tempdir().unwrap();
    let work_main = dir.path().join("efm-main.db");
    let work_mount = dir.path().join("efm-mount.db");
    std::fs::copy(&fixture_main, &work_main).unwrap();
    std::fs::copy(&fixture_mount, &work_mount).unwrap();
    let db = Db::open(
        DbPaths {
            main: work_main,
            mount_index: Some(work_mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // Plants (the oracle's, in its order).
    let participants = spec.chat_participants.clone();
    let facts: Vec<(String, Value)> = spec
        .facts
        .iter()
        .map(|f| (f.chat_id.clone(), f.event.clone()))
        .collect();
    rt.block_on(db.write(move |w| {
        let c = w.main().connection();
        for (chat_id, ps) in &participants {
            let n = c.execute(
                "UPDATE chats SET participants = ?1 WHERE id = ?2",
                rusqlite::params![ps.to_string(), chat_id],
            )?;
            assert_eq!(n, 1, "the participant plant must hit its chat");
        }
        for (chat_id, ev) in &facts {
            let ev: quilltap_core::db::chats_messages::ChatEventInput =
                serde_json::from_value(ev.clone()).expect("a fact event");
            w.main().chat_messages().add_message(chat_id, &ev)?;
        }
        Ok(())
    }))
    .unwrap();

    // One token map PER SIDE, filled in the same order (lines, then the dump):
    // minted ids differ between the engines, their positions must not.
    let mut ids5: HashMap<String, String> = HashMap::new();
    let mut ids4: HashMap<String, String> = HashMap::new();
    let mut failures = Vec::new();
    for (case, want) in spec.cases.iter().zip(&ran) {
        assert_eq!(want["case"], case.name.as_str());
        if let Some(char_id) = case.archive_before.clone() {
            rt.block_on(db.write(move |w| {
                w.main().connection().execute(
                    "UPDATE characters SET archivedAt = ?1 WHERE id = ?2",
                    rusqlite::params!["2023-07-01T00:00:00.000Z", char_id],
                )?;
                Ok(())
            }))
            .unwrap();
        }
        let job = BackgroundJob {
            id: format!("job-{}", case.name),
            user_id: spec.user_id.clone(),
            job_type: "ESTABLISHED_FACT_MEMORIES".into(),
            status: "PROCESSING".into(),
            payload: json!({"chatId": case.chat_id, "messageId": case.message_id}).to_string(),
            priority: 0.0,
            attempts: 1.0,
            max_attempts: 3.0,
            last_error: None,
            scheduled_at: String::new(),
            started_at: None,
            completed_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let (_, lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(handle_established_fact_memories(
                &db,
                &job,
                EstablishedFactMemoriesDeps {
                    provider: &embedding,
                    key_scope: None,
                    host_zone_fallback: "UTC",
                    now_ms: spec.now_ms,
                },
            ))
            .unwrap()
        });
        let got: Vec<Value> = lines
            .iter()
            .filter(|l| l.contains("[EstablishedFactMemories]"))
            .map(|l| {
                let mut v = parse_line(l);
                if let Some(Value::String(m)) = v["fields"].get("memoryId").cloned() {
                    v["fields"]["memoryId"] = Value::String(token(&mut ids5, &m));
                }
                v
            })
            .collect();
        let want_lines: Vec<Value> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| v4_line(l, &mut ids4))
            .collect();
        if got != want_lines {
            failures.push(format!(
                "{}: lines\n  v5 {}\n  v4 {}",
                case.name,
                serde_json::to_string(&got).unwrap(),
                serde_json::to_string(&want_lines).unwrap()
            ));
        }
    }

    let mut got_mem = db
        .read_main(|c| dump_table_json_conn(c, "memories", "id"))
        .unwrap();
    let mut want_mem = oracle
        .iter()
        .find(|r| r["kind"] == "table")
        .cloned()
        .expect("memories dump");
    // Same order both sides (the oracle's: characterId | content | id).
    let key = |r: &Value| {
        format!(
            "{}|{}|{}",
            r["characterId"].as_str().unwrap_or_default(),
            r["content"].as_str().unwrap_or_default(),
            r["id"].as_str().unwrap_or_default()
        )
    };
    if let Some(rows) = got_mem["rows"].as_array_mut() {
        rows.sort_by_key(key);
    }
    normalize_memories(&mut want_mem, &mut ids4);
    normalize_memories(&mut got_mem, &mut ids5);
    if got_mem["columns"] != want_mem["columns"] {
        failures.push(format!(
            "memories columns\n  v5 {}\n  v4 {}",
            got_mem["columns"], want_mem["columns"]
        ));
    }
    let (g, w) = (
        got_mem["rows"].as_array().unwrap(),
        want_mem["rows"].as_array().unwrap(),
    );
    if g.len() != w.len() {
        failures.push(format!("memories rows: v5 {} vs v4 {}", g.len(), w.len()));
    }
    for (i, (a, b)) in g.iter().zip(w).enumerate() {
        if serde_json::to_string(a).unwrap() != serde_json::to_string(b).unwrap() {
            failures.push(format!("memories row {i}\n  v5 {a}\n  v4 {b}"));
        }
    }
    eprintln!(
        "established_fact_memories_tier3: {} cases, {} memories",
        ran.len(),
        w.len()
    );
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
