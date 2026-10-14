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
//! ## P4.D266 — the dedup FOLD (v4 `3f7320138` F9, pin `01a83539d`)
//!
//! Apply now FOLDS each cluster into its survivor before deleting (capped
//! footnotes, summed counts, unioned links, earliest `occurredAt`, a re-embed
//! when the content or the anchor date changed); a failed fold keeps its
//! losers. On each COPY both sides first run the tier migration (v4's REAL
//! `add-memory-tiers-v1` / v5's `ensure_memory_tiers` — idempotent on the
//! widened pair), then create `memory-dedup-plants.json` through the REAL
//! repositories, then plant a `BEFORE UPDATE` abort trigger on one survivor.
//! The re-embed is pinned (R-D): v4's mocked `generateEmbeddingForUser`
//! answers ONE canned vector and records every text (`embedCalls`); v5
//! registers exactly those texts on a `CannedEmbeddingProvider` and records
//! its own calls — the COUNT and the TEXTS compare. Two named rows are
//! faithful to v4 at `01a83539d` (a v4 fix flips them): `DEDUP_CROSSES_TIERS`
//! (a hot loser folds into a COLD survivor) and `DEDUP_DELETES_DIGEST` (a
//! CONSOLIDATED digest loses and is deleted; its member keeps a dangling
//! `supersededById`). R-E's proof is the C cluster: its survivor links a
//! loser of ANOTHER cluster, and the delete scrubs it from the committed list.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-memory-dedup.ndjson npx jest -- memory-dedup
//! Run:
//!   QT_ORACLE_MEMORY_DEDUP=/tmp/oracle-memory-dedup.ndjson \
//!     cargo test -p quilltap-harness --test memory_dedup_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::db::memories::{CreateOptions, MemCreate};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::model::embedding::{
    CannedEmbeddingProvider, EmbeddingCallOptions, EmbeddingError, EmbeddingProvider,
    EmbeddingResult,
};
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
    // P4.D266: the fold's count fields and the tier columns.
    "reinforcementCount",
    "reinforcedImportance",
    "lastReinforcedAt",
    "tier",
    "supersededById",
    "consolidatedFrom",
    "consolidatedAt",
];

/// The REAL-number columns (rendered as JSON numbers, like v4's dump).
const NUM_COLS: &[&str] = &["importance", "reinforcementCount", "reinforcedImportance"];

fn plants_spec() -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memory-dedup-plants.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

/// A canned provider that RECORDS the texts it is asked for (R-D: the call
/// count and texts compare against v4's `embedCalls`).
#[derive(Clone)]
struct Recording {
    inner: CannedEmbeddingProvider,
    calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl EmbeddingProvider for Recording {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        user_id: &str,
        profile_id: Option<&str>,
        opts: EmbeddingCallOptions,
    ) -> impl std::future::Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        self.calls.lock().unwrap().push(text.to_string());
        self.inner
            .generate_embedding_for_user(text, user_id, profile_id, opts)
    }
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn opt_str(v: &Value) -> Option<String> {
    v.as_str().map(String::from)
}

/// The plants + the abort trigger, on the copy, through v5's REAL repository —
/// the same rows v4's `MemoriesRepository.create` writes.
fn plant(main_path: &std::path::Path, spec: &Value) {
    let w = Writer::open_writable(main_path, PEPPER).expect("open the copy writable");
    // v5's twin of `applyMemoryTiersMigration` (idempotent on the widened pair).
    quilltap_core::test_support::ensure_memory_tiers_columns(w.connection());
    let created_at = spec["createdAt"].as_str().unwrap().to_string();
    for p in spec["plants"].as_array().unwrap() {
        let embedding = p["embedding"].as_array().map(|a| {
            a.iter()
                .map(|x| x.as_f64().unwrap() as f32)
                .collect::<Vec<f32>>()
        });
        w.memories()
            .create(
                &MemCreate {
                    character_id: p["characterId"].as_str().unwrap().into(),
                    about_character_id: None,
                    chat_id: None,
                    project_id: None,
                    content: p["content"].as_str().unwrap().into(),
                    summary: p["summary"].as_str().unwrap().into(),
                    keywords: vec![],
                    tags: vec![],
                    importance: p["importance"].as_f64().unwrap(),
                    embedding,
                    source: p["source"].as_str().unwrap().into(),
                    witnessed_context: None,
                    occurred_at: opt_str(&p["occurredAt"]),
                    narrative_time: None,
                    entities: vec![],
                    kind: "semantic".into(),
                    source_message_id: None,
                    last_accessed_at: None,
                    reinforcement_count: p["reinforcementCount"].as_f64().unwrap(),
                    last_reinforced_at: opt_str(&p["lastReinforcedAt"]),
                    related_memory_ids: strs(&p["relatedMemoryIds"]),
                    reinforced_importance: p["importance"].as_f64().unwrap(),
                    tier: p["tier"].as_str().unwrap().into(),
                    superseded_by_id: opt_str(&p["supersededById"]),
                    superseded_at: opt_str(&p["supersededAt"]),
                    consolidated_from: strs(&p["consolidatedFrom"]),
                    consolidated_at: None,
                },
                &CreateOptions {
                    id: p["id"].as_str().unwrap().into(),
                    created_at: created_at.clone(),
                    updated_at: created_at.clone(),
                },
            )
            .expect("plant a memory");
    }
    w.connection()
        .execute_batch(&format!(
            "CREATE TRIGGER p4d266_planted_fold_failure BEFORE UPDATE ON memories \
             WHEN OLD.id = '{}' BEGIN SELECT RAISE(ABORT, 'planted fold failure'); END",
            spec["failTriggerSurvivorId"].as_str().unwrap()
        ))
        .expect("plant the fold-failure trigger");
}

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
                        // JS renders an integral REAL without a fraction.
                        c if NUM_COLS.contains(&c) => r
                            .get::<_, Option<f64>>(i)?
                            .map(|f| {
                                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 {
                                    Value::from(f as i64)
                                } else {
                                    serde_json::Number::from_f64(f)
                                        .map(Value::Number)
                                        .unwrap_or(Value::Null)
                                }
                            })
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

