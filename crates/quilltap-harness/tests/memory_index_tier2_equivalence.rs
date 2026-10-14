//! P4.D288 (C1-3) tier-2 differential: `services::memory_index` (the ONE writer
//! of `memories.embedding` / `.tier`) and the three new reads against v4's
//! REAL `memoryIndex` + repositories at the `83334b9a5` pin
//! (`harness/oracle/cases/memory-index-tier2.ts`), from the SAME starting file.
//!
//! Each op's RESULT and LINES are compared (from the message on — v5's target
//! is its module path; strings bare, the rest as JSON), then the end state of
//! `memories` and `background_jobs` (`quote(embedding)` byte-exact; minted ids
//! and timestamps masked — `supersededAt` / `updatedAt` stamped by a write, a
//! batch-enqueued job's id and stamps).
//!
//! **`MEMORY_INDEX_ALWAYS_LOADED` (L-2, a named both-ways table):** the oracle's
//! first op (`shape`) LOADS v4's cache, so every later op runs in v4's loaded
//! state and logs no load lines. v5 keeps no cache: every `shape` / `search`
//! reads the rows again and logs `Read memory index rows` + `Loaded vector
//! index from memory rows`. Those two lines are asserted PRESENT on v5's side
//! of every row's op kind and ABSENT on v4's (after the first op), then
//! dropped before the compare. The counts v4 derives from its cache agree
//! because the cache is loaded (`upsert`'s `cached`, `setTier`'s re-stamp,
//! `remove`'s `removed`); the UNLOADED arm (v4 `0`) is the recorded difference.
//!
//! Regenerate + run:
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-memory-index-tier2 /tmp/oracle-memory-index-tier2.ndjson
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MEMORY_INDEX_DIR=/tmp/qt-memory-index-tier2 \
//!     $N/npx tsx $V5W/harness/oracle/cases/memory-index-tier2.ts \
//!     > /tmp/oracle-memory-index-tier2.ndjson
//!   cd $V5W
//!   QT_ORACLE_MEMORY_INDEX_TIER2=/tmp/oracle-memory-index-tier2.ndjson \
//!   QT_FIXTURE_MEMORY_INDEX_DIR=/tmp/qt-memory-index-tier2 \
//!     cargo test -p quilltap-harness --test memory_index_tier2_equivalence -- --nocapture

mod common;

use std::collections::HashSet;

use quilltap_core::db::memories::{MemUpdate, TierExtra};
use quilltap_core::db::runtime::Db;
use quilltap_core::db::vector_store::Tier;
use quilltap_core::db::Writer;
use quilltap_core::services::memory_index::{
    self, IndexSearchOptions, MemoryIndexEntry, MemoryIndexUpsert,
};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "e2880000-0000-4000-8000-0000000000aa";
const CHAR: &str = "e2880000-0000-4000-8000-0000000000c1";
const T0: &str = "2026-01-01T00:00:00.000Z";

/// `MEMORY_INDEX_ALWAYS_LOADED`: `(op kind, the lines v5's per-read load adds)`.
const MEMORY_INDEX_ALWAYS_LOADED: &[(&str, &[&str])] = &[(
    "search",
    &[
        "Read memory index rows",
        "Loaded vector index from memory rows",
    ],
)];

fn unit(v: &[f64]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    v.iter().map(|x| (x / norm) as f32).collect()
}

/// v4's lines as `LEVEL message k=v …` — the standing classes applied: v4's
/// backend `Raw query failed` (v4-only, no `rawQuery` layer in v5) dropped, and
/// an `error` carrying SQLite's quoted-identifier hint mapped onto v5's bare
/// text ([`common::normalize_v4_sqlite`], the column name still compared).
fn want_lines(op: &Value) -> Vec<String> {
    op["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["message"] != "Raw query failed")
        .map(|l| {
            let mut out = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                let rendered = match v {
                    Value::String(s) if k == "error" => common::normalize_v4_sqlite(s),
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                out.push_str(&format!(" {k}={rendered}"));
            }
            out
        })
        .collect()
}

/// v5's captured line minus its target.
fn got_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap().to_string();
            let _target = parts.next();
            format!("{level} {}", parts.next().unwrap_or_default())
        })
        .collect()
}

fn hits_json(hits: Vec<quilltap_core::db::vector_store::VectorSearchResult>) -> Value {
    Value::Array(
        hits.into_iter()
            .map(|h| json!({ "id": h.id, "score": h.score, "cold": h.tier == Tier::Cold }))
            .collect(),
    )
}

