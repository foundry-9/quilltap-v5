//! Tier-3 differential test: the **memory-pipeline job handlers** (P4.6bj
//! units 2–3 — `quilltap_core::services::memory_extraction_job` /
//! `context_summary_job`, v4 `handleMemoryExtraction` +
//! `handleContextSummary`).
//!
//! Both sides copy the same two-DB seed fixture and run the same ten-case
//! sequence (six MEMORY_EXTRACTION + four CONTEXT_SUMMARY). The oracle drives
//! v4's REAL handlers with only the model/infra seams mocked (see the case
//! header); this test replays the RECORDED canned completions + embeddings
//! through the ported handlers — the extraction path through
//! `build_turn_transcript` + `process_turn_for_memory`, the summary path
//! through `generate_context_summary_with_seams(RealContextSummarySeams)` —
//! then diffs six tables (`memories`, `vector_indices`, `vector_entries`,
//! `chat_messages`, `chats`, `background_jobs`) in the shared-cross-table
//! minted-values remap form, and asserts each case's thrown-error string
//! matches the oracle's.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; the /tmp
//! mirror dodges jest's `/.claude/` testPathIgnorePatterns):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this worktree>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-mpj-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-mpj-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-memory-pipeline-jobs-fixture.ts
//!   mkdir -p /tmp/qt-mpj-oracle/cases /tmp/qt-mpj-oracle/fixtures
//!   cp $V5/harness/oracle/cases/memory-pipeline-jobs-tier3.test.ts /tmp/qt-mpj-oracle/cases/
//!   cp $V5/harness/oracle/fixtures/memory-pipeline-jobs-tier3.json /tmp/qt-mpj-oracle/fixtures/
//!   TZ=UTC QT_FIXTURE_MPJ_MAIN=/tmp/qt-mpj-main.db QT_FIXTURE_MPJ_MOUNT=/tmp/qt-mpj-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-memory-pipeline-jobs.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots /tmp/qt-mpj-oracle/cases -- memory-pipeline-jobs-tier3
//! Run (TZ pinned to match the oracle's date math):
//!   TZ=UTC QT_ORACLE_MPJ=/tmp/oracle-memory-pipeline-jobs.ndjson \
//!   QT_FIXTURE_MPJ_MAIN=/tmp/qt-mpj-main.db QT_FIXTURE_MPJ_MOUNT=/tmp/qt-mpj-mount.db \
//!     cargo test -p quilltap-harness --test memory_pipeline_jobs_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::context_summary_job::{handle_context_summary, ContextSummaryPayload};
use quilltap_core::services::cost_estimation::MessageCostEstimator;
use quilltap_core::services::memory_extraction_job::{
    handle_memory_extraction, MemoryExtractionPayload,
};
use quilltap_core::services::memory_processor::MemoryExtractionLimits;
use serde::Deserialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Spec structures (harness/oracle/fixtures/memory-pipeline-jobs-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LimitsW {
    enabled: bool,
    max_per_hour: f64,
    soft_start_fraction: f64,
    soft_floor: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeCaseW {
    name: String,
    chat_id: String,
    turn_opener_message_id: Option<String>,
    extraction_anchor_message_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CsCaseW {
    name: String,
    chat_id: String,
    force_regenerate: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    connection_profile_id: String,
    memory_extraction_limits: LimitsW,
    me_cases: Vec<MeCaseW>,
    cs_cases: Vec<CsCaseW>,
}

// ---------------------------------------------------------------------------
// Oracle rows.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CannedMessageW {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct CannedUsageW {
    #[serde(rename = "promptTokens")]
    prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    completion_tokens: i64,
    #[serde(rename = "totalTokens")]
    total_tokens: i64,
}

#[derive(Deserialize)]
struct CannedRowW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMessageW>,
    response: String,
    usage: CannedUsageW,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memory-pipeline-jobs-tier3.json")
}

// The oracle's `estimateMessageCost` mock returns `{ cost: null }`.
struct NullCost;

impl MessageCostEstimator for NullCost {
    async fn estimate(
        &self,
        _provider: &str,
        _model: &str,
        _prompt_tokens: i64,
        _completion_tokens: i64,
        _user_id: &str,
    ) -> Option<f64> {
        None
    }
}

// ---------------------------------------------------------------------------
// Table normalization — the shared-cross-table id-map remap form.
// ---------------------------------------------------------------------------

struct TableSpec {
    table: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    id_array_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    /// Free-text columns that may EMBED minted ids (the gate's "absorbed
    /// into <uuid>" debug line) — every UUID already in the shared map is
    /// replaced by its token; unknown UUIDs (pinned ids) stay literal.
    text_id_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "memories",
        order_by: "content",
        id_columns: &["id", "sourceMessageId"],
        id_array_columns: &["relatedMemoryIds"],
        ts_columns: &[
            "createdAt",
            "updatedAt",
            "lastReinforcedAt",
            "lastAccessedAt",
        ],
        text_id_columns: &[],
    },
    TableSpec {
        table: "vector_entries",
        order_by: "embedding",
        id_columns: &["id"],
        id_array_columns: &[],
        ts_columns: &["createdAt"],
        text_id_columns: &[],
    },
    TableSpec {
        table: "vector_indices",
        order_by: "id",
        id_columns: &[],
        id_array_columns: &[],
        ts_columns: &["createdAt", "updatedAt"],
        text_id_columns: &[],
    },
    TableSpec {
        table: "chat_messages",
        order_by: "content",
        // Seeded message ids are pinned (tokenized consistently both sides);
        // the librarian whisper + system-event rows carry minted ids.
        id_columns: &["id"],
        id_array_columns: &[],
        ts_columns: &["createdAt"],
        text_id_columns: &["debugMemoryLogs"],
    },
    TableSpec {
        table: "chats",
        order_by: "id",
        id_columns: &[],
        id_array_columns: &[],
        ts_columns: &["updatedAt", "lastMessageAt"],
        text_id_columns: &[],
    },
    TableSpec {
        table: "background_jobs",
        order_by: "type",
        // The chained CHAT_DANGER_CLASSIFICATION rows mint fresh job ids; the
        // payloads carry only pinned chat/profile ids.
        id_columns: &["id"],
        id_array_columns: &[],
        ts_columns: &["createdAt", "updatedAt", "scheduledAt"],
        text_id_columns: &[],
    },
];

