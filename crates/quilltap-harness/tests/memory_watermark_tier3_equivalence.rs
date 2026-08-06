//! Tier-3 differential test: the memory gate's **watermark auto-housekeeping
//! check** (v4 `maybeEnqueueHousekeeping` + `enqueueMemoryHousekeeping` + the
//! in-memory housekeeping-outcome cache — the gate deferral this unit closes).
//!
//! Exercised the way it fires in production: each scenario drives ONE
//! `create_memory_with_gate` INSERT (canned embedding, empty vector store) and
//! the `background_jobs` table is the state under test — a real enqueue at the
//! watermark, the below-watermark and disabled-settings no-ops, the
//! `perCharacterCapOverrides` raise, the durable 15-minute throttle (a seeded
//! COMPLETED sweep with a FUTURE `updatedAt` is always inside the window), the
//! in-flight dedupe (a seeded PENDING job), and the in-memory
//! ineffective-sweep back-off (both sides record the same outcome through
//! their real cache before the write). Four tables diffed in the
//! shared-id-map remap form.
//!
//! Generate the fixture + oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   TMPO=/tmp/qt-memory-watermark-tier3-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/harness/oracle/cases" "$TMPO/harness/oracle/fixtures"
//!   cp "$V5/harness/oracle/cases/memory-watermark-tier3.test.ts" "$TMPO/harness/oracle/cases/"
//!   cp "$V5/harness/oracle/fixtures/memory-watermark-tier3.json" "$TMPO/harness/oracle/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-memory-watermark.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-memory-watermark-fixture.ts
//!   QT_FIXTURE_WATERMARK=/tmp/qt-memory-watermark.db \
//!   QT_ORACLE_OUT=/tmp/oracle-memory-watermark.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/harness/oracle/cases" -- memory-watermark-tier3
//! Run:
//!   QT_ORACLE_WATERMARK=/tmp/oracle-memory-watermark.ndjson \
//!   QT_FIXTURE_WATERMARK=/tmp/qt-memory-watermark.db \
//!     cargo test -p quilltap-harness --test memory_watermark_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::housekeeping_outcome_cache::record_housekeeping_outcome;
use quilltap_core::services::memory_gate::{
    create_memory_with_gate, CreateMemoryOptions, GateAction, MemoryServiceOptions,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordOutcomeW {
    deleted: f64,
    total_before: f64,
    cap: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CandidateW {
    content: String,
    summary: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioW {
    name: String,
    user_id: String,
    character_id: String,
    #[serde(default)]
    record_outcome: Option<RecordOutcomeW>,
    candidate: CandidateW,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    canned_embeddings: HashMap<String, Vec<f32>>,
    scenarios: Vec<ScenarioW>,
}

struct TableSpec {
    table: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
}

// Walk order must match the oracle's line order (the shared id map is
// first-seen).
const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "background_jobs",
        order_by: "payload",
        id_columns: &["id"],
        ts_columns: &[
            "createdAt",
            "updatedAt",
            "scheduledAt",
            "startedAt",
            "completedAt",
        ],
    },
    TableSpec {
        table: "memories",
        order_by: "content",
        id_columns: &["id"],
        ts_columns: &[
            "createdAt",
            "updatedAt",
            "lastReinforcedAt",
            "lastAccessedAt",
        ],
    },
    TableSpec {
        table: "vector_indices",
        order_by: "id",
        id_columns: &[],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        table: "vector_entries",
        order_by: "embedding",
        id_columns: &["id"],
        ts_columns: &["createdAt"],
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memory-watermark-tier3.json")
}

fn token_for(id_map: &mut HashMap<String, String>, raw: &str) -> String {
    let next = format!("ID_{}", id_map.len());
    id_map.entry(raw.to_string()).or_insert(next).clone()
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

fn oracle_table(oracle_text: &str, table: &str) -> Value {
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
        if v.get("table").and_then(Value::as_str) == Some(table) {
            return v;
        }
    }
    panic!("oracle ndjson missing table {table}");
}

#[tokio::test]
async fn memory_watermark_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_WATERMARK") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_WATERMARK to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_WATERMARK") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_WATERMARK to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let work = std::env::temp_dir().join(format!(
        "qt-memory-watermark-rust-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let mut provider = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        provider = provider.with_vector(text.clone(), vec.clone());
    }

    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    for scenario in &spec.scenarios {
        if let Some(record) = &scenario.record_outcome {
            record_housekeeping_outcome(
                &scenario.character_id,
                record.deleted,
                record.total_before,
                record.cap,
            );
        }
        let data = CreateMemoryOptions {
            character_id: scenario.character_id.clone(),
            content: scenario.candidate.content.clone(),
            summary: scenario.candidate.summary.clone(),
            source: Some("AUTO".to_string()),
            ..Default::default()
        };
        let opts = MemoryServiceOptions {
            user_id: scenario.user_id.clone(),
            embedding_profile_id: None,
        };
        let outcome = create_memory_with_gate(&db, &provider, &data, &opts)
            .await
            .unwrap_or_else(|e| panic!("gate {}: {e:?}", scenario.name));
        assert_eq!(
            outcome.action,
            GateAction::Insert,
            "scenario {} action",
            scenario.name
        );
    }

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            db.read_main(|conn| dump_table_json_conn(conn, t.table, t.order_by))
                .unwrap_or_else(|e| panic!("dump {}: {e:?}", t.table))
        })
        .collect();
    drop(db);
    let _ = std::fs::remove_file(&work);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|t| oracle_table(&oracle_text, t.table))
        .collect();

    normalize_all(&mut got);
    normalize_all(&mut want);

    for (i, t) in TABLES.iter().enumerate() {
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{} column set / order",
            t.table
        );
        assert_eq!(got[i]["rows"], want[i]["rows"], "{} rows diverge", t.table);
    }
}