fn ids_of(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

fn tier_of(s: &str) -> Tier {
    if s == "cold" {
        Tier::Cold
    } else {
        Tier::Hot
    }
}

/// Scores within 1e-12; everything else exact.
fn same_result(got: &Value, want: &Value) -> bool {
    match (got, want) {
        (Value::Number(a), Value::Number(b)) => {
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() <= 1e-12
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_result(x, y))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|w| same_result(v, w)))
        }
        _ => got == want,
    }
}

/// `w` is a writer opened on THIS thread — the index's writes log on the
/// caller thread, where the capture sees them (`Db::write` runs its closure on
/// the writer thread, invisible to a thread-local capture buffer).
async fn run_op(db: &Db, w: &Writer, op: &Value) -> Value {
    let input = &op["input"];
    match op["op"].as_str().unwrap() {
        "shape" => {
            let s = db
                .read_main(|c| Ok(memory_index::shape(c, CHAR, 3)))
                .unwrap();
            json!({ "size": s.size, "dominantDimensions": s.dominant_dimensions, "matching": s.matching })
        }
        "search" => {
            let exclude: HashSet<String> = ids_of(&input["exclude"]).into_iter().collect();
            let opts = IndexSearchOptions {
                hot_only: input["hotOnly"].as_bool().unwrap(),
                exclude: (!input["exclude"].is_null()).then_some(&exclude),
                limit: input["limit"].as_u64().unwrap() as usize,
            };
            let q = unit(&[1.0, 0.0, 0.0]);
            hits_json(
                db.read_main(|c| Ok(memory_index::search(c, CHAR, &q, opts)))
                    .unwrap(),
            )
        }
        "upsert" => {
            let entries: Vec<MemoryIndexUpsert> = input
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    let v: Vec<f64> = e["v"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_f64().unwrap())
                        .collect();
                    let mut alongside = MemUpdate::default();
                    if let Some(a) = e.get("alongside") {
                        alongside.content = a["content"].as_str().map(str::to_string);
                        alongside.summary = a["summary"].as_str().map(str::to_string);
                    }
                    MemoryIndexUpsert {
                        id: e["id"].as_str().unwrap().to_string(),
                        embedding: unit(&v),
                        tier: None,
                        alongside,
                    }
                })
                .collect();
            let found = memory_index::upsert(w.connection(), CHAR, entries).unwrap();
            json!(found)
        }
        "setTier" => {
            let ids = ids_of(&input["ids"]);
            let tier = tier_of(input["tier"].as_str().unwrap());
            let extra = &input["extra"];
            let extra = TierExtra {
                superseded_by_id: extra
                    .get("supersededById")
                    .map(|v| v.as_str().map(str::to_string)),
                consolidated_at: extra
                    .get("consolidatedAt")
                    .map(|v| v.as_str().map(str::to_string)),
            };
            let changed = memory_index::set_tier(w.connection(), CHAR, &ids, tier, extra).unwrap();
            json!(changed)
        }
        "remove" => {
            let ids = ids_of(&input["ids"]);
            let n = ids.len();
            let removed = db
                .read_main(|c| memory_index::indexed_among(c, CHAR, &ids))
                .unwrap();
            json!(memory_index::remove(CHAR, n, removed))
        }
        "countCorpusSince" => {
            let since = input["since"].as_str().map(str::to_string);
            let (total, created) = db
                .read_main(move |c| {
                    quilltap_core::db::memories_read::count_corpus_since(c, since.as_deref())
                })
                .unwrap();
            json!({ "total": total, "createdSince": created })
        }
        "findInstanceDefault" => {
            let p = db
                .read_main(quilltap_core::db::embedding_profiles::find_instance_default)
                .unwrap();
            p.map_or(Value::Null, |p| json!(p.id))
        }
        "findActiveByType" => {
            let jobs = db
                .read_main(|c| {
                    quilltap_core::db::background_jobs::BackgroundJobsRepository::new(c)
                        .find_active_by_type("EMBEDDING_GENERATE")
                })
                .unwrap();
            // The oracle's second read counts; the first lists ids.
            if op["result"].is_number() {
                json!(jobs.len())
            } else {
                json!(jobs.iter().map(|j| j.id.clone()).collect::<Vec<_>>())
            }
        }
        "enqueueMissingEmbeddings" => {
            let cid = input["characterId"].as_str();
            memory_index::enqueue_missing_embeddings(
                db,
                USER,
                cid,
                input["limit"].as_i64().unwrap(),
            )
            .await
            .unwrap()
            .to_json()
        }
        "indexInserted" => {
            let entries: Vec<MemoryIndexEntry> = ids_of(&input["ids"])
                .into_iter()
                .map(|id| MemoryIndexEntry {
                    id,
                    embedding: unit(&[0.9, 0.1, 0.0]),
                    tier: None,
                })
                .collect();
            memory_index::index_inserted(CHAR, &entries);
            Value::Null
        }
        "plant" => {
            // The fallback arms' failure plant — the same SQL as v4's side.
            w.connection()
                .execute_batch(input.as_str().unwrap())
                .expect("plant");
            Value::Null
        }
        other => panic!("unknown op {other}"),
    }
}

