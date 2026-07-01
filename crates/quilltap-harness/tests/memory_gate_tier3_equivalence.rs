//! Tier-3 differential test: the **memory gate** service
//! (`quilltap_core::services::memory_gate` — v4 `createMemoryWithGate` /
//! `runMemoryGate`). The first model-dependent service, verified tier-3 → tier-2.
//!
//! Both sides inject the SAME canned embedding for each candidate (keyed by the
//! exact `${summary}\n\n${content}` text) — the oracle via a `jest.mock` of
//! `generateEmbeddingForUser`, the Rust side via a `CannedEmbeddingProvider` built
//! from the same corpus map — then run v4's REAL `createMemoryWithGate` /
//! `quilltap_core`'s gate over the same seeded fixture and structural-diff the
//! three affected tables (`memories`, `vector_indices`, `vector_entries`). Because
//! the model call is pinned identically, any divergence is in the orchestration.
//!
//! One scenario per outcome, each on its own character (the per-character vector
//! stores never cross): INSERT (empty store), INSERT (non-empty, all below the
//! related band), INSERT_RELATED (two seeds linked), REINFORCE (no content change),
//! REINFORCE with a content change + re-embed (novel-detail footnote → the vector
//! is regenerated + rewritten), SKIP_NEAR_DUPLICATE, SKIP_EMBEDDING_FAILED.
//!
//! NORMALIZATION: minted-values, shared cross-table id-map. The gate mints new
//! memory ids + timestamps on an INSERT; the seed rows are baked identically into
//! the fixture. A single first-seen-token id-map is shared across all three tables
//! (walked memories → vector_entries; `characterId` and the `vector_indices` id
//! stay literal, being pinned), so a minted memory id verifies by *relationship*
//! (its `vector_entries.id`, and its appearance inside a related memory's
//! `relatedMemoryIds` JSON array). Every minted timestamp is placeholdered.
//!
//! Generate the oracle output + fixture (Node, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-memory-gate-fixture.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-memory-gate-fixture.ts
//!   QT_FIXTURE_GATE=/tmp/qt-memory-gate-fixture.db \
//!   QT_ORACLE_OUT=/tmp/oracle-memory-gate.ndjson \
//!     $N/npx jest --silent --roots "$PWD" --roots "$V5/harness/oracle/cases" -- memory-gate-tier3
//! Run:
//!   QT_ORACLE_GATE=/tmp/oracle-memory-gate.ndjson \
//!   QT_FIXTURE_GATE=/tmp/qt-memory-gate-fixture.db \
//!     cargo test -p quilltap-harness --test memory_gate_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::memory_gate::{
    create_memory_with_gate, CreateMemoryOptions, GateAction, MemoryServiceOptions,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    scenarios: Vec<Scenario>,
    #[serde(rename = "cannedEmbeddings")]
    canned_embeddings: HashMap<String, Vec<f32>>,
    #[serde(rename = "cannedFailures")]
    canned_failures: Vec<String>,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "expectedAction")]
    expected_action: String,
    candidate: Candidate,
}

#[derive(Deserialize)]
struct Candidate {
    content: String,
    summary: String,
    #[serde(default)]
    source: Option<String>,
}

/// Per-table dump/normalization spec. `order_by` must be a column that is
/// deterministic and identical on both sides (never a minted id). The slice order
/// here is the canonical walk order for the shared id-remap.
struct TableSpec {
    table: &'static str,
    order_by: &'static str,
    /// Scalar id columns remapped through the shared map.
    id_columns: &'static [&'static str],
    /// JSON-array-string columns whose string elements are remapped.
    id_array_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "memories",
        order_by: "content",
        id_columns: &["id"],
        id_array_columns: &["relatedMemoryIds"],
        ts_columns: &[
            "createdAt",
            "updatedAt",
            "lastReinforcedAt",
            "lastAccessedAt",
        ],
    },
    TableSpec {
        table: "vector_entries",
        order_by: "embedding",
        id_columns: &["id"],
        id_array_columns: &[],
        ts_columns: &["createdAt"],
    },
    TableSpec {
        table: "vector_indices",
        order_by: "id",
        // id == characterId (pinned) — left literal.
        id_columns: &[],
        id_array_columns: &[],
        ts_columns: &["createdAt", "updatedAt"],
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memory-gate-tier3.json")
}