fn token_for(id_map: &mut HashMap<String, String>, raw: &str) -> String {
    let next = format!("ID_{}", id_map.len());
    id_map.entry(raw.to_string()).or_insert(next).clone()
}

/// Replace every UUID-shaped substring that is already in the shared id map
/// with its token (embedded minted ids in free-text cells). Unknown UUIDs stay
/// literal — pinned ids are identical on both sides.
fn replace_mapped_uuids(raw: &str, id_map: &HashMap<String, String>) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    let is_uuid_char = |c: u8| c.is_ascii_hexdigit() || c == b'-';
    while i < bytes.len() {
        if bytes[i].is_ascii_hexdigit() {
            let mut j = i;
            while j < bytes.len() && is_uuid_char(bytes[j]) {
                j += 1;
            }
            let cand = &raw[i..j];
            if cand.len() == 36 {
                if let Some(token) = id_map.get(cand) {
                    out.push_str(token);
                    i = j;
                    continue;
                }
            }
            out.push_str(cand);
            i = j;
        } else {
            let ch = raw[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn normalize_table(dump: &mut Value, spec: &TableSpec, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{}: dump has no rows array", spec.table));

    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{}: row is not an object", spec.table));

        for col in spec.id_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let token = token_for(id_map, raw);
                obj.insert((*col).to_string(), Value::String(token));
            }
        }

        for col in spec.id_array_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(raw) {
                    let mapped: Vec<Value> = items
                        .iter()
                        .map(|v| match v.as_str() {
                            Some(s) => Value::String(token_for(id_map, s)),
                            None => v.clone(),
                        })
                        .collect();
                    let re = serde_json::to_string(&Value::Array(mapped)).unwrap();
                    obj.insert((*col).to_string(), Value::String(re));
                }
            }
        }

        for col in spec.text_id_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let replaced = replace_mapped_uuids(raw, id_map);
                obj.insert((*col).to_string(), Value::String(replaced));
            }
        }

        for col in spec.ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