/// The rows the oracle planted between ops (v4 wrote them through its
/// repositories; the same cells here).
async fn plant_profile_and_active_job(db: &Db) {
    db.write(|w| {
        w.main().connection().execute_batch(
            "INSERT INTO embedding_profiles (id, userId, name, provider, modelName, isDefault, createdAt, updatedAt) \
             VALUES ('e2880000-0000-4000-8000-0000000000e1', 'e2880000-0000-4000-8000-0000000000aa', 'Default', \
                     'OPENAI', 'text-embedding-3-small', 1, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'); \
             INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, \
                     scheduledAt, startedAt, completedAt, createdAt, updatedAt) \
             VALUES ('e2880000-0000-4000-8000-0000000000f1', 'e2880000-0000-4000-8000-0000000000aa', 'EMBEDDING_GENERATE', \
                     'PENDING', '{\"entityType\":\"MEMORY\",\"entityId\":\"e2880000-0000-4000-8000-000000000005\",\
\"characterId\":\"e2880000-0000-4000-8000-0000000000c1\",\"profileId\":\"e2880000-0000-4000-8000-0000000000e1\"}', \
                     10, 0, 3, NULL, '2026-01-01T00:00:00.000Z', NULL, NULL, '2026-01-01T00:00:00.000Z', \
                     '2026-01-01T00:00:00.000Z');",
        )?;
        Ok(())
    })
    .await
    .unwrap();
}

/// The end-state dump, minted values masked.
fn end_state(db: &Db) -> (Value, Value) {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            "SELECT id, characterId, content, summary, tier, supersededById, supersededAt, consolidatedAt, \
                    quote(embedding) AS embedding, createdAt, updatedAt FROM memories ORDER BY id",
        )?;
        let memories: Vec<Value> = stmt
            .query_map([], |r| {
                let mut o = serde_json::Map::new();
                for (i, name) in [
                    "id", "characterId", "content", "summary", "tier", "supersededById",
                    "supersededAt", "consolidatedAt", "embedding", "createdAt", "updatedAt",
                ]
                .iter()
                .enumerate()
                {
                    o.insert((*name).to_string(), json!(r.get::<_, Option<String>>(i)?));
                }
                Ok(Value::Object(o))
            })?
            .collect::<Result<_, _>>()?;
        let mut stmt = c.prepare(
            "SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, scheduledAt, createdAt, updatedAt \
               FROM background_jobs ORDER BY createdAt, payload",
        )?;
        let jobs: Vec<Value> = stmt
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "userId": r.get::<_, String>(1)?,
                    "type": r.get::<_, String>(2)?,
                    "status": r.get::<_, String>(3)?,
                    "payload": r.get::<_, String>(4)?,
                    "priority": r.get::<_, f64>(5)?,
                    "attempts": r.get::<_, f64>(6)?,
                    "maxAttempts": r.get::<_, f64>(7)?,
                    "scheduledAt": r.get::<_, String>(8)?,
                    "createdAt": r.get::<_, String>(9)?,
                    "updatedAt": r.get::<_, String>(10)?,
                }))
            })?
            .collect::<Result<_, _>>()?;
        Ok((Value::Array(memories), Value::Array(jobs)))
    })
    .unwrap()
}