/// Remap id columns + JSON-array id elements through the shared first-seen map,
/// then placeholder minted timestamps — all in dump (walk) order.
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
                // Stored as compact JSON-array text (e.g. `["a","b"]`); remap each
                // element and re-serialize compactly (both sides use the same
                // `serde_json`/`JSON.stringify` shape).
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

        for col in spec.ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

fn token_for(id_map: &mut HashMap<String, String>, raw: &str) -> String {
    let next = format!("ID_{}", id_map.len());
    id_map.entry(raw.to_string()).or_insert(next).clone()
}

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

/// Pick the NDJSON line whose `table` field matches from the multi-line oracle.
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
async fn memory_gate_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_GATE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_GATE to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_GATE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_GATE to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    // Fresh copy so the shared seed fixture stays pristine.
    let work = std::env::temp_dir().join(format!("qt-memory-gate-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // The same canned embeddings both sides inject.
    let mut provider = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        provider = provider.with_vector(text.clone(), vec.clone());
    }
    for text in &spec.canned_failures {
        provider = provider.with_failure(text.clone());
    }

    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    for scenario in &spec.scenarios {
        let data = CreateMemoryOptions {
            character_id: scenario.character_id.clone(),
            content: scenario.candidate.content.clone(),
            summary: scenario.candidate.summary.clone(),
            source: scenario.candidate.source.clone(),
            ..Default::default()
        };
        let opts = MemoryServiceOptions {
            user_id: spec.user_id.clone(),
            embedding_profile_id: None,
        };
        let outcome = create_memory_with_gate(&db, &provider, &data, &opts)
            .await
            .unwrap_or_else(|e| panic!("gate {}: {e:?}", scenario.name));

        let got_action = match outcome.action {
            GateAction::Insert => "INSERT",
            GateAction::InsertRelated => "INSERT_RELATED",
            GateAction::Reinforce => "REINFORCE",
            GateAction::SkipNearDuplicate => "SKIP_NEAR_DUPLICATE",
            GateAction::SkipEmbeddingFailed => "SKIP_EMBEDDING_FAILED",
        };
        assert_eq!(
            got_action, scenario.expected_action,
            "scenario {} action",
            scenario.name
        );
    }

    // Dump the three tables off a read-only pooled connection (the writer thread
    // has committed each awaited write).
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
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{} row state diverged\n  rust:   {}\n  oracle: {}",
            t.table, got[i]["rows"], want[i]["rows"]
        );
    }

    // Sanity: seven memories survive (3 inserted + 4 seeds that were reinforced /
    // skipped, minus the two SKIP scenarios that wrote nothing new). Concretely:
    // insert_empty(+1), insert_low(seed+insert=2), insert_related(2 seeds+1=3),
    // reinforce(seed=1), reinforce_reembed(seed=1), skip_near_dup(seed=1) → the
    // skip_embedding_failed character has none.
    // 6 seeds (B,C×2,D,E,F) + 3 inserts (A,B,C) = 9 memory rows and 9 vector
    // entries (every insert adds one; the two SKIP scenarios and the reinforce
    // scenarios add none, and reinforce_reembed rewrites its seed in place).
    let mem_rows = got[0]["rows"].as_array().expect("memory rows");
    assert_eq!(mem_rows.len(), 9, "expected 9 memory rows");
    let entry_rows = got[1]["rows"].as_array().expect("entry rows");
    assert_eq!(entry_rows.len(), 9, "expected 9 vector entries");

    eprintln!(
        "OK: memory-gate tier-3 matched oracle (memories + vector_indices + vector_entries)."
    );
}
