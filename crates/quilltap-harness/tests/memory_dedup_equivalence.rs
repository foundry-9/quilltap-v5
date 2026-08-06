//! P4.43 MEMORY-DEDUP differential: `services::memory_dedup::deduplicate_all_memories`
//! vs v4's REAL `deduplicateAllMemories` (`lib/tools/memory-dedup.ts`), over a
//! FRESH copy of the committed `embedding-profiles-{main,mount}.db` fixture, in
//! BOTH modes:
//!   - `preview` (dryRun=true)  — diffs the result JSON; the memories table is
//!     unchanged;
//!   - `apply`   (dryRun=false) — diffs the result JSON AND the mutated memories
//!     table (survivor content rewritten with `[+]` footnotes, discards deleted,
//!     the neighbour-scrub through `deleteMemoriesWithUnlinkBatch`).
//!
//! Deterministic — NO LLM, NO jobs. `processedAt` is normalized on both sides;
//! the memories dump omits `createdAt` / `updatedAt` (a survivor's `updatedAt` is
//! minted on apply). The fixture's crafted corpus exercises: a no-cluster
//! character (Aria), a cluster WITHOUT novel-detail merge (Bram), and a cluster
//! WITH a two-detail merge over episodic-column-bearing rows (Cleo).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-memory-dedup.ndjson npx jest -- memory-dedup
//! Run:
//!   QT_ORACLE_MEMORY_DEDUP=/tmp/oracle-memory-dedup.ndjson \
//!     cargo test -p quilltap-harness --test memory_dedup_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::memory_dedup::deduplicate_all_memories;
use serde_json::{json, Map, Value};

const USER_ID: &str = "aa000000-0000-4000-8000-0000000000aa";
const THRESHOLD: f64 = 0.8;
const PEPPER: &str = "ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=";

/// The memories columns the differential compares (must match the oracle's
/// `MEM_COLS`).
const MEM_COLS: &[&str] = &[
    "id",
    "characterId",
    "content",
    "summary",
    "importance",
    "embedding",
    "occurredAt",
    "narrativeTime",
    "entities",
    "kind",
    "source",
    "keywords",
    "tags",
    "relatedMemoryIds",
];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Dump `memories` (all rows, MEM_COLS, ORDER BY id) in the oracle's shape. TEXT
/// columns emit their raw stored string (or null); `embedding` a BLOB → hex;
/// `importance` a number (all fixture values are fractional, so `from_f64`
/// renders identically to v4's `JSON.stringify`).
fn dump_memories(db: &Db) -> Value {
    db.read_main(|conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM memories ORDER BY id",
            MEM_COLS.join(", ")
        ))?;
        let rows: Vec<Value> = stmt
            .query_map([], |r| {
                let mut m = Map::new();
                for (i, col) in MEM_COLS.iter().enumerate() {
                    let v = match *col {
                        "importance" => r
                            .get::<_, Option<f64>>(i)?
                            .and_then(serde_json::Number::from_f64)
                            .map(Value::Number)
                            .unwrap_or(Value::Null),
                        "embedding" => r
                            .get::<_, Option<Vec<u8>>>(i)?
                            .map(|b| Value::String(hex(&b)))
                            .unwrap_or(Value::Null),
                        _ => r
                            .get::<_, Option<String>>(i)?
                            .map(Value::String)
                            .unwrap_or(Value::Null),
                    };
                    m.insert((*col).to_string(), v);
                }
                Ok(Value::Object(m))
            })?
            .collect::<Result<_, _>>()?;
        Ok(Value::Array(rows))
    })
    .unwrap()
}

fn open_fresh(mode: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-dedup-{mode}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("embedding-profiles-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("embedding-profiles-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db")
}

#[test]
fn memory_dedup_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MEMORY_DEDUP") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_DEDUP (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    let oracle: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(oracle.len(), 2, "expected preview + apply oracle records");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut failed = false;
    for want in &oracle {
        let mode = want["mode"].as_str().unwrap();
        let dry_run = mode == "preview";
        let db = open_fresh(mode);

        let mut result = rt
            .block_on(deduplicate_all_memories(&db, USER_ID, THRESHOLD, dry_run))
            .expect("dedup");
        // Normalize the one non-deterministic field (both sides).
        result["processedAt"] = json!("<processedAt>");

        if result != want["result"] {
            eprintln!(
                "[{mode} result] MISMATCH:\n  got:  {}\n  want: {}",
                result, want["result"]
            );
            failed = true;
        } else {
            eprintln!("[{mode} result] OK.");
        }

        let memories = dump_memories(&db);
        if memories != want["memories"] {
            eprintln!(
                "[{mode} memories] MISMATCH:\n  got:  {}\n  want: {}",
                memories, want["memories"]
            );
            failed = true;
        } else {
            eprintln!(
                "[{mode} memories] OK ({} rows).",
                memories.as_array().map(|a| a.len()).unwrap_or(0)
            );
        }
    }

    assert!(!failed, "memory-dedup differential mismatched");
    eprintln!("OK: memory_dedup matched oracle (preview + apply).");
}