/// Minted values → placeholders, on BOTH sides: any stamp that is not the
/// planted `T0`, and a job id that is not a planted one.
fn mask(rows: &Value, stamp_keys: &[&str], mint_id: bool) -> Value {
    Value::Array(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let mut o = row.as_object().unwrap().clone();
                for k in stamp_keys {
                    if let Some(Value::String(s)) = o.get(*k) {
                        if s != T0 {
                            o.insert((*k).to_string(), json!("<ts>"));
                        }
                    }
                }
                if mint_id && !o["id"].as_str().unwrap().starts_with("e2880000-") {
                    o.insert("id".into(), json!("<minted>"));
                }
                // Numbers compare as numbers (v4's JSON vs v5's f64).
                for (_, v) in o.iter_mut() {
                    if let Some(f) = v.as_f64() {
                        *v = json!(f);
                    }
                }
                Value::Object(o)
            })
            .collect(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn memory_index_matches_v4() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MEMORY_INDEX_TIER2") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_INDEX_TIER2 to the oracle NDJSON (see header).");
        return;
    };
    let Ok(dir) = std::env::var("QT_FIXTURE_MEMORY_INDEX_DIR") else {
        eprintln!("SKIP: set QT_FIXTURE_MEMORY_INDEX_DIR to the case's dir (see header).");
        return;
    };
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("oracle line");
    global_capture::install();
    let scratch = tempfile::tempdir().unwrap();
    let work = scratch.path().join("main.db");
    std::fs::copy(std::path::Path::new(&dir).join("base.db"), &work).expect("copy base.db");
    let db = Db::open_main(&work, PEPPER).expect("open");
    let writer = Writer::open_writable(&work, PEPPER).expect("open the caller-thread writer");

    let mut failures = Vec::new();
    let mut always_loaded_met = 0usize;
    let mut planted = false;
    let mut end: Option<(Value, Value)> = None;
    for (i, op) in oracle["ops"].as_array().unwrap().iter().enumerate() {
        let kind = op["op"].as_str().unwrap();
        // The end state is taken where v4 takes it: before the first failure
        // plant drops a column.
        if kind == "plant" && end.is_none() {
            end = Some(end_state(&db));
        }
        // v4 planted the profile + the active job right before its first
        // `findInstanceDefault` that answers a profile.
        if !planted && kind == "findInstanceDefault" && op["result"].is_string() {
            plant_profile_and_active_job(&db).await;
            planted = true;
        }
        let (got, lines) = global_capture::capture_async(run_op(&db, &writer, op)).await;
        let mut got_lines = got_lines(lines);
        let want_lines = want_lines(op);
        if let Some((_, extra)) = MEMORY_INDEX_ALWAYS_LOADED.iter().find(|(k, _)| *k == kind) {
            for line in *extra {
                assert!(
                    got_lines.iter().any(|l| l.contains(line)),
                    "op {i} ({kind}): v5 no longer reloads — retire MEMORY_INDEX_ALWAYS_LOADED: {got_lines:?}"
                );
                assert!(
                    !want_lines.iter().any(|l| l.contains(line)),
                    "op {i} ({kind}): v4's cache was not loaded: {want_lines:?}"
                );
            }
            got_lines.retain(|l| !extra.iter().any(|e| l.contains(e)));
            always_loaded_met += 1;
        }
        let want = op.get("result").cloned().unwrap_or(Value::Null);
        if !same_result(&got, &want) {
            failures.push(format!("op {i} ({kind}): result\n  v5 {got}\n  v4 {want}"));
        }
        if got_lines != want_lines {
            failures.push(format!(
                "op {i} ({kind}): lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
    }
    let (memories, jobs) = end.expect("the fallback arms ran");
    let m_keys = ["supersededAt", "updatedAt"];
    let j_keys = ["scheduledAt", "createdAt", "updatedAt"];
    if mask(&memories, &m_keys, false) != mask(&oracle["memories"], &m_keys, false) {
        failures.push(format!(
            "memories end state\n  v5 {}\n  v4 {}",
            mask(&memories, &m_keys, false),
            mask(&oracle["memories"], &m_keys, false)
        ));
    }
    if mask(&jobs, &j_keys, true) != mask(&oracle["jobs"], &j_keys, true) {
        failures.push(format!(
            "background_jobs end state\n  v5 {}\n  v4 {}",
            mask(&jobs, &j_keys, true),
            mask(&oracle["jobs"], &j_keys, true)
        ));
    }
    assert!(planted, "the planted profile + job ran");
    assert!(
        always_loaded_met >= 5,
        "MEMORY_INDEX_ALWAYS_LOADED rows met: {always_loaded_met}"
    );
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!(
        "OK: memory_index matched v4 over {} ops ({} always-loaded rows).",
        oracle["ops"].as_array().unwrap().len(),
        always_loaded_met
    );
}