#[tokio::test]
async fn memory_pipeline_jobs_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MPJ") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MPJ to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture_main = match std::env::var("QT_FIXTURE_MPJ_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MPJ_MAIN to the seed main .db (see header).");
            return;
        }
    };
    let fixture_mount = match std::env::var("QT_FIXTURE_MPJ_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MPJ_MOUNT to the seed mount .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let mut oracle_canned: Vec<CannedRowW> = Vec::new();
    let mut oracle_embeddings: Vec<(String, Vec<f32>)> = Vec::new();
    let mut oracle_tables: HashMap<String, Value> = HashMap::new();
    let mut oracle_errors: HashMap<String, Option<String>> = HashMap::new();
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("ran") => {
                oracle_errors.insert(
                    v["call"].as_str().unwrap().to_string(),
                    v.get("error").and_then(Value::as_str).map(str::to_string),
                );
            }
            Some("canned") => {
                oracle_canned.push(serde_json::from_value(v).expect("parse canned row"))
            }
            Some("cannedEmbedding") => {
                let text = v["text"].as_str().expect("embedding text").to_string();
                let vec: Vec<f32> = v["vector"]
                    .as_array()
                    .expect("embedding vector")
                    .iter()
                    .map(|x| x.as_f64().unwrap() as f32)
                    .collect();
                oracle_embeddings.push((text, vec));
            }
            Some("table") => {
                oracle_tables.insert(v["table"].as_str().unwrap().to_string(), v);
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }
    assert_eq!(
        oracle_errors.len(),
        spec.me_cases.len() + spec.cs_cases.len(),
        "oracle ran a different number of cases — regenerate the oracle NDJSON"
    );

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-mpj-main-rust-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-mpj-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fixture_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));

    // Replay EXACTLY the oracle-recorded entries — an input the oracle never
    // sent surfaces as a canned-miss, not an answer.
    let mut completion = CannedCompletionProvider::new();
    for row in &oracle_canned {
        let messages: Vec<CompletionMessage> = row
            .messages
            .iter()
            .map(|m| CompletionMessage {
                role: match m.role.as_str() {
                    "system" => CompletionRole::System,
                    "user" => CompletionRole::User,
                    "assistant" => CompletionRole::Assistant,
                    other => panic!("unexpected role {other}"),
                },
                content: m.content.clone(),
            })
            .collect();
        completion = completion.with_response(
            &row.provider,
            &row.model,
            row.temperature,
            &messages,
            row.response.clone(),
            Some(CompletionUsage {
                prompt_tokens: row.usage.prompt_tokens,
                completion_tokens: row.usage.completion_tokens,
                total_tokens: row.usage.total_tokens,
            }),
        );
    }

    let mut embedding = CannedEmbeddingProvider::new();
    for (text, vec) in &oracle_embeddings {
        embedding = embedding.with_vector(text.clone(), vec.clone());
    }

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    let executor = CheapLlmTaskExecutor::new();
    let cost = NullCost;
    let limits = MemoryExtractionLimits {
        enabled: spec.memory_extraction_limits.enabled,
        max_per_hour: spec.memory_extraction_limits.max_per_hour,
        soft_start_fraction: spec.memory_extraction_limits.soft_start_fraction,
        soft_floor: spec.memory_extraction_limits.soft_floor,
    };

    let assert_case_error = |name: &str, got: Option<String>| {
        let want = oracle_errors
            .get(name)
            .unwrap_or_else(|| panic!("{name}: missing from oracle — regenerate"));
        assert_eq!(&got, want, "{name}: thrown-error divergence");
    };

    for case in &spec.me_cases {
        let payload = MemoryExtractionPayload {
            chat_id: case.chat_id.clone(),
            turn_opener_message_id: case.turn_opener_message_id.clone(),
            extraction_anchor_message_id: case.extraction_anchor_message_id.clone(),
            connection_profile_id: spec.connection_profile_id.clone(),
        };
        let got = handle_memory_extraction(
            &db,
            &completion,
            &embedding,
            &executor,
            &cost,
            &spec.user_id,
            &payload,
            Some(limits.clone()),
        )
        .await
        .err();
        assert_case_error(&case.name, got);
    }

    for case in &spec.cs_cases {
        let payload = ContextSummaryPayload {
            chat_id: case.chat_id.clone(),
            connection_profile_id: spec.connection_profile_id.clone(),
            force_regenerate: case.force_regenerate.unwrap_or(false),
        };
        let got = handle_context_summary(
            &db,
            &completion,
            &embedding,
            &executor,
            &spec.user_id,
            &payload,
        )
        .await
        .err();
        assert_case_error(&case.name, got);
    }

    // Dump + diff the six tables in the shared-id-map remap form.
    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            db.read_main(|conn| dump_table_json_conn(conn, t.table, t.order_by))
                .unwrap_or_else(|e| panic!("dump {}: {e:?}", t.table))
        })
        .collect();
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            let mut v = oracle_tables
                .remove(t.table)
                .unwrap_or_else(|| panic!("oracle missing table {}", t.table));
            v.as_object_mut().unwrap().remove("kind");
            v
        })
        .collect();

    normalize_all(&mut got);
    normalize_all(&mut want);

    for (i, t) in TABLES.iter().enumerate() {
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{} column set / order",
            t.table
        );
        if got[i]["rows"] != want[i]["rows"] {
            // Row-level report: dump both sides to temp files, then panic with
            // the first differing row (the full arrays overflow the terminal).
            let gp = std::env::temp_dir().join(format!("qt-mpj-got-{}.json", t.table));
            let wp = std::env::temp_dir().join(format!("qt-mpj-want-{}.json", t.table));
            let _ = std::fs::write(&gp, serde_json::to_string_pretty(&got[i]["rows"]).unwrap());
            let _ = std::fs::write(&wp, serde_json::to_string_pretty(&want[i]["rows"]).unwrap());
            let ga = got[i]["rows"].as_array().unwrap();
            let wa = want[i]["rows"].as_array().unwrap();
            for (j, (g, w)) in ga.iter().zip(wa.iter()).enumerate() {
                if g != w {
                    panic!(
                        "{} rows diverge at index {j} (full dumps: {} / {})\n got: {g:#}\nwant: {w:#}",
                        t.table,
                        gp.display(),
                        wp.display()
                    );
                }
            }
            panic!(
                "{} row-count diverges: got {} vs want {} (full dumps: {} / {})",
                t.table,
                ga.len(),
                wa.len(),
                gp.display(),
                wp.display()
            );
        }
    }
    println!(
        "memory_pipeline_jobs: {} cases OK",
        spec.me_cases.len() + spec.cs_cases.len()
    );
}