/// A fresh Db over a private scratch dir; the dir is deleted when this drops
/// (the `Db` field drops first).
struct ScratchDb {
    db: Db,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for ScratchDb {
    type Target = Db;
    fn deref(&self) -> &Db {
        &self.db
    }
}

fn open_fresh(mode: &str, spec: &Value) -> ScratchDb {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-dedup-{mode}-"))
        .tempdir()
        .expect("tempdir");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("embedding-profiles-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("embedding-profiles-mount.db"), &mount).unwrap();
    plant(&main, spec);
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db");
    ScratchDb { db, _dir: scratch }
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

    let spec = plants_spec();
    let canned: Vec<f32> = spec["cannedEmbedding"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap() as f32)
        .collect();

    let mut failed = false;
    for want in &oracle {
        let mode = want["mode"].as_str().unwrap();
        let dry_run = mode == "preview";
        let db = open_fresh(mode, &spec);

        // R-D: exactly the texts v4 was asked for answer the canned vector; a
        // text v5 asks for that v4 did not is a canned failure → a WARN + an
        // unchanged `embedding` cell → a visible table diff.
        let want_calls = strs(&want["embedCalls"]);
        let mut inner = CannedEmbeddingProvider::new();
        for text in &want_calls {
            inner = inner.with_vector(text.clone(), canned.clone());
        }
        let provider = Recording {
            inner,
            calls: Default::default(),
        };

        let mut result = rt
            .block_on(deduplicate_all_memories(
                &db,
                Some(&provider),
                USER_ID,
                THRESHOLD,
                dry_run,
            ))
            .expect("dedup");
        let got_calls = provider.calls.lock().unwrap().clone();
        if got_calls != want_calls {
            eprintln!(
                "[{mode} embedCalls] MISMATCH:\n  got:  {got_calls:?}\n  want: {want_calls:?}"
            );
            failed = true;
        } else {
            eprintln!("[{mode} embedCalls] OK ({} calls).", got_calls.len());
        }
        // Normalize the one non-deterministic field (both sides).
        result["processedAt"] = json!("<processedAt>");

        // BYTE-wise: a `preserve_order` map's `==` is order-blind.
        if serde_json::to_string(&result).unwrap()
            != serde_json::to_string(&want["result"]).unwrap()
        {
            eprintln!(
                "[{mode} result] MISMATCH:\n  got:  {}\n  want: {}",
                result, want["result"]
            );
            failed = true;
        } else {
            eprintln!("[{mode} result] OK.");
        }

        let memories = dump_memories(&db);
        if serde_json::to_string(&memories).unwrap()
            != serde_json::to_string(&want["memories"]).unwrap()
        {
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

    // Silent-stale guards: the fresh oracle carries the fold's columns and the
    // apply row really re-embedded.
    let apply = oracle.iter().find(|r| r["mode"] == "apply").unwrap();
    assert!(
        text.contains("reinforcementCount"),
        "no reinforcementCount in the oracle — a stale (pre-P4.D266) NDJSON?"
    );
    assert!(
        !strs(&apply["embedCalls"]).is_empty(),
        "the apply row recorded no re-embed — a stale oracle?"
    );
    assert!(!failed, "memory-dedup differential mismatched");
    eprintln!("OK: memory_dedup matched oracle (preview + apply).");
}
